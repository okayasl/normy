use crate::{
    all_langs,
    context::Context,
    lang::Lang,
    stage::{Stage, StageError, StaticStageIter},
    testing::stage_contract::StageTestConfig,
    unicode::normalize_punctuation_char,
};
use std::iter::FusedIterator;
use std::{borrow::Cow, str::Chars};

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
        Ok(Cow::Owned(
            text.chars().map(normalize_punctuation_char).collect(),
        ))
    }

    fn try_dynamic_iter<'a>(
        &self,
        text: &'a str,
        _ctx: &'a Context,
    ) -> Option<Box<dyn FusedIterator<Item = char> + 'a>> {
        Some(Box::new(NormalizePunctuationIter::new(text)))
    }
}

impl StaticStageIter for NormalizePunctuation {
    type Iter<'a> = NormalizePunctuationIter<'a>;

    #[inline(always)]
    fn try_static_iter<'a>(&self, text: &'a str, _ctx: &'a Context) -> Option<Self::Iter<'a>> {
        Some(NormalizePunctuationIter::new(text))
    }
}

pub struct NormalizePunctuationIter<'a> {
    chars: Chars<'a>,
}

impl<'a> NormalizePunctuationIter<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            chars: text.chars(),
        }
    }
}

impl<'a> Iterator for NormalizePunctuationIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.chars.next().and_then(|c| {
            // The core logic is performed here.
            let n = normalize_punctuation_char(c);
            if n == '\0' { None } else { Some(n) }
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        // Since this is a filtering iterator (c may be replaced by '\0' or removed),
        // we can only provide the maximum size (upper bound).
        self.chars.size_hint()
    }
}

// 2. Implement FusedIterator for maximum compiler optimization.
impl<'a> FusedIterator for NormalizePunctuationIter<'a> {}

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
    fn test_normalize_punctuation() {
        let stage = NormalizePunctuation;
        let text = Cow::Borrowed("“Hello” – said ‘John’…");
        let result = stage.apply(text.clone(), &Context::new(ENG)).unwrap();
        assert_eq!(result, "\"Hello\" - said 'John'.");
    }
}
