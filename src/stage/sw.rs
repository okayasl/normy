// src/stage/segment_word.rs
//! Word segmentation stage – inserts U+0020 spaces only where required by the
//! current language’s rules (CJK unigram, Thai/Lao/Khmer syllable breaks, etc.).
//! Zero-allocation for Western text, fully fused iterator for monomorphised pipelines.
use std::{
    borrow::Cow,
    iter::{FusedIterator, Peekable},
    sync::Arc,
};

use crate::{
    context::Context,
    lang::{Lang, LocaleBehavior},
    stage::{CharMapper, Stage, StageError},
    unicode::is_any_whitespace,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct SegmentWord;

impl Stage for SegmentWord {
    fn name(&self) -> &'static str {
        "segment_word"
    }

    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        Ok(ctx.lang.needs_segmentation() && needs_segmentation(text, ctx.lang))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !ctx.lang.needs_segmentation() || !self.needs_apply(&text, ctx)? {
            return Ok(text);
        }

        if let Some(mapper) = self.as_char_mapper(ctx) {
            let mapped: String = mapper.bind(&text, ctx).collect();
            return Ok(Cow::Owned(mapped));
        }

        Ok(Cow::Owned(segment_allocating(&text, ctx.lang)))
    }

    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        ctx.lang
            .needs_segmentation()
            .then_some(self as &dyn CharMapper)
    }

    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        ctx.lang.needs_segmentation().then_some(self)
    }
}

impl CharMapper for SegmentWord {
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        Some(c)
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(segment_chars(text.chars(), ctx.lang).fuse())
    }
}

#[inline]
pub fn needs_segmentation(text: &str, lang: Lang) -> bool {
    let mut prev = None;
    for curr in text.chars() {
        if let Some(p) = prev
            && lang.needs_boundary_between(p, curr)
        {
            println!(
                "Needs boundary between {} and {} for text {}",
                p, curr, text
            );
            return true;
        }
        prev = Some(curr);
    }
    false
}

#[inline]
pub fn segment_allocating(text: &str, lang: Lang) -> String {
    segment_chars(text.chars(), lang).collect()
}

#[inline]
fn segment_chars<I>(chars: I, lang: Lang) -> impl Iterator<Item = char>
where
    I: Iterator<Item = char>,
{
    struct Seg<I: Iterator> {
        lang: Lang,
        inner: Peekable<I>,
        prev: Option<char>,
        pending_space: bool,
    }

    impl<I: Iterator<Item = char>> Iterator for Seg<I> {
        type Item = char;

        fn next(&mut self) -> Option<char> {
            // --- Case 1: Emit pending artificial space ---
            if self.pending_space {
                println!("Emitting pending space after {:?}", self.prev);
                self.pending_space = false;
                return Some(' ');
            }

            // --- Case 2: Main iteration ---
            while let Some(curr) = self.inner.next() {
                println!("Current char: {:?}", curr);

                // --- collapse whitespace ---
                if is_any_whitespace(curr) {
                    println!("Whitespace detected: {:?}", curr);
                    while self.inner.peek().is_some_and(|c| is_any_whitespace(*c)) {
                        let skipped = self.inner.next().unwrap();
                        println!("Skipping whitespace: {:?}", skipped);
                    }
                    if self.prev.is_some() && self.inner.peek().is_some() {
                        println!(
                            "Inserting space due to collapsed whitespace after {:?}",
                            self.prev
                        );
                        self.pending_space = true;
                    }
                    continue;
                }

                // --- segmentation boundary ---
                if let Some(prev_c) = self.prev
                    && self.lang.needs_boundary_between(prev_c, curr)
                {
                    println!("Boundary detected between {:?} and {:?}", prev_c, curr);
                    self.pending_space = true;
                }

                // --- emit previous char if present ---
                if let Some(prev_c) = self.prev.take() {
                    // We only emit prev_c here; curr will be emitted in next iteration
                    println!("Emitting previous char: {:?}", prev_c);
                    self.prev = Some(curr);
                    return Some(prev_c);
                } else {
                    // first char, store and continue
                    println!("Storing first char: {:?}", curr);
                    self.prev = Some(curr);
                }
            }

            // --- End of iterator: emit last char if present ---
            if let Some(last) = self.prev.take() {
                println!("Emitting last char at end: {:?}", last);
                return Some(last);
            }

            None
        }
    }

    Seg {
        lang,
        inner: chars.peekable(),
        prev: None,
        pending_space: false,
    }
}

/// Iterator wrapper for explicit usage if needed
pub struct SegmentWordIterator {
    inner: Box<dyn FusedIterator<Item = char>>,
}

impl SegmentWordIterator {
    pub fn new<I>(iter: I, lang: Lang) -> Self
    where
        I: Iterator<Item = char> + FusedIterator + 'static,
    {
        Self {
            inner: Box::new(segment_chars(iter, lang).fuse()),
        }
    }
}

