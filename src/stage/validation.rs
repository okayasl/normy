//! SIMD-accelerated UTF-8 validation – **cannot** be expressed as a char iterator.
//! Both mapper methods return `None`; the stage always falls back to the
//! `Cow<str>` path (which is still zero-copy on success).

use crate::{
    context::Context,
    stage::{Stage, StageError},
};
use simdutf8::basic::from_utf8;
use std::borrow::Cow;
use std::sync::Arc;

/// Public stage – zero-sized.
pub struct Utf8Validate;

impl Stage for Utf8Validate {
    fn name(&self) -> &'static str {
        "utf8_validate"
    }

    fn needs_apply(&self, _: &str, _: &Context) -> Result<bool, StageError> {
        Ok(true) // always run in production
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _: &Context) -> Result<Cow<'a, str>, StageError> {
        from_utf8(text.as_bytes())
            .map_err(|e| StageError::Validation(self.name(), e.to_string()))?;
        Ok(text)
    }

    // No iterator representation – validation is a bulk byte check.
    #[inline]
    fn as_char_mapper(&self) -> Option<&dyn crate::stage::CharMapper> {
        None
    }
    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>) -> Option<Arc<dyn crate::stage::CharMapper>> {
        None
    }
}
