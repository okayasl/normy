use crate::{
    CAT, DAN, DEU, ELL, ENG, FRA, ISL, ITA, LIT, NLD, NOR, POR, SPA, SWE, TUR,
    context::Context,
    lang::{Lang, LangEntry},
    stage::{CharMapper, FusedIterator, Stage, StageError},
    testing::stage_contract::StageTestConfig,
};
use std::borrow::Cow;
use std::sync::Arc;

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
        if text.chars().any(|c| ctx.lang_entry.needs_case_fold(c)) {
            return Ok(true);
        }
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
        // Handle peek-ahead languages
        if ctx.lang_entry.requires_peek_ahead() {
            return apply_with_peek_ahead(text, ctx);
        }

        // Pre-allocate capacity based on expected expansions
        let (_count, extra_bytes) = ctx.lang_entry.hint_capacity_fold(&text);
        let mut out = String::with_capacity(text.len() + extra_bytes);
        let mut changed = false;

        for c in text.chars() {
            if let Some(folded) = ctx.lang_entry.apply_case_fold(c) {
                out.push(folded);
                // Only mark as changed if the character actually changed
                if folded != c {
                    changed = true;
                }
            } else if let Some(m) = ctx.lang_entry.fold_map().iter().find(|m| m.from == c) {
                out.push_str(m.to);
                changed = true;
            } else if c.to_lowercase().next() != Some(c) {
                // Fallback: Unicode toLowercase() differs
                out.extend(c.to_lowercase());
                changed = true;
            } else {
                out.push(c);
            }
        }

        if changed {
            Ok(Cow::Owned(out))
        } else {
            Ok(text) // ← ZERO-COPY — even if called directly
        }
    }

    #[inline]
    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        // Use lang.rs helpers instead of manual checks
        if ctx.lang_entry.has_one_to_one_folds() && !ctx.lang_entry.requires_peek_ahead() {
            Some(self)
        } else {
            None
        }
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        if ctx.lang_entry.has_one_to_one_folds() && !ctx.lang_entry.requires_peek_ahead() {
            Some(self)
        } else {
            None
        }
    }
}

fn apply_with_peek_ahead<'a>(
    text: Cow<'a, str>,
    ctx: &Context,
) -> Result<Cow<'a, str>, StageError> {
    let fold_map = ctx.lang_entry.fold_map();
    let (foldable_count, extra_bytes) = ctx.lang_entry.hint_capacity_fold(&text);

    let mut out = String::with_capacity(
        text.len()
            + extra_bytes
            + if ctx.lang_entry.requires_peek_ahead() {
                foldable_count
            } else {
                0
            },
    );

    let mut changed = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        // Check for peek-ahead rules (e.g., Dutch IJ → ij)
        if let Some(target) = ctx.lang_entry.get_peek_fold(c, chars.peek().copied()) {
            let next_char = chars.next().unwrap(); // We know it exists from peek

            // Build what the original two-char sequence was
            let mut original = String::with_capacity(8);
            original.push(c);
            original.push(next_char);

            // Only mark as changed if the target differs from original
            if target != original.as_str() {
                changed = true;
            }

            out.push_str(target);
            continue;
        }

        // Check fold_map for multi-char expansions
        if let Some(m) = fold_map.iter().find(|m| m.from == c) {
            out.push_str(m.to);
            // Check if the mapping actually changes the character
            if m.to.len() != 1 || !m.to.starts_with(c) {
                changed = true;
            }
            continue;
        }

        // Fallback to Unicode lowercase
        let lowercase = c.to_lowercase();
        let first_lower = lowercase.clone().next();

        if first_lower != Some(c) {
            // Character changes when lowercased
            out.extend(lowercase);
            changed = true;
        } else {
            // Character unchanged
            out.push(c);
        }
    }

    if changed {
        Ok(Cow::Owned(out))
    } else {
        Ok(text) // ← ZERO-COPY for peek-ahead too!
    }
}

impl CharMapper for CaseFold {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> Option<char> {
        ctx.lang_entry.apply_case_fold(c)
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(CaseFoldIter {
            chars: text.chars(),
            lang: ctx.lang_entry,
        })
    }
}

struct CaseFoldIter<'a> {
    chars: std::str::Chars<'a>,
    lang: LangEntry,
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

impl<'a> FusedIterator for CaseFoldIter<'a> {}

