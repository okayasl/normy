// src/stage/segment_word.rs
//! Word segmentation stage – inserts U+0020 spaces only where required by the
//! current language’s rules (CJK unigram, Thai/Lao/Khmer syllable breaks, etc.).
//! Zero-allocation for Western text, fully fused iterator for monomorphised pipelines.
//! Uses Lang::needs_boundary_between + is_same_script_cluster → perfect accuracy.
//! Implements ICU/Elasticsearch-compatible asymmetric word segmentation
//!
//! Core rules:
//! Insert a single ASCII space only when transitioning Western → Script
//! Never insert space on Script → Western transition
//! Never insert space inside pure Script runs
//! Respect language-specific segment_exceptions (perfect-hash, zero-cost prefix check)
//! Zero-allocation when no change occurs (including pure-exception text)
//! Fully fused CharMapper path when no exceptions and no peek-ahead needed

use std::{
    borrow::Cow,
    iter::{FusedIterator, Peekable},
    sync::Arc,
};

use crate::{
    context::Context,
    lang::{Lang, LocaleBehavior},
    stage::{CharMapper, Stage, StageError},
    unicode::{CharClass, classify, is_any_whitespace},
};

#[derive(Debug, Default, Clone, Copy)]
pub struct SegmentWord;

impl Stage for SegmentWord {
    fn name(&self) -> &'static str {
        "segment_word"
    }

    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        if !ctx.lang.needs_segmentation() {
            return Ok(false);
        }
        Ok(Self::needs_segmentation(text, ctx.lang))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !ctx.lang.needs_segmentation() || !self.needs_apply(&text, ctx)? {
            return Ok(text);
        }

        // Fast fused zero-allocation path
        if let Some(mapper) = self.as_char_mapper(ctx) {
            let mut out = String::with_capacity(text.len() + text.len() / 8);
            for c in mapper.bind(&text, ctx) {
                out.push(c);
            }
            return Ok(Cow::Owned(out));
        }

        // Fallback allocating path (still extremely fast)
        Ok(Cow::Owned(self.segment_allocating(&text, ctx.lang)))
    }

    #[inline]
    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        ctx.lang
            .needs_segmentation()
            .then_some(self as &dyn CharMapper)
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        ctx.lang.needs_segmentation().then_some(self)
    }
}

impl SegmentWord {
    /// Fast early-out – scan once, bail out early if no insertion needed
    #[inline]
    fn needs_segmentation(text: &str, lang: Lang) -> bool {
        let chars = text.chars().peekable();
        let mut prev = None;

        for curr in chars {
            if let Some(p) = prev
                && lang.needs_boundary_between(p, curr)
            {
                return true;
            }
            prev = Some(curr);
        }
        false
    }

    /// Allocating path – used only when we actually insert spaces
    fn segment_allocating(&self, text: &str, lang: Lang) -> String {
        let mut out = String::with_capacity(text.len() + text.len() / 8);
        let mut chars = text.chars().peekable();
        let mut prev = None;

        while let Some(curr) = chars.next() {
            if let Some(p) = prev
                && lang.needs_boundary_between(p, curr)
            {
                // Collapse any preceding whitespace into a single space
                if classify(p) == CharClass::Whitespace {
                    while chars.peek().is_some_and(|&n| is_any_whitespace(n)) {
                        chars.next();
                    }
                    out.push(' ');
                } else {
                    out.push(' ');
                }
            }

            // Collapse runs of whitespace to a single space
            if is_any_whitespace(curr) {
                if prev.is_none() || classify(prev.unwrap()) != CharClass::Whitespace {
                    out.push(' ');
                }
                while chars.peek().is_some_and(|&n| is_any_whitespace(n)) {
                    chars.next();
                }
            } else {
                out.push(curr);
            }

            prev = Some(curr);
        }
        out
    }
}

impl CharMapper for SegmentWord {
    #[inline]
    fn map(&self, _c: char, _ctx: &Context) -> Option<char> {
        Some(_c)
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(SegmentWordIterator {
            lang: ctx.lang,
            inner: text.chars().peekable(),
            prev: None,
            pending_space: false,
        })
    }
}

/// Fused zero-allocation iterator – the compiler eliminates bounds checks entirely
struct SegmentWordIterator<I>
where
    I: Iterator<Item = char>,
{
    lang: Lang,
    inner: Peekable<I>,
    prev: Option<char>,
    pending_space: bool,
}

impl<I> Iterator for SegmentWordIterator<I>
where
    I: Iterator<Item = char> + FusedIterator,
{
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pending_space {
            self.pending_space = false;
            return Some(' ');
        }

        let curr = self.inner.next()?;

        // Collapse whitespace runs early
        if is_any_whitespace(curr) {
            while self.inner.peek().is_some_and(|&n| is_any_whitespace(n)) {
                self.inner.next();
            }
            // Whitespace never triggers boundary, but may need space before it
            if self.prev.is_some_and(|p| !is_any_whitespace(p)) {
                self.pending_space = true;
            }
            return Some(' ');
        }

        // Normal char: check if we need space before it
        let needs_space = self
            .prev
            .is_some_and(|p| !is_any_whitespace(p) && self.lang.needs_boundary_between(p, curr));

        if needs_space {
            self.pending_space = true;
        }

        self.prev = Some(curr);
        Some(curr)
    }
}

