use crate::{
    CAT, DAN, DEU, ELL, ENG, FRA, ISL, ITA, LIT, NLD, NOR, POR, SPA, SWE, TUR,
    context::Context,
    lang::{Lang, LangEntry},
    stage::{FusedIterator, Stage, StageError, StaticFusableStage},
    testing::stage_contract::StageTestConfig,
};
use std::borrow::Cow;

/// Locale-sensitive case folding for search and comparison.
///
/// `CaseFold` performs full Unicode case folding with language-specific rules,
/// including:
/// - Multi-character expansions (e.g. German `ß` → `"ss"`, `ẞ` → `"ss"`)
/// - Context-sensitive mappings via peek-ahead (e.g. Dutch `IJ` → `"ij"`)
/// - Locale-aware lowercase mapping using `case_map` (e.g. Turkish `İ` → `i`, `I` → `ı`)
/// - Fallback to Unicode full case folding (`.to_lowercase()` + compatibility mappings)
///
/// This stage is intended for information retrieval, search indexing, and any
/// scenario requiring case-insensitive matching that respects linguistic norms.
/// It is stronger than simple lowercasing but weaker than NFKC/NFKD.
///
/// When the target language has only one-to-one mappings and no peek-ahead rules,
/// this stage implements `CharMapper`, enabling zero-allocation pipeline fusion.
pub struct CaseFold;

impl Stage for CaseFold {
    fn name(&self) -> &'static str {
        "case_fold"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        if text.is_ascii() {
            return Ok(text.bytes().any(|b| b.is_ascii_uppercase()));
        }
        // Check if any character needs case folding
        if text.chars().any(|c| ctx.lang_entry.needs_case_fold(c)) {
            return Ok(true);
        }

        // If language requires peek-ahead, check for context-sensitive rules
        if ctx.lang_entry.requires_peek_ahead() {
            let mut chars = text.chars().peekable();
            while let Some(c) = chars.next() {
                if ctx
                    .lang_entry
                    .get_peek_fold(c, chars.peek().copied())
                    .is_some()
                {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // 1. Handle peek-ahead languages (Dutch, Greek)
        if ctx.lang_entry.requires_peek_ahead() {
            return apply_with_peek_ahead(text, ctx);
        }

        // 2. Fast path: Manual loop (The "Manual Slow-Path" Optimization)
        let cap = (text.len() as f64 * 1.1) as usize;
        let mut out = String::with_capacity(cap);

        for c in text.chars() {
            // Priority 1: Multi-char expansions (ß -> ss)
            if let Some(to) = ctx.lang_entry.find_fold_map(c) {
                out.push_str(to);
                continue;
            }
            // Priority 2: Language specific 1:1 (Turkish İ -> i)
            if let Some(to) = ctx.lang_entry.find_case_map(c) {
                out.push(to);
                continue;
            }
            // Priority 3: Unicode Standard Fallback
            out.extend(c.to_lowercase());
        }
        Ok(Cow::Owned(out))
    }
}

impl StaticFusableStage for CaseFold {
    type Adapter<'a, I>
        = CaseFoldAdapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    fn supports_static_fusion(&self) -> bool {
        true
    }

    #[inline(always)]
    fn static_fused_adapter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::Adapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        CaseFoldAdapter {
            input,
            lang: &ctx.lang_entry,
            pending: None,
        }
    }
}

/// Universal adapter for case folding.
/// Works for both Static Fusion (Generic I) and Dynamic Fusion (I = Box<dyn ...>).
pub struct CaseFoldAdapter<'a, I> {
    input: I,
    lang: &'a LangEntry,
    /// Buffer for multi-character expansions (e.g., "ss" from ß)
    pending: Option<&'a str>,
}

