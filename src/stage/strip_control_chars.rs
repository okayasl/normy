use crate::{
    context::Context,
    lang::Lang,
    stage::{Stage, StageError, StaticFusableStage},
    testing::stage_contract::StageTestConfig,
    unicode::is_control,
};
use std::borrow::Cow;
use std::iter::FusedIterator;

/// Remove all Unicode control characters (General Category `Cc`)
///
/// This stage strips **C0 and C1 control characters** (`U+0000`–`U+001F`, `U+007F`–`U+009F`)
/// which are never visible and often represent corruption, logging artifacts,
/// or malicious injection in text streams.
///
/// ### Use Cases
/// - Cleaning scraped web text
/// - Sanitizing user input from legacy systems
/// - Removing terminal control sequences (BEL, ESC, etc.)
/// - Preparing logs for indexing
///
/// ### Important Notes
/// - **Format controls (Cf)** like ZWSP, ZWJ, RLM are **not** removed → use `StripFormatControls`
/// - **Zero-copy** when no control characters present
/// - **CharMapper path** → fully fused, zero-allocation pipeline capable
///
/// This stage is **language-agnostic** and **idempotent**.
#[derive(Debug, Default, Clone, Copy)]
pub struct StripControlChars;

impl Stage for StripControlChars {
    fn name(&self) -> &'static str {
        "remove_control_chars"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        // Fast, branch-predictable scan — 90%+ of real text has no Cc
        Ok(text.chars().any(is_control))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // Direct allocation path for standalone execution
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            if !is_control(c) {
                out.push(c);
            }
        }
        Ok(Cow::Owned(out))
    }
}

impl StaticFusableStage for StripControlChars {
    type Adapter<'a, I>
        = StripControlCharsAdapter<I>
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
        StripControlCharsAdapter { input }
    }
}

pub struct StripControlCharsAdapter<I> {
    input: I,
}

impl<I: Iterator<Item = char>> Iterator for StripControlCharsAdapter<I> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        // Simple filter pattern: find the next non-control character
        self.input.find(|&c| !is_control(c))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.input.size_hint();
        // It can only shrink, so lower bound is 0
        (0, upper)
    }
}

impl<I: FusedIterator<Item = char>> FusedIterator for StripControlCharsAdapter<I> {}

impl StageTestConfig for StripControlChars {
    fn one_to_one_languages() -> &'static [Lang] {
        &[] // 1→0 mapping (filter)
    }

    fn samples(_lang: Lang) -> &'static [&'static str] {
        &[
            "hello\u{0001}world\u{007F}",
            "clean text only",
            "\u{001F}start",
            "end\u{009F}",
            "",
        ]
    }

    fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
        &["clean text only", "hello world", "test123", ""]
    }

    fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
        &[
            ("hello\u{0001}world", "helloworld"),
            ("\u{001F}start", "start"),
            ("end\u{009F}", "end"),
        ]
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;
    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(StripControlChars);
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