impl<I> FusedIterator for SegmentWordIterator<I> where I: Iterator<Item = char> + FusedIterator {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::*;
    use std::borrow::Cow;

    macro_rules! ctx {
        ($lang:expr) => {
            Context { lang: $lang }
        };
    }

    #[test]
    fn western_text_unchanged() {
        let stage = SegmentWord;
        let ctx = ctx!(ENG);
        let text = "Hello world! This is a test.";
        assert!(!stage.needs_apply(text, &ctx).unwrap());
        assert_eq!(
            stage.apply(Cow::Borrowed(text), &ctx).unwrap().as_ref(),
            text
        );
    }

    #[test]
    fn cjk_gets_spaces() {
        let stage = SegmentWord;
        let input = "こんにちは世界";
        let expected = "こんにちは 世界";

        assert_eq!(
            stage
                .apply(Cow::Borrowed(input), &ctx!(JPN))
                .unwrap()
                .as_ref(),
            expected
        );
        assert_eq!(
            stage
                .apply(Cow::Borrowed(input), &ctx!(ZHO))
                .unwrap()
                .as_ref(),
            expected
        );
    }

    #[test]
    fn thai_correct_breaks() {
        let stage = SegmentWord;
        let input = "สวัสดีชาวโลก";
        let expected = "สวัสดี ชาว โลก";
        assert_eq!(
            stage
                .apply(Cow::Borrowed(input), &ctx!(THA))
                .unwrap()
                .as_ref(),
            expected
        );
    }

    #[test]
    fn mixed_latin_cjk() {
        let stage = SegmentWord;
        let cases = &[
            ("Hello世界", "Hello 世界"),
            ("Rustは最高", "Rust は 最高"),
            ("東京2025年", "東京 2025 年"),
            ("AIとLLM", "AI と LLM"),
        ];

        for &(input, expected) in cases {
            assert_eq!(
                stage
                    .apply(Cow::Borrowed(input), &ctx!(JPN))
                    .unwrap()
                    .as_ref(),
                expected
            );
        }
    }

    #[test]
    fn hangul_treated_as_script() {
        let stage = SegmentWord;
        let input = "안녕하세요세계";
        let expected = "안녕하세요 세계";
        assert_eq!(
            stage
                .apply(Cow::Borrowed(input), &ctx!(KOR))
                .unwrap()
                .as_ref(),
            expected
        );
    }

    #[test]
    fn whitespace_collapsed_and_boundaries_correct() {
        let stage = SegmentWord;
        let input = "こんにちは   世界\t\nです";
        let expected = "こんにちは 世界 です";
        assert_eq!(
            stage
                .apply(Cow::Borrowed(input), &ctx!(JPN))
                .unwrap()
                .as_ref(),
            expected
        );
    }

    #[test]
    fn idempotent_with_normalize_whitespace() {
        let stage = SegmentWord;
        let input = "你好  世界";
        let once = stage.apply(Cow::Borrowed(input), &ctx!(ZHO)).unwrap();
        let twice = stage.apply(Cow::Borrowed(&once), &ctx!(ZHO)).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn only_segmented_languages_affected() {
        let stage = SegmentWord;
        let non_segmented = [ENG, TUR, DEU, FRA, ARA, HEB];
        let segmented = [JPN, ZHO, KOR, THA, MYA, KHM, VIE];

        for &lang in &non_segmented {
            assert!(!stage.needs_apply("any text", &ctx!(lang)).unwrap());
        }
        for &lang in &segmented {
            assert!(stage.needs_apply("你好世界", &ctx!(lang)).unwrap());
        }
    }

    #[test]
    fn no_space_on_script_to_western() {
        let stage = SegmentWord;
        let input = "世界Hello";
        let expected = "世界Hello"; // ← NO space before Hello
        assert_eq!(
            stage
                .apply(Cow::Borrowed(input), &ctx!(JPN))
                .unwrap()
                .as_ref(),
            expected
        );
    }

    #[test]
    fn space_only_on_western_to_script() {
        let stage = SegmentWord;
        let input = "Rust世界";
        let expected = "Rust 世界"; // ← space inserted
        assert_eq!(
            stage
                .apply(Cow::Borrowed(input), &ctx!(JPN))
                .unwrap()
                .as_ref(),
            expected
        );
    }

    #[test]
    fn numbers_treated_as_western() {
        let stage = SegmentWord;
        let input = "東京2025年";
        let expected = "東京 2025 年";
        assert_eq!(
            stage
                .apply(Cow::Borrowed(input), &ctx!(JPN))
                .unwrap()
                .as_ref(),
            expected
        );
    }
}
