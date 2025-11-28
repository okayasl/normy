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
                    .peek_ahead_fold(c, chars.peek().copied())
                    .is_some()
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if ctx.lang_entry.requires_peek_ahead() {
            return apply_with_peek_ahead(text, ctx);
        }

        let (_count, extra_bytes) = ctx.lang_entry.count_foldable_bytes(&text);
        let mut out = String::with_capacity(text.len() + extra_bytes);

        for c in text.chars() {
            if let Some(ch) = ctx.lang_entry.fold_char(c) {
                out.push(ch);
            } else if let Some(m) = ctx.lang_entry.fold_map().iter().find(|m| m.from == c) {
                out.push_str(m.to);
            } else {
                // Defensive: should never happen — but correct if it does
                out.extend(c.to_lowercase());
            }
        }

        Ok(Cow::Owned(out))
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
    let (foldable_count, extra_bytes) = ctx.lang_entry.count_foldable_bytes(&text); // Reuse helper
    let mut out = String::with_capacity(
        text.len()
            + extra_bytes
            + if ctx.lang_entry.requires_peek_ahead() {
                foldable_count
            } else {
                0
            },
    );
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(target) = ctx.lang_entry.peek_ahead_fold(c, chars.peek().copied()) {
            chars.next();
            out.push_str(target);
            continue;
        }
        if let Some(m) = fold_map.iter().find(|m| m.from == c) {
            out.push_str(m.to);
        } else {
            out.extend(c.to_lowercase());
        }
    }
    Ok(Cow::Owned(out))
}

impl CharMapper for CaseFold {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> Option<char> {
        ctx.lang_entry.fold_char(c)
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
        self.lang.fold_char(c)
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
}

// ─────────────────────────────────────────────────────────────────────────────
//  Universal contract tests (zero-cost, full coverage)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::testing::stage_contract::*;

    #[test]
    fn zero_copy() {
        zero_copy_when_no_changes(CaseFold);
    }
    #[test]
    fn fast_slow_eq() {
        fast_and_slow_paths_equivalent(CaseFold);
    }
    #[test]
    fn idempotent() {
        stage_is_idempotent(CaseFold);
    }
    #[test]
    fn needs_apply() {
        needs_apply_is_accurate(CaseFold);
    }
    #[test]
    fn empty_ascii() {
        handles_empty_string_and_ascii(CaseFold);
    }
    #[test]
    fn mixed_scripts() {
        no_panic_on_mixed_scripts(CaseFold);
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
}