impl<'a, I> Iterator for CaseFoldAdapter<'a, I>
where
    I: Iterator<Item = char>,
{
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        // 1. Handle pending characters (unchanged logic)
        if let Some(pending_str) = self.pending {
            let mut chars = pending_str.chars();
            let first = chars.next().unwrap();
            let rest = chars.as_str();

            if rest.is_empty() {
                self.pending = None;
            } else {
                self.pending = Some(rest);
            }
            return Some(first);
        }

        let c = self.input.next()?;

        // 2. 1:N expansions (German ß -> ss)
        if let Some(to) = self.lang.find_fold_map(c) {
            let mut chars = to.chars();
            let first = chars.next().unwrap();
            let rest = chars.as_str();
            if !rest.is_empty() {
                self.pending = Some(rest);
            }
            return Some(first);
        }

        // 3. 1:1 mapping (Turkish İ -> i)
        if let Some(to) = self.lang.find_case_map(c) {
            return Some(to);
        }

        // 4. Unicode Fallback
        let mut lowercase = c.to_lowercase();
        let first = lowercase.next().unwrap_or(c);

        // Note: We could buffer the rest of `lowercase` here if needed,
        // but for current Normy languages, find_fold_map covers the expansions.

        Some(first)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<'a, I: FusedIterator<Item = char>> FusedIterator for CaseFoldAdapter<'a, I> {}

fn apply_with_peek_ahead<'a>(
    text: Cow<'a, str>,
    ctx: &Context,
) -> Result<Cow<'a, str>, StageError> {
    // Use capacity hint for allocation
    let (fold_count, extra_bytes) = ctx.lang_entry.hint_capacity_fold(&text);
    let capacity = text.len() + extra_bytes + (fold_count * 2);
    let mut out = String::with_capacity(capacity);
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        // 1. Check for peek-ahead rules first (e.g., Dutch IJ → ij)
        if let Some(target) = ctx.lang_entry.get_peek_fold(c, chars.peek().copied()) {
            // Consume the peeked character.
            // Note: The original code had an expensive byte-level comparison
            // to check if it 'changed'. We can now skip that.
            chars.next().unwrap();
            out.push_str(target);
            continue;
        }

        // 2. Check fold_map for multi-char expansions
        if let Some(to) = ctx.lang_entry.find_fold_map(c) {
            out.push_str(to);
            continue;
        }

        // Check case_map (language-specific 1:1 mappings, e.g., Turkish İ→i)
        if let Some(to) = ctx.lang_entry.find_case_map(c) {
            out.push(to);
            continue;
        }

        // 4. Fallback to Unicode lowercase
        out.extend(c.to_lowercase());
    }

    // Since needs_apply returned true, we MUST return an Owned Cow.
    Ok(Cow::Owned(out))
}

impl StageTestConfig for CaseFold {
    fn one_to_one_languages() -> &'static [Lang] {
        &[
            ENG, FRA, SPA, ITA, POR, DAN, NOR, SWE, ISL, CAT, TUR, LIT, ELL,
        ]
    }

    fn samples(lang: Lang) -> &'static [&'static str] {
        match lang {
            TUR => &["İSTANBUL", "I", "İ", "ısı", "i"],
            DEU => &["Straße", "GROẞ", "Fuß"],
            NLD => &["IJssel", "Ĳssel", "ijssel", "Ij"],
            ELL => &["ΣΟΦΟΣ", "ΟΔΟΣ", "Σ"],
            LIT => &["JIS", "Jį", "ĄČĘĖ"],
            _ => &["Hello WORLD", "Test 123", " café ", "NAÏVE"],
        }
    }

    fn should_pass_through(lang: Lang) -> &'static [&'static str] {
        match lang {
            TUR => &["ısı", "i", "istanbul", "hello"],
            DEU => &["strasse", "gross", "hello", "test"],
            NLD => &["ijssel", "hello", "world"],
            ELL => &["σοφοσ", "οδοσ", "hello"],
            LIT => &["jis", "jį", "hello"],
            _ => &["hello", "world", "test123", ""],
        }
    }

    fn should_transform(lang: Lang) -> &'static [(&'static str, &'static str)] {
        match lang {
            TUR => &[
                ("İ", "i"),
                ("I", "ı"),
                ("İSTANBUL", "istanbul"),
                ("ISI", "ısı"),
            ],
            DEU => &[
                ("ß", "ss"),
                ("ẞ", "ss"),
                ("Straße", "strasse"),
                ("GROẞ", "gross"),
            ],
            NLD => &[
                ("IJ", "ij"),
                ("Ĳ", "ij"),
                ("ĳ", "ij"),
                ("IJssel", "ijssel"),
                ("Ij", "ij"),
            ],
            ELL => &[("Σ", "σ"), ("ΣΟΦΟΣ", "σοφοσ"), ("ΟΔΟΣ", "οδοσ")],
            LIT => &[("JIS", "jis"), ("JĮ", "jį"), ("Ė", "ė")],
            _ => &[
                ("HELLO", "hello"),
                ("World", "world"),
                ("NAÏVE", "naïve"),
                ("ABC", "abc"),
            ],
        }
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;

    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(CaseFold);
    }
}
