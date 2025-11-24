// src/stage/unigram_cjk.rs
//! UnigramCJK stage – inserts spaces between consecutive CJK ideographs.
//! Works after SegmentWord. Fully iterator-based and fused for efficiency.

use std::{borrow::Cow, iter::FusedIterator, sync::Arc};

use crate::{
    context::Context,
    stage::{CharMapper, Stage, StageError},
    unicode::is_cjk_han_or_kana,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct CjkUnigram;

impl Stage for CjkUnigram {
    fn name(&self) -> &'static str {
        "unigram_cjk"
    }

    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        Ok(ctx.lang_entry.needs_unigram_cjk() && text.chars().any(is_cjk_han_or_kana))
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

impl CharMapper for CjkUnigram {
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        Some(c)
    }

    fn bind<'a>(&self, text: &'a str, _ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(segment_chars(text.chars()).fuse())
    }
}
fn segment_chars<I>(chars: I) -> impl FusedIterator<Item = char>
where
    I: Iterator<Item = char>,
{
    struct Seg<I> {
        inner: I,
        prev_was_cjk: bool,
        pending: Option<char>,
    }

    impl<I: Iterator<Item = char>> Iterator for Seg<I> {
        type Item = char;

        fn next(&mut self) -> Option<Self::Item> {
            if let Some(ch) = self.pending.take() {
                self.prev_was_cjk = is_cjk_han_or_kana(ch);
                return Some(ch);
            }

            let curr = self.inner.next()?;
            let curr_is_cjk = is_cjk_han_or_kana(curr);

            if self.prev_was_cjk && curr_is_cjk {
                self.pending = Some(curr);
                self.prev_was_cjk = false; // space is not CJK
                return Some(' ');
            }

            self.prev_was_cjk = curr_is_cjk;
            Some(curr)
        }
    }

    impl<I: Iterator<Item = char>> FusedIterator for Seg<I> {}

    Seg {
        inner: chars,
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
    use crate::lang::data::{ARA, DEU, ENG, FRA, JPN, KOR, TUR, ZHO};

    use super::*;

    #[test]
    fn test_unigram_cjk_extended() {
        let cases = &[
            ("", ""),
            ("A", "A"),
            ("中", "中"),
            ("日本語漢字", "日 本 語 漢 字"),
            ("Rust日本語123漢字", "Rust日 本 語123漢 字"),
            ("CJKテスト2025", "CJKテ ス ト2025"),
            ("Hello世界!", "Hello世 界!"),
            (
                "私はRustとPythonが好き😊2025年",
                "私 はRustとPythonが 好 き😊2025年",
            ),
            ("漢字ABC漢字123", "漢 字ABC漢 字123"),
            ("東京2024年", "東 京2024年"),
            ("一二三四五六七八九十", "一 二 三 四 五 六 七 八 九 十"),
            ("日 本", "日 本"),
        ];

        for &(input, expected) in cases {
            let output = segment_cjk_unigram(input);
            assert_eq!(output, expected, "Failed on input: {input}");
        }
    }

    #[test]
    fn test_unigram_cjk_zho_enabled() {
        let stage = CjkUnigram;
        let ctx = Context::new(ZHO);

        assert!(stage.needs_apply("中华人民共和国", &ctx).unwrap());
        assert!(stage.needs_apply("北京大学", &ctx).unwrap());
        assert!(!stage.needs_apply("Hello world", &ctx).unwrap());
    }

    #[test]
    fn test_unigram_cjk_jpn_disabled_by_default() {
        let stage = CjkUnigram;
        let ctx = Context::new(JPN);

        assert!(!stage.needs_apply("日本語", &ctx).unwrap());
        assert!(!stage.needs_apply("最高のプログラミング言語", &ctx).unwrap());
        assert!(!stage.needs_apply("漢字漢字漢字", &ctx).unwrap());
    }

    #[test]
    fn test_unigram_cjk_non_cjk_languages_never_run() {
        let stage = CjkUnigram;
        let languages = [ENG, DEU, FRA, TUR, ARA, KOR];

        for &lang in &languages {
            let ctx = Context::new(lang);
            assert!(!stage.needs_apply("東京大学", &ctx).unwrap());
            assert!(!stage.needs_apply("中华人民共和国", &ctx).unwrap());
        }
    }

    #[test]
    fn test_unigram_cjk_correct_segmentation_when_enabled() {
        let stage = CjkUnigram;
        let ctx = Context::new(ZHO);

        let cases = &[
            ("", ""),
            ("中", "中"),
            ("中国人民", "中 国 人 民"),
            ("中华人民共和国", "中 华 人 民 共 和 国"),
            ("北京大学2025", "北 京 大 学2025"),
            ("编程语言Rust", "编 程 语 言Rust"),
            ("Hello世界你好", "Hello世 界 你 好"),
            ("一二三四五六七八九十", "一 二 三 四 五 六 七 八 九 十"),
        ];

        for &(input, expected) in cases {
            let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(&*result, expected, "Failed on input: {input}");
        }
    }
}
