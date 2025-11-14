use crate::{
    context::Context,
    stage::{Stage, StageError},
};
#[cfg(feature = "simd")]
use simdutf8::basic::from_utf8;
use std::borrow::Cow;
use std::sync::Arc;

pub struct Utf8Validate;

impl Stage for Utf8Validate {
    fn name(&self) -> &'static str {
        "simd_utf8_validate"
    }

    fn needs_apply(&self, _: &str, _: &Context) -> Result<bool, StageError> {
        Ok(true)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _: &Context) -> Result<Cow<'a, str>, StageError> {
        #[cfg(feature = "simd")]
        {
            from_utf8(text.as_bytes())
                .map_err(|e| StageError::Failed("invalid UTF-8", e.to_string()))?;
            Ok(text)
        }
        #[cfg(not(feature = "simd"))]
        {
            // Fallback to scalar if compiled without SIMD
            std::str::from_utf8(text.as_bytes())
                .map_err(|e| StageError::Failed("invalid UTF-8", e.to_string()))?;
            Ok(text)
        }
    }

    #[inline]
    fn as_char_mapper(&self, _: &Context) -> Option<&dyn crate::stage::CharMapper> {
        None
    }
    #[inline]
    fn into_dyn_char_mapper(
        self: Arc<Self>,
        _: &Context,
    ) -> Option<Arc<dyn crate::stage::CharMapper>> {
        None
    }
}
