// src/stage/unigram_cjk.rs
//! UnigramCJK stage – inserts spaces between consecutive CJK ideographs.
//! Works after SegmentWord. Fully iterator-based and fused for efficiency.

use std::{
    borrow::Cow,
    iter::{FusedIterator, Peekable},
    sync::Arc,
};

use crate::{
    context::Context,
    stage::{CharMapper, Stage, StageError},
    unicode::is_cjk_han_or_kana,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct UnigramCJK;

impl Stage for UnigramCJK {
    fn name(&self) -> &'static str {
        "unigram_cjk"
    }

    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        // Only apply if text contains at least one CJK ideograph
        Ok(text.chars().any(is_cjk_han_or_kana))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        Ok(Cow::Owned(segment_cjk_unigram(&text)))
    }

    fn as_char_mapper(&self, _ctx: &Context) -> Option<&dyn CharMapper> {
        Some(self as &dyn CharMapper)
    }

    fn into_dyn_char_mapper(self: Arc<Self>, _ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }
}

impl CharMapper for UnigramCJK {
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        Some(c)
    }

    fn bind<'a>(&self, text: &'a str, _ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(segment_chars(text.chars()).fuse())
    }
}

/// Iterator-based implementation
fn segment_chars<I>(chars: I) -> impl FusedIterator<Item = char>
where
    I: Iterator<Item = char>,
{
    struct Seg<I: Iterator<Item = char>> {
        inner: Peekable<I>,
        prev_was_cjk: bool,
        pending: Option<char>, // Store pending character
    }

    impl<I: Iterator<Item = char>> Iterator for Seg<I> {
        type Item = char;

        fn next(&mut self) -> Option<Self::Item> {
            // First, yield any pending character
            if let Some(ch) = self.pending.take() {
                self.prev_was_cjk = is_cjk_han_or_kana(ch);
                return Some(ch);
            }

            let curr = self.inner.next()?;
            let curr_is_cjk = is_cjk_han_or_kana(curr);

            if self.prev_was_cjk && curr_is_cjk {
                // Need to insert space before curr
                self.pending = Some(curr);
                self.prev_was_cjk = false; // The space we're returning is not CJK!
                return Some(' ');
            }

            self.prev_was_cjk = curr_is_cjk;
            Some(curr)
        }
    }

    impl<I: Iterator<Item = char>> FusedIterator for Seg<I> {}

    Seg {
        inner: chars.peekable(),
        prev_was_cjk: false,
        pending: None,
    }
}

/// Convenience function
pub fn segment_cjk_unigram(text: &str) -> String {
    segment_chars(text.chars()).collect()
}

/// Iterator wrapper for external usage
pub struct UnigramCJKIterator {
    inner: Box<dyn FusedIterator<Item = char>>,
}

impl UnigramCJKIterator {
    pub fn new<I>(iter: I) -> Self
    where
        I: Iterator<Item = char> + FusedIterator + 'static,
    {
        Self {
            inner: Box::new(segment_chars(iter).fuse()),
        }
    }
}

impl Iterator for UnigramCJKIterator {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl FusedIterator for UnigramCJKIterator {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unigram_cjk_basic() {
        let cases = &[
            ("", ""),
            ("中", "中"),
            ("日本語漢字", "日 本 語 漢 字"), // consecutive CJK → spaces inserted
            ("Rust日本語", "Rust日 本 語"),   // no space between 't' and '日' (not both CJK)
            ("私はRustが好きです", "私 はRustが 好 き で す"), // space only between CJK pairs
            ("東京2024年", "東 京2024年"),    // no space between '4' and '年'
            ("漢字", "漢 字"),
            ("一二三四五", "一 二 三 四 五"),
        ];

        for &(input, expected) in cases {
            let output = segment_cjk_unigram(input);
            assert_eq!(output, expected, "Failed on input: {input}");
        }
    }
}
