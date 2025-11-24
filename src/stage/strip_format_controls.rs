use crate::{
    context::Context,
    stage::{CharMapper, Stage, StageError},
    unicode::{contains_format_controls, is_format_control},
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

/// Removes Unicode format control characters.
///
/// This stage removes invisible formatting characters that affect text
/// rendering but not content, including:
/// - Zero-width spaces (ZWSP, ZWNJ, ZWJ)
/// - Bidirectional formatting (LRM, RLM, LRE, RLE, PDF, etc.)
/// - Other format controls (word joiner, invisible operators, etc.)
///
/// # When to Use
/// - Preparing text for search indexing
/// - Cleaning text for machine learning
/// - Normalizing text for database storage
/// - API input sanitization
///
/// # Language Independence
/// This stage removes format controls **regardless of language**. Format
/// controls are presentation hints, not content, and are typically unwanted
/// in normalized text pipelines.
pub struct StripFormatControls;

impl Stage for StripFormatControls {
    fn name(&self) -> &'static str {
        "remove_format_controls"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(contains_format_controls(text))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !contains_format_controls(&text) {
            return Ok(text); // Zero-copy fast path
        }

        Ok(Cow::Owned(
            text.chars().filter(|&c| !is_format_control(c)).collect(),
        ))
    }

    #[inline]
    fn as_char_mapper(&self, _ctx: &Context) -> Option<&dyn CharMapper> {
        Some(self) // Always eligible (1→0 filter, no context needed)
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, _ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }
}

impl CharMapper for StripFormatControls {
    #[inline(always)]
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        if is_format_control(c) {
            None // Filter out
        } else {
            Some(c) // Keep
        }
    }

    fn bind<'a>(&self, text: &'a str, _ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(text.chars().filter(|&c| !is_format_control(c)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::data::ENG;

    #[test]
    fn test_zero_width_space() {
        let stage = StripFormatControls;
        let ctx = Context::new(ENG);

        let text = "hello\u{200B}world";
        assert!(stage.needs_apply(text, &ctx).unwrap());

        let result = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn test_bidi_marks() {
        let stage = StripFormatControls;
        let ctx = Context::new(ENG);

        // LRM + RLM
        let text = "hello\u{200E}world\u{200F}";
        let result = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn test_bom() {
        let stage = StripFormatControls;
        let ctx = Context::new(ENG);

        let text = "\u{FEFF}hello"; // BOM at start
        let result = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_multiple_controls() {
        let stage = StripFormatControls;
        let ctx = Context::new(ENG);

        let text = "\u{200B}\u{200C}\u{200D}text\u{202A}\u{202C}";
        let result = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        assert_eq!(result, "text");
    }

    #[test]
    fn test_no_controls_zero_copy() {
        let stage = StripFormatControls;
        let ctx = Context::new(ENG);

        let text = "hello world";
        assert!(!stage.needs_apply(text, &ctx).unwrap());

        let result = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        assert!(matches!(result, Cow::Borrowed(_))); // Zero-copy
    }

    #[test]
    fn test_char_mapper_eligible() {
        let stage = StripFormatControls;
        let ctx = Context::new(ENG);

        assert!(stage.as_char_mapper(&ctx).is_some());
    }

    #[test]
    fn test_real_world_arabic() {
        let stage = StripFormatControls;
        let ctx = Context::new(ENG);

        // Arabic text with RLM
        let text = "مرحبا\u{200F} hello";
        let result = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        assert_eq!(result, "مرحبا hello");
    }

    #[test]
    fn test_idempotency() {
        let stage = StripFormatControls;
        let ctx = Context::new(ENG);

        let text = "hello\u{200B}world";
        let first = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        let second = stage.apply(Cow::Borrowed(&first), &ctx).unwrap();

        assert_eq!(first, "helloworld");
        assert_eq!(first, second);
    }
}