impl Iterator for SegmentWordIterator {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl FusedIterator for SegmentWordIterator {}

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
    fn focused_boundary_hiragana_to_kanji() {
        let lang = JPN;
        let stage = SegmentWord;
        let ctx = ctx!(lang);

        // Hiragana to Kanji — no space, normalization-correct
        let input_1 = "は最高";
        assert!(!stage.needs_apply(input_1, &ctx).unwrap());
        assert!(!needs_segmentation(input_1, lang));

        let expected_1 = "は最高";
        assert_eq!(
            segment_allocating(input_1, lang),
            expected_1,
            "Failure on 'は最高'"
        );

        // Hiragana/Hiragana boundary (should *not* segment)
        let input_2 = "こんにちは";
        let expected_2 = "こんにちは";
        assert_eq!(
            segment_allocating(input_2, lang),
            expected_2,
            "Failure on 'こんにちは'"
        );

        // Western to Hiragana (should segment)
        let input_3 = "Rustは";
        let expected_3 = "Rust は";
        assert_eq!(
            segment_allocating(input_3, lang),
            expected_3,
            "Failure on 'Rustは'"
        );
    }

    #[test]
    fn focused_boundary_western_to_cjk() {
        let lang = JPN;

        // ASCII letter to Kanji
        let input_1 = "o世";
        let expected_1 = "o 世";
        assert_eq!(segment_allocating(input_1, lang), expected_1);

        // Number to Kanji
        let input_2 = "25年";
        let expected_2 = "25 年";
        assert_eq!(segment_allocating(input_2, lang), expected_2);

        // Western → Kanji
        let input_3 = "Hello世界";
        let expected_3 = "Hello 世界";
        assert_eq!(segment_allocating(input_3, lang), expected_3);
    }

    #[test]
    fn test_needs_segmentation() {
        assert!(needs_segmentation("Hello世界", JPN));
        assert!(!needs_segmentation("Hello Rust", ENG));
    }

    #[test]
    fn test_segment_allocating_simple() {
        let input = "Hello世界 Rust";
        let output = segment_allocating(input, JPN);
        assert_eq!(output, "Hello 世界 Rust");
    }

    #[test]
    fn test_segment_allocating_whitespace() {
        let input = "こんにちは   世界\t\nです";
        let expected = "こんにちは 世界 です";
        let output = segment_allocating(input, JPN);
        assert_eq!(output, expected);
    }

    #[test]
    fn test_iterator_basic() {
        let input = "Rustは最高";
        let iter = SegmentWordIterator::new(input.chars().fuse(), JPN);
        let output: String = iter.collect();
        assert_eq!(output, "Rust は最高");
    }

    #[test]
    fn non_segmented_languages_unchanged() {
        let stage = SegmentWord;
        let text = "Hello世界 Rust";
        let non_segmented = [ENG, TUR, DEU, FRA, ARA, HEB];

        for &lang in &non_segmented {
            let ctx = ctx!(lang);
            assert_eq!(
                stage.apply(Cow::Borrowed(text), &ctx).unwrap().as_ref(),
                text
            );
        }
    }

    #[test]
    fn western_to_script_spaces() {
        let stage = SegmentWord;

        let cases = &[
            ("Hello世界", "Hello 世界"),
            ("Rustは最高", "Rust は最高"),
            ("東京2025年", "東京 2025 年"),
            ("AIとLLM", "AI と LLM"),
        ];

        for &(input, expected) in cases {
            let ctx = ctx!(JPN);
            assert_eq!(
                stage.apply(Cow::Borrowed(input), &ctx).unwrap().as_ref(),
                expected
            );
        }
    }

    #[test]
    fn script_boundaries_and_idempotency() {
        let stage = SegmentWord;

        let input = "世界 Hello";
        let ctx = ctx!(JPN);
        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx).unwrap().as_ref(),
            input
        );

        let fused_cases = &[
            ("こんにちは世界", JPN),
            ("สวัสดีชาวโลก", THA),
            ("안녕하세요세계", KOR),
        ];

        for &(text, lang) in fused_cases {
            let ctx = ctx!(lang);
            assert_eq!(
                stage.apply(Cow::Borrowed(text), &ctx).unwrap().as_ref(),
                text
            );
        }

        let text = "こんにちは 世界"; // extra space will be normalized away
        let ctx = ctx!(JPN);
        let once = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        let twice = stage.apply(Cow::Borrowed(&once), &ctx).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn whitespace_collapsed_stage() {
        let stage = SegmentWord;
        let input = "こんにちは   世界\t\nです";
        let expected = "こんにちは 世界 です";
        let ctx = ctx!(JPN);
        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx).unwrap().as_ref(),
            expected
        );
    }

    #[test]
    fn numbers_as_western_stage() {
        let stage = SegmentWord;
        let input = "東京2025年";
        let expected = "東京 2025 年";
        let ctx = ctx!(JPN);
        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx).unwrap().as_ref(),
            expected
        );
    }
}
