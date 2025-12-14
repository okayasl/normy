use crate::{
    CAT, DAN, DEU, ELL, ENG, FRA, ISL, ITA, LIT, NLD, NOR, POR, SPA, SWE, TUR,
    context::Context,
    lang::{Lang, LangEntry},
    stage::{FusedIterator, Stage, StageError, StaticStageIter},
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
        // PERF: No need to check for 'changed' flag here, as needs_apply()
        // guarantees a change will occur. We MUST allocate a new string.

        // 1. Handle peek-ahead languages (Dutch, Greek)
        if ctx.lang_entry.requires_peek_ahead() {
            // Note: This function still needs to be refined to avoid redundant checks,
            // but its implementation is separated for complexity.
            return apply_with_peek_ahead(text, ctx);
        }

        // 2. Fast path: 1:N or 1:1 fold languages (most common)

        // Use hint to estimate capacity.
        let (_fold_count, extra_bytes) = ctx.lang_entry.hint_capacity_fold(&text);

        // Allocate a new string with estimated capacity
        let mut out = String::with_capacity(text.len() + extra_bytes);

        // Create the iterator that handles all the folding logic (excluding peek-ahead).
        // Note: We use a custom local iterator for simplicity and to handle the
        // 1:N (multi-char) expansions that the simple CaseFoldIter (which is 1:1) cannot handle.

        for c in text.chars() {
            // Check fold_map first (language-specific multi-char expansions, e.g., German ß→ss)
            if let Some(to) = ctx.lang_entry.find_fold_map(c) {
                out.push_str(to);
                continue;
            }

            // Check case_map (language-specific 1:1 mappings, e.g., Turkish İ→i)
            if let Some(to) = ctx.lang_entry.find_case_map(c) {
                out.push(to);
                continue;
            }

            // Fallback to Unicode full case folding/lowercase.
            out.extend(c.to_lowercase());
        }

        // Since needs_apply returned true, we MUST return an Owned Cow.
        Ok(Cow::Owned(out))
    }

    // #[inline]
    // fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
    //     // Only eligible if:
    //     // 1. All folds are one-to-one (no ß→ss expansions)
    //     // 2. No peek-ahead rules (no Dutch IJ or Greek final sigma)
    //     if ctx.lang_entry.has_one_to_one_folds() && !ctx.lang_entry.requires_peek_ahead() {
    //         Some(self)
    //     } else {
    //         None
    //     }
    // }

    // #[inline]
    // fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
    //     if ctx.lang_entry.has_one_to_one_folds() && !ctx.lang_entry.requires_peek_ahead() {
    //         Some(self)
    //     } else {
    //         None
    //     }
    // }

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

// impl CharMapper for CaseFold {
//     #[inline(always)]
//     fn map(&self, c: char, ctx: &Context) -> Option<char> {
//         // Delegate to LangEntry's unified method
//         ctx.lang_entry.apply_case_fold(c)
//     }

//     fn bind<'a>(
//         &self,
//         text: &'a str,
//         ctx: &'a Context,
//     ) -> Box<dyn FusedIterator<Item = char> + 'a> {
//         Box::new(CaseFoldIter::new(text, ctx))
//     }
// }

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
    fn test_precomputed_flags() {
        let ctx_eng = Context::new(ENG);
        let ctx_deu = Context::new(DEU);
        let ctx_nld = Context::new(NLD);

        // English: no fold_map, no peek-ahead
        assert!(!ctx_eng.lang_entry.has_fold_map());
        assert!(!ctx_eng.lang_entry.requires_peek_ahead());
        assert!(ctx_eng.lang_entry.has_one_to_one_folds());

        // German: has fold_map (ß→ss), NOT one-to-one
        assert!(ctx_deu.lang_entry.has_fold_map());
        assert!(!ctx_deu.lang_entry.has_one_to_one_folds());
        assert!(!ctx_deu.lang_entry.requires_peek_ahead());

        // Dutch: has fold_map (Ĳ→ij), requires peek-ahead
        assert!(ctx_nld.lang_entry.has_fold_map());
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
