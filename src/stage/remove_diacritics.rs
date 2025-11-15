// src/stage/remove_diacritics.rs
use crate::{
    context::Context,
    lang::LocaleBehavior,
    stage::{CharMapper, FusedIterator, Stage, StageError},
};
use std::borrow::Cow;
use std::sync::Arc;
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfkd_quick};

pub struct RemoveDiacritics;

impl Stage for RemoveDiacritics {
    fn name(&self) -> &'static str {
        "remove_diacritics"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        let set = match ctx.lang.diacritics() {
            Some(s) => s,
            None => return Ok(false),
        };
        // Zero-copy fast path: NFKD + no diacritics → skip
        let norm = is_nfkd_quick(text.chars());
        if matches!(norm, IsNormalized::Yes) && !text.chars().any(|c| set.contains(&c)) {
            return Ok(false);
        }
        Ok(true)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let set = match ctx.lang.diacritics() {
            Some(s) => s,
            None => return Ok(text),
        };
        let mut out = String::with_capacity(text.len());
        for c in text.nfkd() {
            if !set.contains(&c) {
                out.push(c);
            }
        }
        Ok(Cow::Owned(out))
    }

    #[inline]
    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        ctx.lang.diacritics().is_some().then_some(self as _)
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        ctx.lang.diacritics().is_some().then_some(self)
    }
}

impl CharMapper for RemoveDiacritics {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> Option<char> {
        let set = ctx.lang.diacritics()?;
        if set.contains(&c) { None } else { Some(c) }
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        if let Some(set) = ctx.lang.diacritics() {
            Box::new(text.nfkd().filter(move |c| !set.contains(c)))
        } else {
            Box::new(text.chars())
        }
    }
}
