use crate::{
    context::Context,
    lang::Lang,
    stage::{CharMapper, Stage, StageError, StageIter},
    testing::stage_contract::StageTestConfig,
    unicode::is_control,
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

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

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // We are here → control chars exist → allocate once, filter perfectly
        Ok(Cow::Owned(StripControlCharsIter::new(&text, ctx).collect()))
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

impl CharMapper for StripControlChars {
    #[inline(always)]
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        if is_control(c) { None } else { Some(c) }
    }

    #[inline(always)]
    fn bind<'a>(
        &self,
        text: &'a str,
        ctx: &'a Context,
    ) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(StripControlCharsIter::new(text, ctx))
    }
}

impl StageIter for StripControlChars {
    type Iter<'a> = StripControlCharsIter<'a>;

    #[inline(always)]
    fn try_iter<'a>(&self, text: &'a str, ctx: &'a Context) -> Option<Self::Iter<'a>> {
        Some(StripControlCharsIter::new(text, ctx))
    }
}

pub struct StripControlCharsIter<'a> {
    chars: std::str::Chars<'a>,
}

impl<'a> StripControlCharsIter<'a> {
    #[inline(always)]
    pub fn new(text: &'a str, _ctx: &'a Context) -> Self {
        Self {
            chars: text.chars(),
        }
    }
}

impl Iterator for StripControlCharsIter<'_> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let c = self.chars.next()?;
            if !is_control(c) {
                return Some(c);
            }
            // skip control char
        }
    }
}

impl FusedIterator for StripControlCharsIter<'_> {}

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
    fn test_char_mapper_map() {
        let stage = StripControlChars;
        let mapper: &dyn CharMapper = &stage;
        let ctx = Context::default();

        assert_eq!(mapper.map('A', &ctx), Some('A'));
        assert_eq!(mapper.map('\u{0001}', &ctx), None);
        assert_eq!(mapper.map('\u{007F}', &ctx), None);
    }

    #[test]
    fn test_char_mapper_bind_iterates_filtered() {
        let stage = StripControlChars;
        let mapper: &dyn CharMapper = &stage;
        let ctx = Context::default();

        let input = "A\u{0001}B\u{007F}C";
        let collected: String = mapper.bind(input, &ctx).collect();
        assert_eq!(collected, "ABC");
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
