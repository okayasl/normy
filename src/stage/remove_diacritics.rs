//! src/stage/remove_diacritics.rs
//!
//! Removes language-specific diacritical marks using NFD (Canonical Decomposition).

use crate::{
    context::Context,
    lang::{Lang, LocaleBehavior},
    stage::{CharMapper, Stage, StageError},
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfd_quick};

/// Removes language-specific diacritical marks without expanding compatibility
/// characters (ligatures, fractions, superscripts, etc.).
///
/// # Normalization Form
///
/// Uses **NFD (Canonical Decomposition)** before filtering:
/// - Precomposed characters decomposed: `é` → `e` + combining acute
/// - **Ligatures preserved**: `ﬁ` remains `ﬁ` (not expanded to `fi`)
/// - **Fractions preserved**: `½` remains `½` (not decomposed to `1⁄2`)
/// - **Superscripts preserved**: `m²` remains `m²` (not normalized to `m2`)
///
/// # When to Use
///
/// - Preparing text for phonetic processing
/// - Removing accents while preserving typography
/// - Text-to-speech preprocessing
pub struct RemoveDiacritics;

impl Stage for RemoveDiacritics {
    fn name(&self) -> &'static str {
        "remove_diacritics"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        if !ctx.lang.has_diacritics() {
            return Ok(false);
        }
        if text.is_ascii() {
            return Ok(false);
        }
        // Quick NFD check
        if matches!(is_nfd_quick(text.chars()), IsNormalized::Yes)
            && !ctx.lang.contains_diacritics(text)
        {
            return Ok(false);
        }
        Ok(true)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !ctx.lang.has_diacritics() {
            return Ok(text);
        }

        let mut has_diacritic = false;
        let mut out = String::with_capacity(text.len());

        for c in text.nfd() {
            if ctx.lang.is_diacritic(c) {
                has_diacritic = true;
                continue;
            }
            out.push(c);
        }

        if !has_diacritic {
            Ok(text)
        } else {
            Ok(Cow::Owned(out))
        }
    }

    #[inline]
    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        ctx.lang.has_diacritics().then_some(self)
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        ctx.lang.has_diacritics().then_some(self)
    }
}

impl CharMapper for RemoveDiacritics {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> Option<char> {
        if ctx.lang.is_diacritic(c) {
            None
        } else {
            Some(c)
        }
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        if text.is_ascii() || !ctx.lang.has_diacritics() {
            return Box::new(text.chars());
        }
        Box::new(RemoveDiacriticsIter {
            chars: text.nfd(),
            lang: ctx.lang,
        })
    }
}

struct RemoveDiacriticsIter<I> {
    chars: I,
    lang: Lang,
}

impl<I: Iterator<Item = char>> Iterator for RemoveDiacriticsIter<I> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let c = self.chars.next()?;
            if !self.lang.is_diacritic(c) {
                return Some(c);
            }
        }
    }
}

impl<I: Iterator<Item = char>> FusedIterator for RemoveDiacriticsIter<I> {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{ARA, ENG, FRA, HEB, VIE};

    fn ctx(lang: crate::lang::Lang) -> Context {
        Context { lang }
    }

    // ------------------------------------------------------------------------
    // Basic Functionality
    // ------------------------------------------------------------------------

    #[test]
    fn test_ascii_no_op() {
        let stage = RemoveDiacritics;
        let c = ctx(ENG);

        assert!(!stage.needs_apply("hello world", &c).unwrap());
        assert_eq!(stage.apply(Cow::Borrowed("hello"), &c).unwrap(), "hello");
    }

    #[test]
    fn test_arabic_diacritics() {
        let stage = RemoveDiacritics;
        let c = ctx(ARA);
        let input = "مَرْحَبًا"; // "Hello" with diacritics

        assert!(stage.needs_apply(input, &c).unwrap());
        assert_eq!(stage.apply(Cow::Borrowed(input), &c).unwrap(), "مرحبا");
    }

    #[test]
    fn test_french_accents() {
        let stage = RemoveDiacritics;
        let c = ctx(FRA);

        assert_eq!(stage.apply(Cow::Borrowed("café"), &c).unwrap(), "cafe");
        assert_eq!(stage.apply(Cow::Borrowed("naïve"), &c).unwrap(), "naive");
        assert_eq!(stage.apply(Cow::Borrowed("résumé"), &c).unwrap(), "resume");
    }

    #[test]
    fn test_vietnamese() {
        let stage = RemoveDiacritics;
        let c = ctx(VIE);

        assert_eq!(stage.apply(Cow::Borrowed("Hà Nội"), &c).unwrap(), "Ha Noi");
    }

    #[test]
    fn test_hebrew() {
        let stage = RemoveDiacritics;
        let c = ctx(HEB);

        // Hebrew with nikud (vowel points)
        let input = "שָׁלוֹם"; // "Shalom" with diacritics
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();
        assert_eq!(result, "שלום");
    }

    // ------------------------------------------------------------------------
    // NFD vs NFKD: Verify No Side Effects
    // ------------------------------------------------------------------------

