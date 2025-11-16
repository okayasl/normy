//! src/stage/remove_diacritics.rs
//! Zero-copy, locale-aware diacritic removal.

use crate::{
    context::Context,
    lang::{Lang, LocaleBehavior},
    stage::{CharMapper, FusedIterator, Stage, StageError},
};
use std::borrow::Cow;
use std::sync::Arc;
use unicode_normalization::{UnicodeNormalization, is_nfkd_quick};

pub struct RemoveDiacritics;

impl Stage for RemoveDiacritics {
    fn name(&self) -> &'static str {
        "remove_diacritics"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        // Use helper method
        if !ctx.lang.has_diacritics() {
            return Ok(false);
        }

        // Pure ASCII → no diacritics possible
        if text.is_ascii() {
            return Ok(false);
        }

        // Quick NFKD check – if already NFKD and no diacritic chars, skip
        if matches!(
            is_nfkd_quick(text.chars()),
            unicode_normalization::IsNormalized::Yes
        ) && !ctx.lang.contains_diacritics(text)
        {
            return Ok(false);
        }

        Ok(true)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // Use helper method
        if !ctx.lang.has_diacritics() {
            return Ok(text);
        }

        // Decompose first – diacritics appear after NFKD
        let mut out = String::with_capacity(text.len());
        for c in text.nfkd() {
            if !ctx.lang.is_diacritic(c) {
                out.push(c);
            }
        }
        Ok(Cow::Owned(out))
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
        // Filter out diacritics, keep everything else
        if ctx.lang.is_diacritic(c) {
            None
        } else {
            Some(c)
        }
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        // ASCII fast-path
        if text.is_ascii() {
            return Box::new(text.chars());
        }

        // Check if language has diacritics
        if !ctx.lang.has_diacritics() {
            return Box::new(text.chars());
        }

        // Use custom iterator with lang helpers
        Box::new(RemoveDiacriticsIter {
            chars: text.nfkd(),
            lang: ctx.lang,
        })
    }
}

// Custom iterator using lang.rs helpers
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
            // Skip diacritics, continue loop
        }
    }
}

impl<I: Iterator<Item = char>> FusedIterator for RemoveDiacriticsIter<I> {}

// Tests remain the same...

// ---------------------------------------------------------------------
// Tests (add to the file)
// ---------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{ARA, ENG, FRA, VIE};

    fn ctx(lang: crate::lang::Lang) -> Context {
        Context { lang }
    }

    #[test]
    fn ascii_no_op() {
        let stage = RemoveDiacritics;
        let c = ctx(ENG);
        assert!(!stage.needs_apply("hello world", &c).unwrap());
        assert_eq!(stage.apply(Cow::Borrowed("hello"), &c).unwrap(), "hello");
    }

    #[test]
    fn arabic_diacritics() {
        let stage = RemoveDiacritics;
        let c = ctx(ARA);
        let input = "مَرْحَبًا";
        assert!(stage.needs_apply(input, &c).unwrap());
        assert_eq!(stage.apply(Cow::Borrowed(input), &c).unwrap(), "مرحبا");
    }

    #[test]
    fn french_accents() {
        let stage = RemoveDiacritics;
        let c = ctx(FRA);
        assert_eq!(
            stage.apply(Cow::Borrowed("café naïve"), &c).unwrap(),
            "cafe naive"
        );
    }

    #[test]
    fn vietnamese() {
        let stage = RemoveDiacritics;
        let c = ctx(VIE);
        assert_eq!(stage.apply(Cow::Borrowed("Hà Nội"), &c).unwrap(), "Ha Noi");
    }

    #[test]
    fn char_mapper_eligibility() {
        let stage = RemoveDiacritics;
        assert!(stage.as_char_mapper(&ctx(ARA)).is_some());
        assert!(stage.as_char_mapper(&ctx(ENG)).is_none());
    }

    #[test]
    fn idempotency() {
        let stage = RemoveDiacritics;
        let c = ctx(ARA);
        let once = stage.apply(Cow::Borrowed("مَرْحَبًا"), &c).unwrap();
        let twice = stage.apply(Cow::Borrowed(&once), &c).unwrap();
        assert_eq!(once, "مرحبا");
        assert_eq!(once, twice);
    }
}
