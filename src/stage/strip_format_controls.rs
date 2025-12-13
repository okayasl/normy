use crate::{
    context::Context,
    lang::Lang,
    stage::{CharMapper, Stage, StageError, StageIter},
    testing::stage_contract::StageTestConfig,
    unicode::{contains_format_controls, is_format_control},
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

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
        // We are here → format controls exist → allocate once, filter perfectly
        Ok(Cow::Owned(StripFormatControlsIter::new(&text).collect()))
    }

    #[inline]
    fn as_char_mapper(&self, _ctx: &Context) -> Option<&dyn CharMapper> {
        Some(self)
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, _ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }

    fn try_dynamic_iter<'a>(
        &self,
        text: &'a str,
        _ctx: &'a Context,
    ) -> Option<Box<dyn FusedIterator<Item = char> + 'a>> {
        Some(Box::new(StripFormatControlsIter::new(text)))
    }
}

impl CharMapper for StripFormatControls {
    #[inline(always)]
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        if is_format_control(c) { None } else { Some(c) }
    }

    #[inline(always)]
    fn bind<'a>(
        &self,
        text: &'a str,
        _ctx: &'a Context,
    ) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(StripFormatControlsIter::new(text))
    }
}

impl StageIter for StripFormatControls {
    type Iter<'a> = StripFormatControlsIter<'a>;

    #[inline(always)]
    fn try_iter<'a>(&self, text: &'a str, _ctx: &'a Context) -> Option<Self::Iter<'a>> {
        Some(StripFormatControlsIter::new(text))
    }
}

pub struct StripFormatControlsIter<'a> {
    chars: std::str::Chars<'a>,
}

impl<'a> StripFormatControlsIter<'a> {
    #[inline(always)]
    pub fn new(text: &'a str) -> Self {
        Self {
            chars: text.chars(),
        }
    }
}

impl Iterator for StripFormatControlsIter<'_> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let c = self.chars.next()?;
            if !is_format_control(c) {
                return Some(c);
            }
            // skip Cf
        }
    }
}

impl FusedIterator for StripFormatControlsIter<'_> {}

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

    fn skip_zero_copy_apply_test() -> bool {
        true
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