    #[test]
    fn test_preserves_ligatures() {
        let stage = RemoveDiacritics;
        let c = ctx(FRA);

        // Latin ligatures should be preserved
        assert_eq!(stage.apply(Cow::Borrowed("ﬁle"), &c).unwrap(), "ﬁle");
        assert_eq!(stage.apply(Cow::Borrowed("ﬂoor"), &c).unwrap(), "ﬂoor");
        assert_eq!(stage.apply(Cow::Borrowed("oﬀer"), &c).unwrap(), "oﬀer");
    }

    #[test]
    fn test_preserves_fractions() {
        let stage = RemoveDiacritics;
        let c = ctx(FRA);

        assert_eq!(
            stage.apply(Cow::Borrowed("½ tasse"), &c).unwrap(),
            "½ tasse"
        );
        assert_eq!(stage.apply(Cow::Borrowed("¾"), &c).unwrap(), "¾");
    }

    #[test]
    fn test_preserves_superscripts() {
        let stage = RemoveDiacritics;
        let c = ctx(FRA);

        assert_eq!(stage.apply(Cow::Borrowed("m²"), &c).unwrap(), "m²");
        assert_eq!(stage.apply(Cow::Borrowed("x³"), &c).unwrap(), "x³");
    }

    #[test]
    fn test_combined_diacritics_and_ligatures() {
        let stage = RemoveDiacritics;
        let c = ctx(FRA);

        // Should remove diacritics but preserve ligatures
        let input = "café ﬁle";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();

        assert_eq!(result, "cafe ﬁle");
        assert!(result.contains("ﬁ"), "Ligature should be preserved");
        assert!(!result.contains("é"), "Diacritic should be removed");
    }

    // ------------------------------------------------------------------------
    // Language-Specific Behavior
    // ------------------------------------------------------------------------

    #[test]
    fn test_english_no_diacritics_early_return() {
        let stage = RemoveDiacritics;
        let c = ctx(ENG);

        // English has no diacritics, should return early
        let input = "file ﬁle ½";
        assert!(!stage.needs_apply(input, &c).unwrap());

        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();
        assert!(matches!(result, Cow::Borrowed(_))); // Zero-copy
        assert_eq!(result, input);
    }

    #[test]
    fn test_language_without_diacritics_defined() {
        let stage = RemoveDiacritics;

        // Languages with empty diacritic lists
        for lang in [ENG, crate::lang::DEU, crate::lang::NLD] {
            let c = ctx(lang);
            assert!(!stage.needs_apply("test", &c).unwrap());
            assert!(stage.as_char_mapper(&c).is_none());
        }
    }

    // ------------------------------------------------------------------------
    // CharMapper Eligibility
    // ------------------------------------------------------------------------

    #[test]
    fn test_char_mapper_eligibility() {
        let stage = RemoveDiacritics;

        // Languages with diacritics: eligible
        assert!(stage.as_char_mapper(&ctx(ARA)).is_some());
        assert!(stage.as_char_mapper(&ctx(FRA)).is_some());
        assert!(stage.as_char_mapper(&ctx(HEB)).is_some());
        assert!(stage.as_char_mapper(&ctx(VIE)).is_some());

        // Languages without diacritics: not eligible
        assert!(stage.as_char_mapper(&ctx(ENG)).is_none());
    }

    // ------------------------------------------------------------------------
    // Edge Cases
    // ------------------------------------------------------------------------

    #[test]
    fn test_idempotency() {
        let stage = RemoveDiacritics;
        let c = ctx(ARA);

        let once = stage.apply(Cow::Borrowed("مَرْحَبًا"), &c).unwrap();
        let twice = stage.apply(Cow::Borrowed(&once), &c).unwrap();

        assert_eq!(once, "مرحبا");
        assert_eq!(once, twice);
    }

    #[test]
    fn test_empty_string() {
        let stage = RemoveDiacritics;
        let c = ctx(FRA);

        assert!(!stage.needs_apply("", &c).unwrap());
        assert_eq!(stage.apply(Cow::Borrowed(""), &c).unwrap(), "");
    }

    #[test]
    fn test_no_diacritics_zero_copy() {
        let stage = RemoveDiacritics;
        let c = ctx(FRA);

        let input = "hello world";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();

        assert!(matches!(result, Cow::Borrowed(_))); // Zero-copy
        assert_eq!(result, input);
    }

    #[test]
    fn test_all_diacritics_removed() {
        let stage = RemoveDiacritics;
        let c = ctx(FRA);

        // String of only diacritics (combining marks after NFD)
        let input = "e\u{0301}\u{0300}"; // e with acute and grave
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();

        assert_eq!(result, "e");
    }

    // ------------------------------------------------------------------------
    // Real-World Examples
    // ------------------------------------------------------------------------

    #[test]
    fn test_french_sentences() {
        let stage = RemoveDiacritics;
        let c = ctx(FRA);

        let examples = vec![
            ("Crème brûlée", "Creme brulee"),
            ("École française", "Ecole francaise"),
            ("Où est la bibliothèque?", "Ou est la bibliotheque?"),
        ];

        for (input, expected) in examples {
            assert_eq!(stage.apply(Cow::Borrowed(input), &c).unwrap(), expected);
        }
    }

    #[test]
    fn test_mixed_scripts() {
        let stage = RemoveDiacritics;
        let c = ctx(ARA);

        // Arabic with English
        let input = "Hello مَرْحَبًا World";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();

        assert_eq!(result, "Hello مرحبا World");
    }
}
