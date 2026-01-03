use crate::{
    context::Context,
    lang::Lang,
    stage::{Stage, StageError, StaticFusableStage},
    testing::stage_contract::StageTestConfig,
    unicode::{contains_format_controls, is_format_control},
};
use std::borrow::Cow;
use std::iter::FusedIterator;

/// Removes Unicode format control characters (General Category Cf).
///
/// This stage strips invisible formatting controls such as:
///
/// - Zero-width spaces/joiners (ZWSP, ZWJ, ZWNJ)
/// - Bidirectional marks/overrides (LRM, RLM, LRE, etc.)
/// - Byte Order Mark (BOM, U+FEFF)
/// - Word joiner and invisible operators
///
/// These characters can disrupt tokenization, search matching, or cause security issues
/// in user-generated content. General control characters (Cc) are **preserved** — use
/// `StripControlChars` for those.
///
/// Zero-copy when no Cf characters are present. Fully fusable filter.
///
/// This stage is eligible for static fusion in all supported languages.
#[derive(Debug, Default, Clone, Copy)]
pub struct StripFormatControls;

impl Stage for StripFormatControls {
    fn name(&self) -> &'static str {
        "remove_format_controls"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        // Fast, predictable scan — 99%+ of real text has no Cf
        Ok(contains_format_controls(text))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // Direct allocation path: avoid iterator abstraction for single-stage execution
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            if !is_format_control(c) {
                out.push(c);
            }
        }
        Ok(Cow::Owned(out))
    }
}

impl StaticFusableStage for StripFormatControls {
    type Adapter<'a, I>
        = StripFormatControlsAdapter<I>
    where
        I: FusedIterator<Item = char> + 'a;

    #[inline(always)]
    fn supports_static_fusion(&self) -> bool {
        true
    }

    #[inline(always)]
    fn static_fused_adapter<'a, I>(&self, input: I, _ctx: &'a Context) -> Self::Adapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        StripFormatControlsAdapter { input }
    }
}

pub struct StripFormatControlsAdapter<I> {
    input: I,
}

impl<I: Iterator<Item = char>> Iterator for StripFormatControlsAdapter<I> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        // Skips characters until is_format_control(c) returns false
        self.input.find(|&c| !is_format_control(c))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.input.size_hint();
        (0, upper) // Can only shrink
    }
}

impl<I: FusedIterator<Item = char>> FusedIterator for StripFormatControlsAdapter<I> {}

impl StageTestConfig for StripFormatControls {
    fn one_to_one_languages() -> &'static [Lang] {
        &[]
    }

    fn samples(_lang: Lang) -> &'static [&'static str] {
        &[
            "hello\u{200B}world",       // ZWSP
            "\u{FEFF}bommed",           // BOM
            "Arabic\u{200F}text",       // RLM
            "a\u{2066}b\u{2069}c",      // LRI/PDI
            "text\u{200D}join\u{200C}", // ZWJ/ZWNJ
            "word\u{2060}joiner",       // Word joiner
            "clean text",
        ]
    }

    fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
        &["clean text", "hello world", "test123", ""]
    }

    fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
        &[
            ("hello\u{200B}world", "helloworld"),
            ("\u{FEFF}text", "text"),
            ("a\u{200E}b", "ab"),
            ("join\u{200D}me", "joinme"),
            ("no\u{2060}break", "nobreak"),
        ]
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;

    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(StripFormatControls);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::data::ENG;

    #[test]
    fn test_non_latin_scripts() {
        // Ensure format control stripping works with non-ASCII text
        let stage = StripFormatControls;
        let ctx = Context::new(ENG);

        // Arabic with RLM
        assert_eq!(
            stage
                .apply(Cow::Borrowed("مرحبا\u{200F} hello"), &ctx)
                .unwrap(),
            "مرحبا hello"
        );

        // Chinese with ZWSP
        assert_eq!(
            stage
                .apply(Cow::Borrowed("你好\u{200B}世界"), &ctx)
                .unwrap(),
            "你好世界"
        );

        // Mixed scripts with BOM
        assert_eq!(
            stage
                .apply(Cow::Borrowed("\u{FEFF}Привет مرحبا"), &ctx)
                .unwrap(),
            "Привет مرحبا"
        );
    }
}