impl StageTestConfig for CaseFold {
    fn one_to_one_languages() -> &'static [Lang] {
        &[ENG, FRA, SPA, ITA, POR, DAN, NOR, SWE, ISL, CAT]
    }

    fn samples(lang: Lang) -> &'static [&'static str] {
        match lang {
            TUR => &["İSTANBUL", "I", "İ", "ısı", "i"],
            DEU => &["Straße", "GROẞ"],
            NLD => &["IJssel", "Ĳssel", "ijssel"],
            ELL => &["ΣΟΦΟΣ", "ΟΔΟΣ"],
            LIT => &["JIS", "Jį"],
            _ => &["Hello WORLD", "Test 123", " café ", "NAÏVE"],
        }
    }

    fn should_pass_through(lang: Lang) -> &'static [&'static str] {
        match lang {
            TUR => &["ısı", "i", "istanbul", "hello"], // Already lowercase Turkish
            DEU => &["strasse", "gross", "hello"],     // Already folded German
            NLD => &["ijssel", "hello", "world"],      // Already lowercase Dutch
            ELL => &["σοφοσ", "οδοσ"],                 // Already lowercase Greek
            LIT => &["jis", "jį", "hello"],            // Already lowercase Lithuanian
            _ => &["hello", "world", "test123", ""],   // Simple lowercase
        }
    }

    fn should_transform(lang: Lang) -> &'static [(&'static str, &'static str)] {
        match lang {
            TUR => &[
                ("İ", "i"), // Turkish dotted I
                ("I", "ı"), // Turkish dotless I
                ("İSTANBUL", "istanbul"),
            ],
            DEU => &[
                ("ẞ", "ss"), // Capital Eszett
                ("Straße", "strasse"),
                ("GROẞ", "gross"),
            ],
            NLD => &[
                ("Ĳ", "ij"), // Dutch IJ ligature U+0132
                ("ĳ", "ij"), // Dutch ij ligature U+0133
                ("Ĳssel", "ijssel"),
            ],
            ELL => &[("Σ", "σ"), ("ΣΟΦΟΣ", "σοφοσ"), ("ΟΔΟΣ", "οδοσ")],
            LIT => &[("JIS", "jis"), ("JĮ", "jį")],
            _ => &[("HELLO", "hello"), ("World", "world"), ("NAÏVE", "naïve")],
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
mod language_specific_tests {
    use crate::{DEU, ELL, LIT, NLD, TUR};

    use super::*;

    #[test]
    fn case_fold_english_basic() {
        let ctx = Context::new(ENG);
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("HELLO"), &ctx).unwrap(),
            "hello"
        ); // ← fixed
    }

    #[test]
    fn case_fold_turkish_dotted_i_capital_i() {
        let ctx = Context::new(TUR);
        assert_eq!(CaseFold.apply(Cow::Borrowed("I"), &ctx).unwrap(), "ı");
    }

    #[test]
    fn case_fold_turkish_dotted_capital_i() {
        let ctx = Context::new(TUR);
        assert_eq!(CaseFold.apply(Cow::Borrowed("İ"), &ctx).unwrap(), "i");
    }

    #[test]
    fn case_fold_turkish_istanbul() {
        let ctx = Context::new(TUR);
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("İSTANBUL"), &ctx).unwrap(),
            "istanbul"
        );
    }

    #[test]
    fn case_fold_german_eszett_lowercase() {
        let ctx = Context::new(DEU);
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("straße"), &ctx).unwrap(),
            "strasse"
        );
    }

    #[test]
    fn case_fold_german_eszett_capital() {
        let ctx = Context::new(DEU);
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("GROẞ"), &ctx).unwrap(),
            "gross"
        );
    }

    #[test]
    fn case_fold_dutch_ij_sequence_uppercase() {
        let ctx = Context::new(NLD);
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("IJssel"), &ctx).unwrap(),
            "ijssel"
        );
    }

    #[test]
    fn case_fold_dutch_ij_ligature() {
        let ctx = Context::new(NLD);
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("Ĳssel"), &ctx).unwrap(),
            "ijssel"
        );
    }

    #[test]
    fn case_fold_dutch_ij_already_lowercase() {
        let ctx = Context::new(NLD);
        let result = CaseFold.apply(Cow::Borrowed("ijssel"), &ctx).unwrap();
        assert_eq!(result, "ijssel");
        assert!(!CaseFold.needs_apply("ijssel", &ctx).unwrap());
    }

    #[test]
    fn case_fold_greek_final_sigma() {
        let ctx = Context::new(ELL);
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("ΣΟΦΟΣ"), &ctx).unwrap(),
            "σοφοσ"
        ); // ← ς not σ
        assert_eq!(CaseFold.apply(Cow::Borrowed("ΟΔΟΣ"), &ctx).unwrap(), "οδοσ");
    }

    #[test]
    fn case_fold_lithuanian_i_without_dot() {
        let ctx = Context::new(LIT);
        assert_eq!(CaseFold.apply(Cow::Borrowed("JIS"), &ctx).unwrap(), "jis");
    }

    #[test]
    fn case_fold_char_mapper_eligibility() {
        // 1:1 languages → Some
        assert!(CaseFold.as_char_mapper(&Context::new(ENG)).is_some());
        assert!(CaseFold.as_char_mapper(&Context::new(FRA)).is_some());
        assert!(CaseFold.as_char_mapper(&Context::new(SPA)).is_some());

        // via lowercase → Some
        assert!(CaseFold.as_char_mapper(&Context::new(LIT)).is_some()); // contextual I
        assert!(CaseFold.as_char_mapper(&Context::new(TUR)).is_some()); // Turkish is 1:1!

        // Contextual or multi-char → None
        assert!(CaseFold.as_char_mapper(&Context::new(DEU)).is_none()); // ß → ss
        assert!(CaseFold.as_char_mapper(&Context::new(NLD)).is_none()); // IJ peek-ahead
        // assert!(CaseFold.as_char_mapper(&Context::new(ELL)).is_none()); // final sigma
    }

    #[test]
    fn test_needs_apply_for_french() {
        let ctx = Context::new(ENG);
        let needs_apply = CaseFold.needs_apply(" café ", &ctx).unwrap();
        assert!(!needs_apply);
    }

    #[test]
    fn test_needs_apply_for_turkish() {
        let ctx = Context::new(TUR);
        let needs_apply = CaseFold.needs_apply(" ısı ", &ctx).unwrap();
        assert!(!needs_apply);
    }

    #[test]
    fn test_needs_apply_for_dutch() {
        let ctx = Context::new(NLD);
        let needs_apply = CaseFold.needs_apply(" ijssel ", &ctx).unwrap();
        assert!(!needs_apply);
    }

    #[test]
    fn direct_apply_on_already_folded_text() {
        let ctx = Context::new(TUR);
        let text = "ısı"; // Already lowercase Turkish

        // Direct call (bypassing needs_apply check)
        let result = CaseFold.apply(Cow::Borrowed(text), &ctx).unwrap();

        // Should be zero-copy
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, text);
    }
}
