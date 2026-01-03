use crate::{
    context::Context,
    lang::Lang,
    stage::{Stage, StageError, StaticFusableStage},
    testing::stage_contract::StageTestConfig,
    unicode::is_control,
};
use std::borrow::Cow;
use std::iter::FusedIterator;

/// Removes Unicode control characters (General Category Cc).
///
/// This stage strips C0 controls (U+0000–U+001F) and C1 controls (U+007F–U+009F),
/// which are invisible and often indicate data corruption or injection attacks.
///
/// - Format controls (Cf) such as ZWSP, ZWJ, and BOM are **preserved** — use
///   `StripFormatControls` for those.
/// - Zero-copy when no Cc characters are present.
/// - Fully fusable filter.
///
/// This stage is eligible for static fusion in all supported languages.
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
        &[]
    }

    fn samples(_lang: Lang) -> &'static [&'static str] {
        &[
            "hello\u{0001}world\u{007F}",
            "clean text only",
            "\u{001F}start\u{0085}middle\u{009F}end",
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
            ("a\u{007F}b\u{0085}c", "abc"),
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
    use crate::unicode::is_control;

    #[test]
    fn test_cc_vs_cf_boundary() {
        // Documents that StripControlChars (Cc) and StripFormatControls (Cf) don't overlap

        // Cc - Control characters (stripped by this stage)
        assert!(is_control('\u{0000}')); // NUL
        assert!(is_control('\u{001F}')); // C0 controls
        assert!(is_control('\u{007F}')); // DEL
        assert!(is_control('\u{009F}')); // C1 controls

        // Not Cc - Should NOT be stripped by this stage
        assert!(!is_control('A'));
        assert!(!is_control(' '));

        // CRITICAL: Format controls (Cf) are NOT control chars (Cc)
        assert!(!is_control('\u{200B}')); // ZWSP - handled by StripFormatControls
        assert!(!is_control('\u{200E}')); // LRM - handled by StripFormatControls
        assert!(!is_control('\u{FEFF}')); // BOM - handled by StripFormatControls
    }
}
