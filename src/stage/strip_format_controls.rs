use crate::{
    context::Context,
    lang::Lang,
    stage::{Stage, StageError, StaticFusableStage},
    testing::stage_contract::StageTestConfig,
    unicode::{contains_format_controls, is_format_control},
};
use std::borrow::Cow;
use std::iter::FusedIterator;

/// Remove all Unicode format control characters (General Category `Cf`)
///
/// Strips invisible presentation controls that affect rendering but not content:
/// - Zero-width spaces (ZWSP, ZWNJ, ZWJ)
/// - Bidirectional overrides (LRM, RLM, LRE, PDF, etc.)
/// - Byte Order Mark (BOM U+FEFF)
/// - Tag characters, interlinear annotation, etc.
///
/// ### Critical for:
/// - Search indexing (prevents hidden text attacks)
/// - ML training data cleaning
/// - API input normalization
/// - Tokenization consistency
///
/// Zero-copy when clean. Fully fused pipeline. Language-agnostic. Idempotent.
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
            "hello\u{200B}world",
            "\u{FEFF}bommed",
            "Arabic\u{200F}text",
            "a\u{2066}b\u{2069}c",
            "clean text",
        ]
    }

    fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
        &["clean text", "hello world", "test123", ""]
    }

    fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
        &[
            ("hello\u{200B}world", "helloworld"), // Remove ZWSP
            ("\u{FEFF}text", "text"),             // Remove BOM
            ("a\u{200E}b", "ab"),                 // Remove LRM
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
