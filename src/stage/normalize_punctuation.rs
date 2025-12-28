use crate::{
    all_langs,
    context::Context,
    lang::Lang,
    stage::{Stage, StageError, StaticFusableStage},
    testing::stage_contract::StageTestConfig,
    unicode::normalize_punctuation_char,
};
use std::borrow::Cow;
use std::iter::FusedIterator;

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
        // Punctuation normalization usually results in the same or smaller string.
        let mut out = String::with_capacity(text.len());

        for c in text.chars() {
            let n = normalize_punctuation_char(c);
            if n != '\0' {
                out.push(n);
            }
        }

        Ok(Cow::Owned(out))
    }
}

impl StaticFusableStage for NormalizePunctuation {
    type Adapter<'a, I>
        = NormalizePunctuationAdapter<I>
    where
        I: FusedIterator<Item = char> + 'a;
    fn supports_static_fusion(&self) -> bool {
        true
    }

    #[inline(always)]
    fn static_fused_adapter<'a, I>(&self, input: I, _ctx: &'a Context) -> Self::Adapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        NormalizePunctuationAdapter { input }
    }
}

pub struct NormalizePunctuationAdapter<I> {
    input: I,
}

impl<I: Iterator<Item = char>> Iterator for NormalizePunctuationAdapter<I> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        // Use a loop to skip characters that normalize to '\0'
        loop {
            let c = self.input.next()?;
            let n = normalize_punctuation_char(c);
            if n != '\0' {
                return Some(n);
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.input.size_hint();
        (0, upper) // Lower bound is 0 because we might filter everything
    }
}

impl<I: FusedIterator<Item = char>> FusedIterator for NormalizePunctuationAdapter<I> {}

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
