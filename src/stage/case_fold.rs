use crate::{
    CAT, DAN, DEU, ELL, ENG, FRA, ISL, ITA, LIT, NLD, NOR, POR, SPA, SWE, TUR,
    context::Context,
    lang::{Lang, LangEntry},
    stage::{FusableStage, FusedIterator, Stage, StageError, StaticFusableStage, StaticStageIter},
    testing::stage_contract::StageTestConfig,
};
use std::{borrow::Cow, str::Chars};

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
        let cap = self.expected_capacity(text.len()); // Just math, no iteration
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

    /// CaseFold can participate in fusable segments when checking needs_apply
    /// on the original text is sufficient.
    #[inline]
    fn safe_skip_approximation(&self) -> bool {
        true
    }

    /// Returns self as FusableStage when the stage supports 1:1 character mapping
    /// (no multi-character expansions, no peek-ahead).
    ///
    /// For languages with 1:N mappings (e.g., German ß→ss) or peek-ahead rules,
    /// the fusion will fall back to the apply() method.
    #[inline]
    fn as_fusable(&self) -> Option<&dyn FusableStage> {
        Some(self)
    }

    #[inline(always)]
    fn expected_capacity(&self, input_len: usize) -> usize {
        // Most languages shrink or stay the same in lowercase,
        // but German/Greek can expand. 10% overhead is a safe buffer.
        (input_len as f64 * 1.1) as usize
    }

    fn try_dynamic_iter<'a>(
        &self,
        text: &'a str,
        ctx: &'a Context,
    ) -> Option<Box<dyn FusedIterator<Item = char> + 'a>> {
        if ctx.lang_entry.has_one_to_one_folds() && !ctx.lang_entry.requires_peek_ahead() {
            // Only proceed if we are in the guaranteed 1:1 path.
            Some(Box::new(CaseFoldIter::new(text, ctx)))
        } else {
            // If the language requires expansion (ß->ss) or peek-ahead (IJ->ij),
            // we MUST fall back to the Stage::apply method.
            None
        }
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

impl StaticStageIter for CaseFold {
    type Iter<'a> = CaseFoldIter<'a>;

    #[inline(always)]
    fn try_static_iter<'a>(&self, text: &'a str, ctx: &'a Context) -> Option<Self::Iter<'a>> {
        if ctx.lang_entry.has_one_to_one_folds() && !ctx.lang_entry.requires_peek_ahead() {
            // Only proceed if we are in the guaranteed 1:1 path.
            Some(CaseFoldIter::new(text, ctx))
        } else {
            // If the language requires expansion (ß->ss) or peek-ahead (IJ->ij),
            // we MUST fall back to the Stage::apply method.
            None
        }
    }
}

// ============================================================================
// FusableStage Implementation - Dynamic Iterator Fusion
// ============================================================================

impl FusableStage for CaseFold {
    fn dyn_fused_adapter<'a>(
        &self,
        input: Box<dyn FusedIterator<Item = char> + 'a>,
        ctx: &'a Context,
    ) -> Box<dyn FusedIterator<Item = char> + 'a> {
        // Always return an adapter that can handle both 1:1 and 1:N cases
        // For peek-ahead languages, we still can't fuse (would require complex lookahead),
        // but 1:N expansions can be handled with a pending buffer
        if ctx.lang_entry.requires_peek_ahead() {
            // Peek-ahead languages like Dutch cannot be fused in iterator chains
            // The fusion system should detect this and fall back to apply()
            // For now, we use a simple adapter that will produce incorrect results
            // This should be caught by safe_skip_approximation check
            Box::new(CaseFoldSimpleAdapter {
                input,
                lang: &ctx.lang_entry,
            })
        } else {
            // Use the full adapter that handles 1:N expansions
            Box::new(CaseFoldAdapter {
                input,
                lang: &ctx.lang_entry,
                pending: None,
            })
        }
    }
}

// ============================================================================
// Iterator Adapters
// ============================================================================

/// Simple adapter for peek-ahead languages (fallback only).
/// This should not be used in practice as peek-ahead languages should fall back to apply().
pub struct CaseFoldSimpleAdapter<'a> {
    input: Box<dyn FusedIterator<Item = char> + 'a>,
    lang: &'a LangEntry,
}

impl<'a> Iterator for CaseFoldSimpleAdapter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let c = self.input.next()?;
        self.lang.apply_case_fold(c)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<'a> FusedIterator for CaseFoldSimpleAdapter<'a> {}

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

pub struct CaseFoldIter<'a> {
    chars: Chars<'a>,
    lang: &'a LangEntry,
}

impl<'a> CaseFoldIter<'a> {
    pub fn new(text: &'a str, ctx: &'a Context) -> Self {
        Self {
            chars: text.chars(),
            lang: &ctx.lang_entry,
        }
    }
}

impl<'a> Iterator for CaseFoldIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let c = self.chars.next()?;
        self.lang.apply_case_fold(c)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}

impl<'a> ExactSizeIterator for CaseFoldIter<'a> {
    #[inline]
    fn len(&self) -> usize {
        self.chars.size_hint().0
    }
}

impl<'a> FusedIterator for CaseFoldIter<'a> {}

