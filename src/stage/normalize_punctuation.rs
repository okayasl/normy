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

/// Replaces typographic Unicode punctuation with ASCII equivalents.
///
/// This stage maps various Unicode punctuation characters to their basic ASCII
/// counterparts using a fixed table. It simplifies text for search indexing,
/// tokenization, or downstream systems that expect plain ASCII punctuation.
///
/// | Unicode                  | ASCII |
/// |--------------------------|-------|
/// | `“` `”` `„` `«` `»` `′` `″` | `"`   |
/// | `‘` `’` `‚`               | `'`   |
/// | `–` `—` `─` `―`           | `-`   |
/// | `…` `⋯` `․` `‧`           | `.`   |
/// | `•` `·` `∙`               | `*`   |
/// | `‹`                      | `<`   |
/// | `›`                      | `>`   |
///
/// All other characters pass through unchanged. The stage is fully fusable.
///
/// This stage is eligible for static fusion in all supported languages.
#[derive(Debug, Default, Clone, Copy)]
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
        all_langs()
    }

    fn samples(_lang: Lang) -> &'static [&'static str] {
        &["Hello \"World\" 123", " déjà-vu… ", "TEST—", "", "\"'–…•‹›"]
    }

    fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
        &["hello world", "test-123", "it's okay", ""]
    }

    fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
        &[
            ("“Hello”", "\"Hello\""),
            ("‘world’", "'world'"),
            ("—dash—", "-dash-"),
            ("…", "."),
            ("• bullet", "* bullet"),
            ("‹angle›", "<angle>"),
        ]
    }
}

// Single source of truth
#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;

    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(NormalizePunctuation);
    }
}
