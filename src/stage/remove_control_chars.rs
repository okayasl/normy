//! stage/remove_control_chars.rs
//! Remove C0 and C1 control characters (not format controls — those are Cf)
//! e.g. BEL, DEL, etc. — very common in crawled data

use crate::{
    context::Context,
    stage::{CharMapper, Stage, StageError},
    unicode::is_control,
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

pub struct RemoveControlChars;

impl Stage for RemoveControlChars {
    fn name(&self) -> &'static str {
        "remove_control_chars"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(text.chars().any(is_control))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, _ctx)? {
            return Ok(text);
        }
        Ok(Cow::Owned(
            text.chars().filter(|&c| !is_control(c)).collect(),
        ))
    }

    #[inline]
    fn as_char_mapper(&self, _ctx: &Context) -> Option<&dyn CharMapper> {
        Some(self)
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, _ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }
}

impl CharMapper for RemoveControlChars {
    #[inline(always)]
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        if is_control(c) { None } else { Some(c) }
    }

    fn bind<'a>(&self, text: &'a str, _ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(text.chars().filter(|&c| !is_control(c)))
    }
}
