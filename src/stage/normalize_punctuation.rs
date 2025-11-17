//! stage/normalize_punctuation.rs
//! Normalize fancy punctuation to ASCII equivalents
//! “” → ", ‘’ → ', –—→ -, …→ ..., etc.
//! Critical for search relevance

use crate::{
    context::Context,
    stage::{CharMapper, Stage, StageError},
    unicode::normalize_punctuation_char,
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

pub struct NormalizePunctuation;

impl Stage for NormalizePunctuation {
    fn name(&self) -> &'static str {
        "normalize_punctuation"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(text.chars().any(|c| normalize_punctuation_char(c) != c))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, _ctx)? {
            return Ok(text);
        }
        Ok(Cow::Owned(
            text.chars().map(normalize_punctuation_char).collect(),
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

impl CharMapper for NormalizePunctuation {
    #[inline(always)]
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        let n = normalize_punctuation_char(c);
        if n == '\0' { None } else { Some(n) }
    }

    fn bind<'a>(&self, text: &'a str, _ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(text.chars().filter_map(|c| {
            let n = normalize_punctuation_char(c);
            if n == '\0' { None } else { Some(n) }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::ENG;

    fn make_context() -> Context {
        Context { lang: ENG }
    }

    #[test]
    fn test_needs_apply_detects_fancy_quotes() {
        let stage = NormalizePunctuation;
        assert!(stage.needs_apply("hello “world”", &make_context()).unwrap());
    }

    #[test]
    fn test_needs_apply_false_for_ascii() {
        let stage = NormalizePunctuation;
        assert!(!stage.needs_apply("hello world", &make_context()).unwrap());
    }

    #[test]
    fn test_apply_quotes() {
        let stage = NormalizePunctuation;
        let result = stage
            .apply(Cow::Borrowed("“Hello” ‘world’"), &make_context())
            .unwrap();
        assert_eq!(result, "\"Hello\" 'world'");
    }

    #[test]
    fn test_apply_dashes() {
        let stage = NormalizePunctuation;
        let result = stage
            .apply(Cow::Borrowed("foo – bar — baz"), &make_context())
            .unwrap();
        assert_eq!(result, "foo - bar - baz");
    }

    #[test]
    fn test_apply_ellipsis() {
        let stage = NormalizePunctuation;
        let result = stage
            .apply(Cow::Borrowed("Wait… really?"), &make_context())
            .unwrap();
        assert_eq!(result, "Wait. really?");
    }

    #[test]
    fn test_char_mapper_map() {
        let stage = NormalizePunctuation;
        let mapper: &dyn CharMapper = &stage;

        assert_eq!(mapper.map('“', &make_context()), Some('"'));
        assert_eq!(mapper.map('’', &make_context()), Some('\''));
        assert_eq!(mapper.map('—', &make_context()), Some('-'));
        assert_eq!(mapper.map('x', &make_context()), Some('x')); // unchanged ASCII
    }

    #[test]
    fn test_char_mapper_bind_iterates_normalized() {
        let stage = NormalizePunctuation;
        let mapper: &dyn CharMapper = &stage;

        let iter = mapper.bind("A “quote” and… dash—", &make_context());
        let collected: String = iter.collect();

        assert_eq!(collected, "A \"quote\" and. dash-");
    }

    #[test]
    fn test_apply_when_no_changes_returns_borrowed() {
        let stage = NormalizePunctuation;
        let text = Cow::Borrowed("all ascii here");
        let result = stage.apply(text.clone(), &make_context()).unwrap();

        // ensures zero-copy when no normalization is needed
        match result {
            Cow::Borrowed(_) => {} // OK
            _ => panic!("Expected Cow::Borrowed for unchanged text"),
        }
    }

    #[test]
    fn test_normalize_punctuation() {
        let stage = NormalizePunctuation;
        let text = Cow::Borrowed("“Hello” – said ‘John’…");
        let result = stage.apply(text.clone(), &make_context()).unwrap();
        assert_eq!(result, "\"Hello\" - said 'John'.");
    }
}
