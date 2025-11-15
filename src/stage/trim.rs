use crate::{
    context::Context,
    stage::{CharMapper, Stage, StageError},
};
use std::{borrow::Cow, sync::Arc};

pub struct Trim;

impl Stage for Trim {
    fn name(&self) -> &'static str {
        "trim"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, _: &Context) -> Result<bool, StageError> {
        let bytes = text.as_bytes();
        // Fast ASCII path: check first/last byte
        if bytes.first().is_some_and(u8::is_ascii_whitespace)
            || bytes.last().is_some_and(u8::is_ascii_whitespace)
        {
            return Ok(true);
        }
        // Unicode fallback: only if needed
        Ok(text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let trimmed = text.trim();
        if trimmed.as_ptr() == text.as_ptr() && trimmed.len() == text.len() {
            return Ok(text);
        }
        Ok(Cow::Owned(trimmed.to_string()))
    }

    #[inline]
    fn as_char_mapper(&self, _: &Context) -> Option<&dyn CharMapper> {
        None
    }
    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, _: &Context) -> Option<Arc<dyn CharMapper>> {
        None
    }
}
