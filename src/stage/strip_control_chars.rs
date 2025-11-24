use crate::{
    context::Context,
    stage::{CharMapper, Stage, StageError},
    unicode::is_control,
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

/// This stage filters out all Unicode control characters (category Cc) from text.
/// Format controls (Cf) are **not** removed. Useful for cleaning input, logs,
/// or text streams that may contain non-printable characters.
pub struct StripControlChars;

impl Stage for StripControlChars {
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

impl CharMapper for StripControlChars {
    #[inline(always)]
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        if is_control(c) { None } else { Some(c) }
    }

    fn bind<'a>(&self, text: &'a str, _ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(text.chars().filter(|&c| !is_control(c)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;

    #[test]
    fn test_is_control() {
        assert!(is_control('\u{0000}'));
        assert!(is_control('\u{001F}'));
        assert!(is_control('\u{007F}'));
        assert!(is_control('\u{009F}'));
        assert!(!is_control('A'));
        assert!(!is_control(' '));
        assert!(!is_control('\u{200B}')); // zero-width space is not Cc
    }

    #[test]
    fn test_needs_apply_detects_control_chars() {
        let stage = StripControlChars;
        let ctx = Context::default();

        assert!(stage.needs_apply("hello\u{0001}world", &ctx).unwrap());
        assert!(!stage.needs_apply("hello world", &ctx).unwrap());
    }

    #[test]
    fn test_apply_removes_control_chars() {
        let stage = StripControlChars;
        let ctx = Context::default();

        let input = "hello\u{0001}\u{007F}world";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert_eq!(result, "helloworld");
    }

    #[test]
    fn test_apply_returns_borrowed_when_no_changes() {
        let stage = StripControlChars;
        let ctx = Context::default();

        let text = "plain ascii";
        let result = stage.apply(Cow::Borrowed(text), &ctx).unwrap();

        match result {
            Cow::Borrowed(_) => {} // OK
            _ => panic!("Expected Cow::Borrowed for unchanged text"),
        }
    }

    #[test]
    fn test_char_mapper_map() {
        let stage = StripControlChars;
        let mapper: &dyn CharMapper = &stage;
        let ctx = Context::default();

        assert_eq!(mapper.map('A', &ctx), Some('A'));
        assert_eq!(mapper.map('\u{0001}', &ctx), None);
        assert_eq!(mapper.map('\u{007F}', &ctx), None);
    }

    #[test]
    fn test_char_mapper_bind_iterates_filtered() {
        let stage = StripControlChars;
        let mapper: &dyn CharMapper = &stage;
        let ctx = Context::default();

        let input = "A\u{0001}B\u{007F}C";
        let collected: String = mapper.bind(input, &ctx).collect();
        assert_eq!(collected, "ABC");
    }

    #[test]
    fn test_idempotency() {
        let stage = StripControlChars;
        let ctx = Context::default();

        let input = "hello\u{0001}\u{007F}world";
        let first = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        let second = stage.apply(first.clone(), &ctx).unwrap();

        assert_eq!(first, "helloworld");
        assert_eq!(first, second);
    }
}