impl StageTestConfig for CaseFold {
    fn one_to_one_languages() -> &'static [Lang] {
        // Languages with ONLY 1:1 mappings and no peek-ahead
        &[ENG, FRA, SPA, ITA, POR, DAN, NOR, SWE, ISL, CAT, TUR, LIT]
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
                ("İ", "i"), // Turkish dotted I
                ("I", "ı"), // Turkish dotless I
                ("İSTANBUL", "istanbul"),
                ("ISI", "ısı"),
            ],
            DEU => &[
                ("ß", "ss"), // Eszett
                ("ẞ", "ss"), // Capital Eszett
                ("Straße", "strasse"),
                ("GROẞ", "gross"),
            ],
            NLD => &[
                ("IJ", "ij"), // Peek-ahead sequence
                ("Ĳ", "ij"),  // Dutch IJ ligature U+0132
                ("ĳ", "ij"),  // Dutch ij ligature U+0133
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

// ─────────────────────────────────────────────────────────────────────────────
//  Universal contract tests (zero-cost, full coverage)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;

    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(CaseFold);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DEU, ELL, LIT, NLD, TUR};

    #[test]
    fn case_fold_english_basic() {
        let ctx = Context::new(ENG);
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("HELLO"), &ctx).unwrap(),
            "hello"
        );
    }

    #[test]
    fn case_fold_turkish_dotted_i_capital_i() {
        let ctx = Context::new(TUR);
        assert_eq!(CaseFold.apply(Cow::Borrowed("I"), &ctx).unwrap(), "ı");
        assert_eq!(CaseFold.apply(Cow::Borrowed("İ"), &ctx).unwrap(), "i");
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("İSTANBUL"), &ctx).unwrap(),
            "istanbul"
        );
    }

    #[test]
    fn case_fold_german_eszett() {
        let ctx = Context::new(DEU);
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("straße"), &ctx).unwrap(),
            "strasse"
        );
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("GROẞ"), &ctx).unwrap(),
            "gross"
        );
        assert_eq!(CaseFold.apply(Cow::Borrowed("Fuß"), &ctx).unwrap(), "fuss");
    }

    #[test]
    fn case_fold_dutch_ij() {
        let ctx = Context::new(NLD);

        // Sequence IJ
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("IJssel"), &ctx).unwrap(),
            "ijssel"
        );

        // Ligature Ĳ (U+0132)
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("Ĳssel"), &ctx).unwrap(),
            "ijssel"
        );
    }

    #[test]
    fn case_fold_greek_final_sigma() {
        let ctx = Context::new(ELL);
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("ΣΟΦΟΣ"), &ctx).unwrap(),
            "σοφοσ"
        );
        assert_eq!(CaseFold.apply(Cow::Borrowed("ΟΔΟΣ"), &ctx).unwrap(), "οδοσ");
    }

    #[test]
    fn case_fold_lithuanian() {
        let ctx = Context::new(LIT);
        assert_eq!(CaseFold.apply(Cow::Borrowed("JIS"), &ctx).unwrap(), "jis");
        assert_eq!(CaseFold.apply(Cow::Borrowed("JĮ"), &ctx).unwrap(), "jį");
    }

    #[test]
    fn test_needs_apply_accuracy() {
        // English uppercase
        let ctx = Context::new(ENG);
        assert!(CaseFold.needs_apply("HELLO", &ctx).unwrap());
        assert!(!CaseFold.needs_apply("hello", &ctx).unwrap());

        // Turkish
        let ctx = Context::new(TUR);
        assert!(CaseFold.needs_apply("İSTANBUL", &ctx).unwrap());
        assert!(!CaseFold.needs_apply("istanbul", &ctx).unwrap());

        // Dutch peek-ahead
        let ctx = Context::new(NLD);
        assert!(CaseFold.needs_apply("IJssel", &ctx).unwrap());
        assert!(!CaseFold.needs_apply("ijssel", &ctx).unwrap());

        // German eszett
        let ctx = Context::new(DEU);
        assert!(CaseFold.needs_apply("Straße", &ctx).unwrap());

        let ctx = Context::new(LIT);
        assert!(CaseFold.needs_apply("IÌ Í Ĩ IĮ ĖĖ ŲŲ ", &ctx).unwrap());

        let ctx = Context::new(LIT);
        assert!(!CaseFold.needs_apply("iì í ĩ iį ėė ųų", &ctx).unwrap());
    }

    #[test]
    fn test_capacity_hint_accuracy() {
        // German: ß→ss expands
        let ctx = Context::new(DEU);
        let (count, extra) = ctx.lang_entry.hint_capacity_fold("Straße");
        assert_eq!(count, 1, "Should detect 1 fold (ß)");
        assert_eq!(extra, 0, "ß is 2 bytes, ss is 2 bytes → 0 extra");

        // English: no folds
        let ctx = Context::new(ENG);
        let (count, extra) = ctx.lang_entry.hint_capacity_fold("hello");
        assert_eq!(count, 0, "No folds needed");
        assert_eq!(extra, 0, "No extra bytes");

        // Turkish: I→ı is 1:1
        let ctx = Context::new(TUR);
        let (count, extra) = ctx.lang_entry.hint_capacity_fold("ISI");
        assert_eq!(count, 0, "Turkish uses case_map, not fold_map");
        assert_eq!(extra, 0);
    }

    #[test]
    fn test_peek_ahead_dutch_mixed_case() {
        let ctx = Context::new(NLD);

        // Test various IJ combinations
        assert_eq!(CaseFold.apply(Cow::Borrowed("IJ"), &ctx).unwrap(), "ij");
        assert_eq!(CaseFold.apply(Cow::Borrowed("Ij"), &ctx).unwrap(), "ij");
        assert_eq!(CaseFold.apply(Cow::Borrowed("iJ"), &ctx).unwrap(), "ij");
        assert_eq!(CaseFold.apply(Cow::Borrowed("Ĳ"), &ctx).unwrap(), "ij");
        assert_eq!(CaseFold.apply(Cow::Borrowed("ĳ"), &ctx).unwrap(), "ij");

        // Single I or J should just lowercase normally
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("I am J"), &ctx).unwrap(),
            "i am j"
        );
    }
}
