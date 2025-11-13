use crate::{
    context::Context,
    stage::{Stage, StageError},
};
use simdutf8::basic::from_utf8;
use std::borrow::Cow;

/// Fast UTF-8 validation — zero-copy, SIMD-accelerated
pub struct Utf8Validate;

impl Stage for Utf8Validate {
    fn name(&self) -> &'static str {
        "utf8_validate"
    }

    fn needs_apply(&self, _: &str, _: &Context) -> Result<bool, StageError> {
        Ok(true) // always run
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _: &Context) -> Result<Cow<'a, str>, StageError> {
        from_utf8(text.as_bytes())
            .map_err(|e| StageError::Validation(self.name(), e.to_string()))?;
        Ok(text)
    }

    fn char_mapper(&self) -> Option<Box<dyn crate::stage::CharMapper + 'static>> {
        None // validation is pre-iteration
    }
}
