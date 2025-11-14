use crate::{
    context::Context,
    lang::LocaleBehavior,
    stage::{CharMapper, Stage, StageError},
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

pub struct RemoveDiacritics;

impl Stage for RemoveDiacritics {
    fn name(&self) -> &'static str {
        "remove_diacritics"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        let diacritics = ctx.lang.diacritics();

        if diacritics.is_none() {
            return Ok(false);
        }

        let diac_set = diacritics.unwrap();
        Ok(text.chars().any(|c| diac_set.contains(&c)))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let diacritics = ctx.lang.diacritics();

        if diacritics.is_none() {
            return Ok(text);
        }

        let diac_set = diacritics.unwrap();
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            if !diac_set.contains(&c) {
                out.push(c);
            }
        }
        Ok(Cow::Owned(out))
    }

    #[inline]
    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        if ctx.lang.diacritics().is_some() {
            Some(self)
        } else {
            None
        }
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        if ctx.lang.diacritics().is_some() {
            Some(self)
        } else {
            None
        }
    }
}

impl CharMapper for RemoveDiacritics {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> char {
        let diacritics = ctx.lang.diacritics();

        if diacritics.is_none() {
            return c;
        }

        let diac_set = diacritics.unwrap();
        if diac_set.contains(&c) {
            '\0' // Marker for "remove this character"
        } else {
            c
        }
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn Iterator<Item = char> + 'a> {
        let diacritics = ctx.lang.diacritics();

        if diacritics.is_none() {
            return Box::new(text.chars());
        }

        Box::new(RemoveDiacriticsIter {
            chars: text.chars(),
            diac_set: diacritics.unwrap(),
        })
    }
}

// ────── DIACRITIC REMOVAL ITERATOR ──────
struct RemoveDiacriticsIter<'a> {
    chars: std::str::Chars<'a>,
    diac_set: &'a [char],
}

impl<'a> Iterator for RemoveDiacriticsIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let c = self.chars.next()?;
            if !self.diac_set.contains(&c) {
                return Some(c);
            }
            // Skip diacritics and continue
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.chars.size_hint();
        (0, upper) // At least 0, at most the remaining chars
    }
}

impl<'a> FusedIterator for RemoveDiacriticsIter<'a> {}
