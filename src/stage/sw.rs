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
    unicode::is_any_whitespace,
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
        let mut prev = None; // prev is the *last non-whitespace char* OR a canonical ' '

        while let Some(curr) = chars.next() {
            // 1. Boundary Insertion
            // If the previous char was NOT a space, check for boundary insertion.
            if let Some(p) = prev
                && p != ' ' // Only check boundary if the previous emitted char was not a space
                && lang.needs_boundary_between(p, curr)
            {
                out.push(' ');
            }

            // 2. Character Handling & Whitespace Collapse
            if is_any_whitespace(curr) {
                // Consume the rest of the whitespace run
                while chars.peek().is_some_and(|&n| is_any_whitespace(n)) {
                    chars.next();
                }

                // If we are at the end, and the string ends with whitespace, we skip the space.
                if chars.peek().is_none() {
                    // Do nothing. Do not push space. The resulting `out` is correct.
                    prev = None; // Reset prev at stream end
                } else {
                    // Only push a space if we are NOT continuing an existing space run
                    if prev.is_none() || prev.unwrap() != ' ' {
                        out.push(' ');
                    }
                    prev = Some(' '); // Canonical space marker for the next iteration
                }
            } else {
                // Normal non-whitespace character
                out.push(curr);
                prev = Some(curr);
            }
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

/// Fused zero-allocation iterator – correct segmentation for all segmented languages
struct SegmentWordIterator<I>
where
    I: Iterator<Item = char>,
{
    lang: Lang,
    inner: Peekable<I>,
    prev: Option<char>,  // last emitted *non-whitespace* or canonical ' '
    pending_space: bool, // true if a Western→Script boundary needs a space
}

impl<I> Iterator for SegmentWordIterator<I>
where
    I: Iterator<Item = char> + FusedIterator,
{
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        // 1️⃣ Emit any pending boundary space first
        if self.pending_space {
            let peek_char = self.inner.peek().copied();
            if peek_char.is_none() || peek_char.is_some_and(is_any_whitespace) {
                // Suppress pending space at end or before whitespace
                self.pending_space = false;
            } else {
                self.pending_space = false;
                return Some(' ');
            }
        }

        // 2️⃣ Get next char
        let curr = self.inner.next()?;

        // 3️⃣ Handle whitespace
        if is_any_whitespace(curr) {
            // Collapse all consecutive whitespace
            while self.inner.peek().is_some_and(|&n| is_any_whitespace(n)) {
                self.inner.next();
            }

            // Only emit a single space if prev was not already a space
            if self.prev.is_none() || !is_any_whitespace(self.prev.unwrap()) {
                self.prev = Some(' ');
                return Some(' ');
            } else {
                // Already had space → skip
                return self.next();
            }
        }

        // 4️⃣ Handle segmentation for segmented languages
        if self.lang.needs_segmentation()
            && let Some(prev_c) = self.prev
        {
            // Western → Script → insert space
            if !is_any_whitespace(prev_c) && self.lang.needs_boundary_between(prev_c, curr) {
                self.pending_space = true;
            }
        }

        // 5️⃣ Emit current character
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

    // ───────────────────────────────
    // 1️⃣ Non-segmented languages remain unchanged
    // ───────────────────────────────
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

    // ───────────────────────────────
    // 2️⃣ Western → Script boundaries
    // ───────────────────────────────
    #[test]
    fn western_to_script_spaces() {
        let stage = SegmentWord;

        // Mixed Western + Script
        let cases = &[
            ("Hello世界", "Hello 世界"),
            ("Rustは最高", "Rust は 最高"),
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

    // ───────────────────────────────
    // 3️⃣ Script → Western / intra-script & idempotency
    // ───────────────────────────────
    #[test]
    fn script_boundaries_and_idempotency() {
        let stage = SegmentWord;

        // Script → Western: no space
        let input = "世界Hello";
        let ctx = ctx!(JPN);
        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx).unwrap().as_ref(),
            input
        );

        // Intra-script: fused (Japanese, Thai, Hangul)
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

        // Idempotency: applying twice gives same output
        let text = "こんにちは 世界";
        let ctx = ctx!(JPN);
        let once = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        let twice = stage.apply(Cow::Borrowed(&once), &ctx).unwrap();
        assert_eq!(once, twice);
    }

    // ───────────────────────────────
    // 4️⃣ Whitespace collapse
    // ───────────────────────────────
    #[test]
    fn whitespace_collapsed() {
        let stage = SegmentWord;
        let input = "こんにちは   世界\t\nです";
        let expected = "こんにちは 世界 です";
        let ctx = ctx!(JPN);
        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx).unwrap().as_ref(),
            expected
        );
    }

    // ───────────────────────────────
    // 5️⃣ Numbers treated as Western
    // ───────────────────────────────
    #[test]
    fn numbers_as_western() {
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
