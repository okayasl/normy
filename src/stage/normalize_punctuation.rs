use crate::{
    all_langs,
    context::Context,
    lang::Lang,
    stage::{CharMapper, Stage, StageError},
    testing::stage_contract::StageTestConfig,
    unicode::normalize_punctuation_char,
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

/// Normalize Unicode punctuation to ASCII equivalents based on a fixed mapping.
///
/// This stage replaces various Unicode punctuation characters with standard
/// ASCII ones according to the internal `PUNCT_NORM` table, making text
/// easier to process in search, tokenization, or other NLP pipelines. It is
/// idempotent and zero-copy when the input requires no changes.
///
/// ## Normalizations performed
///
/// | Unicode | ASCII |
/// |---------|-------|
/// | `“`, `”`, `„`, `«`, `»` | `"` |
/// | `‘`, `’`, `‚`             | `'` |
/// | `–`, `—`, `─`, `―`       | `-` |
/// | `…`, `⋯`, `․`, `‧`       | `.` |
/// | `•`, `·`, `∙`             | `*` |
/// | `‹`                       | `<` |
/// | `›`                       | `>` |
/// | `′`, `″`                  | `"` |
///
/// All other characters, including ASCII, are left unchanged.
///
/// # Features
/// - Implements `Stage` and `CharMapper`, supporting full-text normalization
///   or character-wise mapping.
/// - Returns `Cow::Borrowed` if no changes are needed, avoiding unnecessary allocations.
/// - Suitable for pipelines that require consistent ASCII punctuation, e.g.,
///   search indexing, simplified display, or NLP preprocessing.
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

impl StageTestConfig for NormalizePunctuation {
    fn one_to_one_languages() -> &'static [Lang] {
        all_langs() // Language-independent
    }

    fn samples(_lang: Lang) -> &'static [&'static str] {
        &["Hello \"World\" 123", " déjà-vu… ", "TEST—", "", "\"'–…\""]
    }

    fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
        &[
            "hello world", // No fancy punctuation
            "test-123",    // ASCII hyphen
            "it's okay",   // ASCII apostrophe
            "",
        ]
    }

    fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
        &[
            ("\"Hello\"", "\"Hello\""), // Smart quotes → ASCII
            ("'world'", "'world'"),     // Smart single quotes
            ("—dash—", "-dash-"),       // Em dash → hyphen
            ("…", "."),                 // Ellipsis
        ]
    }

    fn skip_needs_apply_test() -> bool {
        true
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::{ENG, assert_stage_contract};
    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(NormalizePunctuation);
    }

    #[test]
    fn test_apply_mixed_punctuation() {
        let stage = NormalizePunctuation;
        let result = stage
            .apply(Cow::Borrowed("“Hello”—‘world’…•‹›"), &Context::new(ENG))
            .unwrap();
        assert_eq!(result, "\"Hello\"-'world'.*<>");
    }

    #[test]
    fn test_char_mapper_handles_unchanged() {
        let stage = NormalizePunctuation;
        let mapper: &dyn CharMapper = &stage;
        assert_eq!(mapper.map('A', &Context::new(ENG)), Some('A'));
        assert_eq!(mapper.map(' ', &Context::new(ENG)), Some(' '));
        assert_eq!(mapper.map('1', &Context::new(ENG)), Some('1'));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::data::ENG;

    #[test]
    fn test_needs_apply_detects_fancy_quotes() {
        let stage = NormalizePunctuation;
        assert!(
            stage
                .needs_apply("hello “world”", &Context::new(ENG))
                .unwrap()
        );
    }

    #[test]
    fn test_needs_apply_false_for_ascii() {
        let stage = NormalizePunctuation;
        assert!(
            !stage
                .needs_apply("hello world", &Context::new(ENG))
                .unwrap()
        );
    }

    #[test]
    fn test_apply_quotes() {
        let stage = NormalizePunctuation;
        let result = stage
            .apply(Cow::Borrowed("“Hello” ‘world’"), &Context::new(ENG))
            .unwrap();
        assert_eq!(result, "\"Hello\" 'world'");
    }

    #[test]
    fn test_apply_dashes() {
        let stage = NormalizePunctuation;
        let result = stage
            .apply(Cow::Borrowed("foo – bar — baz"), &Context::new(ENG))
            .unwrap();
        assert_eq!(result, "foo - bar - baz");
    }

    #[test]
    fn test_apply_ellipsis() {
        let stage = NormalizePunctuation;
        let result = stage
            .apply(Cow::Borrowed("Wait… really?"), &Context::new(ENG))
            .unwrap();
        assert_eq!(result, "Wait. really?");
    }

    #[test]
    fn test_char_mapper_map() {
        let stage = NormalizePunctuation;
        let mapper: &dyn CharMapper = &stage;

        assert_eq!(mapper.map('“', &Context::new(ENG)), Some('"'));
        assert_eq!(mapper.map('’', &Context::new(ENG)), Some('\''));
        assert_eq!(mapper.map('—', &Context::new(ENG)), Some('-'));
        assert_eq!(mapper.map('x', &Context::new(ENG)), Some('x')); // unchanged ASCII
    }

    #[test]
    fn test_char_mapper_bind_iterates_normalized() {
        let stage = NormalizePunctuation;
        let mapper: &dyn CharMapper = &stage;

        let iter = mapper.bind("A “quote” and… dash—", &Context::new(ENG));
        let collected: String = iter.collect();

        assert_eq!(collected, "A \"quote\" and. dash-");
    }

    #[test]
    fn test_apply_when_no_changes_returns_borrowed() {
        let stage = NormalizePunctuation;
        let text = Cow::Borrowed("all ascii here");
        let result = stage.apply(text.clone(), &Context::new(ENG)).unwrap();

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
        let result = stage.apply(text.clone(), &Context::new(ENG)).unwrap();
        assert_eq!(result, "\"Hello\" - said 'John'.");
    }
}
