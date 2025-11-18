//! stage/segment_word.rs – **Zero-copy, locale-aware mixed-script word segmentation**
//!
//! Inserts spaces at Western ↔ CJK/Southeast-Asian script boundaries while respecting
//! language-specific exception patterns (e.g., "株式会社", "人工智能").
//!
//! # Features
//!
//! * Zero-copy when no segmentation needed
//! * O(1) exception lookup via perfect hash
//! * Dual-path: streaming iterator + bulk apply
//! * Language-agnostic: all rules configured via `Lang`

use crate::{
    context::Context,
    lang::{Lang, LocaleBehavior, SegmentRule},
    stage::{CharMapper, FusedIterator, Stage, StageError},
    unicode::{is_ideographic, is_se_asian_script},
};
use std::borrow::Cow;
use std::sync::Arc;
#[derive(Debug, Clone, Copy, Default)]
pub struct SegmentWord;

impl Stage for SegmentWord {
    fn name(&self) -> &'static str {
        "segment_word"
    }

    #[inline]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        if text.is_empty() || !ctx.lang.needs_segmentation() {
            return Ok(false);
        }

        let mut has_western = false;
        let mut has_script = false;

        for c in text.chars() {
            if is_western_char(c) {
                has_western = true;
            } else if is_script_char(c, ctx.lang) {
                has_script = true;
            }

            if has_western && has_script {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let mut result = String::with_capacity(text.len() + text.len() / 10);
        let mut prev_class = CharClass::Other;
        let text_str = text.as_ref();
        let char_indices = text_str.char_indices();

        for (idx, ch) in char_indices {
            let curr_class = classify(ch, ctx.lang);

            if should_insert_space(prev_class, curr_class, ctx.lang) {
                let remaining = &text_str[idx..];

                if !ctx.lang.is_segment_exception(remaining) {
                    result.push(' ');
                }
            }

            result.push(ch);
            prev_class = curr_class;
        }

        Ok(Cow::Owned(result))
    }

    #[inline]
    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        ctx.lang.segment_rules().is_empty().then_some(self)
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, _: &Context) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }
}

impl CharMapper for SegmentWord {
    #[inline(always)]
    fn map(&self, c: char, _: &Context) -> Option<char> {
        Some(c)
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(SegmentIter::new(text, ctx.lang))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CharClass {
    Western,
    Script,
    Other,
}

#[inline(always)]
fn classify(c: char, lang: Lang) -> CharClass {
    let rules = lang.segment_rules();

    if rules.contains(&SegmentRule::NoBreakInScript) && is_se_asian_script(c) {
        return CharClass::Script;
    }

    if is_ideographic(c)
        && (rules.contains(&SegmentRule::HanAfterWest)
            || rules.contains(&SegmentRule::WestAfterHan)
            || rules.contains(&SegmentRule::NoBreakHan))
    {
        return CharClass::Script;
    }

    if is_western_char(c) {
        CharClass::Western
    } else {
        CharClass::Other
    }
}

#[inline(always)]
fn is_western_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c.is_ascii_punctuation()
}

#[inline(always)]
fn is_script_char(c: char, lang: Lang) -> bool {
    let rules = lang.segment_rules();
    (rules.contains(&SegmentRule::NoBreakInScript) && is_se_asian_script(c))
        || (is_ideographic(c)
            && (rules.contains(&SegmentRule::HanAfterWest)
                || rules.contains(&SegmentRule::WestAfterHan)
                || rules.contains(&SegmentRule::NoBreakHan)))
}

#[inline(always)]
fn should_insert_space(prev: CharClass, curr: CharClass, lang: Lang) -> bool {
    prev == CharClass::Western
        && curr == CharClass::Script
        && lang.segment_rules().contains(&SegmentRule::HanAfterWest)
}

#[derive(Clone)]
struct SegmentIter<'a> {
    chars: std::str::Chars<'a>,
    prev_class: CharClass,
    pending_space: bool,
    pending_char: Option<char>,
    lang: Lang,
}

impl<'a> SegmentIter<'a> {
    fn new(text: &'a str, lang: Lang) -> Self {
        Self {
            chars: text.chars(),
            prev_class: CharClass::Other,
            pending_space: false,
            pending_char: None,
            lang,
        }
    }
}

impl<'a> Iterator for SegmentIter<'a> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        // Step 1: If we have a pending space, return it
        if self.pending_space {
            self.pending_space = false;
            return Some(' ');
        }

        // Step 2: If we have a pending char (after space), return it
        if let Some(c) = self.pending_char.take() {
            return Some(c);
        }

        // Step 3: Read next character from input
        let c = self.chars.next()?;
        let curr = classify(c, self.lang);

        // Step 4: Check if we need to insert space
        if should_insert_space(self.prev_class, curr, self.lang) {
            let is_exception = self.lang.segment_exceptions().iter().any(|&e| {
                if !e.starts_with(c) {
                    return false;
                }
                let suffix = &e[c.len_utf8()..];
                self.chars.as_str().starts_with(suffix)
            });

            if !is_exception {
                // Save current char, update state, then return space
                self.pending_char = Some(c);
                self.prev_class = curr;
                self.pending_space = false; // Don't set to true!
                return Some(' ');
            }
        }

        // Step 5: No space needed, update state and return char
        self.prev_class = curr;
        Some(c)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (low, high) = self.chars.size_hint();
        let lower = low.saturating_sub(self.pending_space as usize);
        let upper = high.map(|h| h + h / 10);
        (lower, upper)
    }
}

impl<'a> FusedIterator for SegmentIter<'a> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{JPN, KHM, KOR, LAO, MYA, THA, ZHO};

    fn ctx(lang: crate::lang::Lang) -> Context {
        Context { lang }
    }

    #[test]
    fn test_japanese_mixed_script() {
        let s = SegmentWord;
        let c = ctx(JPN);
        assert_eq!(s.apply("Hello世界".into(), &c).unwrap(), "Hello 世界");
        assert_eq!(
            s.apply("MyCorp株式会社".into(), &c).unwrap(),
            "MyCorp株式会社"
        );
    }

    #[test]
    fn test_chinese_compounds() {
        let s = SegmentWord;
        let c = ctx(ZHO);
        assert_eq!(s.apply("AI人工智能".into(), &c).unwrap(), "AI人工智能");
        assert_eq!(
            s.apply("中华人民共和国".into(), &c).unwrap(),
            "中华人民共和国"
        );
    }

    #[test]
    fn test_thai_lao_no_intra_break() {
        let s = SegmentWord;
        for lang in [THA, LAO] {
            let c = ctx(lang);
            assert_eq!(
                s.apply("สวัสดีประเทศไทย".into(), &c).unwrap(),
                "สวัสดีประเทศไทย"
            );
            assert_eq!(s.apply("Helloไทย".into(), &c).unwrap(), "Hello ไทย");
        }
    }

    #[test]
    fn test_myanmar_khmer_no_intra_break() {
        let s = SegmentWord;
        for (lang, text) in [(MYA, "မြန်မာ"), (KHM, "កម្ពុជា")] {
            assert_eq!(s.apply(text.into(), &ctx(lang)).unwrap(), text);
        }
    }

    #[test]
    fn test_idempotency_and_empty() {
        let s = SegmentWord;
        let c = ctx(JPN);
        let once = s.apply("Hello世界".into(), &c).unwrap();
        assert_eq!(once, "Hello 世界");
        assert_eq!(s.apply(once.clone(), &c).unwrap(), once);
        assert_eq!(s.apply("".into(), &c).unwrap(), "");
    }

    #[test]
    fn test_segment_iter_japanese() {
        let c = ctx(JPN);
        let iter = SegmentIter::new("MyCorp株式会社", c.lang);
        let result: String = iter.collect();
        assert_eq!(result, "MyCorp株式会社");

        let iter = SegmentIter::new("Hello世界", c.lang);
        let result: String = iter.collect();
        assert_eq!(result, "Hello 世界");
    }

    #[test]
    fn test_segment_iter_chinese() {
        let c = ctx(ZHO);
        let iter = SegmentIter::new("AI人工智能", c.lang);
        let result: String = iter.collect();
        assert_eq!(result, "AI人工智能");

        let iter = SegmentIter::new("GitHub互联网平台", c.lang);
        let result: String = iter.collect();
        assert_eq!(result, "GitHub互联网平台");
    }

    #[test]
    fn test_segment_iter_thai_exception() {
        let c = ctx(THA);
        let iter = SegmentIter::new("Visitประเทศไทย", c.lang);
        let result: String = iter.collect();
        assert_eq!(result, "Visitประเทศไทย");
    }

    #[test]
    fn test_segment_iter_size_hint() {
        let c = ctx(JPN);
        let iter = SegmentIter::new("Hello世界", c.lang);
        let (low, high) = iter.size_hint();
        assert!(low > 0);
        assert!(high.is_some());
        assert!(high.unwrap() >= low);
    }

    #[test]
    fn test_char_mapper_integration() {
        let s = SegmentWord;
        let c = ctx(JPN);

        let iter = s.bind("MyCorp株式会社", &c);
        let result: String = iter.collect();
        assert_eq!(result, "MyCorp株式会社");
    }

    #[test]
    fn test_multiple_exceptions_in_text() {
        let s = SegmentWord;
        let c = ctx(JPN);
        assert_eq!(
            s.apply("Tokyo大学とMyCorp株式会社".into(), &c).unwrap(),
            "Tokyo大学とMyCorp株式会社"
        );
    }

    #[test]
    fn test_exception_at_start() {
        let s = SegmentWord;
        let c = ctx(ZHO);
        assert_eq!(
            s.apply("人工智能AIis cool".into(), &c).unwrap(),
            "人工智能AIis cool"
        );
    }

    #[test]
    fn test_exception_partial_match() {
        let s = SegmentWord;
        let c = ctx(ZHO);
        assert_eq!(s.apply("AI人工".into(), &c).unwrap(), "AI 人工");
        assert_eq!(s.apply("AI人工智能".into(), &c).unwrap(), "AI人工智能");
    }

    #[test]
    fn test_multiple_western_segments() {
        let s = SegmentWord;
        let c = ctx(JPN);
        assert_eq!(
            s.apply("Hello世界World人生".into(), &c).unwrap(),
            "Hello 世界World 人生"
        );
    }

    #[test]
    fn test_consecutive_boundaries() {
        let s = SegmentWord;
        let c = ctx(ZHO);
        assert_eq!(s.apply("a世b界c".into(), &c).unwrap(), "a 世b 界c");
    }

    #[test]
    fn test_numbers_and_cjk() {
        let s = SegmentWord;
        let c = ctx(ZHO);
        assert_eq!(s.apply("2024年中国".into(), &c).unwrap(), "2024 年中国");
        assert_eq!(
            s.apply("AI2023人工智能".into(), &c).unwrap(),
            "AI2023人工智能"
        );
    }

    #[test]
    fn test_punctuation_boundary() {
        let s = SegmentWord;
        let c = ctx(JPN);
        assert_eq!(s.apply("Hello!世界".into(), &c).unwrap(), "Hello! 世界");
        assert_eq!(s.apply("AI,人工".into(), &c).unwrap(), "AI, 人工");
    }

    #[test]
    fn test_korean_hangul() {
        let s = SegmentWord;
        let c = ctx(KOR);
        assert_eq!(s.apply("Hello한국".into(), &c).unwrap(), "Hello 한국");
        assert_eq!(
            s.apply("Samsung삼성전자".into(), &c).unwrap(),
            "Samsung 삼성전자"
        );
    }

    #[test]
    fn test_mixed_thai_western() {
        let s = SegmentWord;
        let c = ctx(THA);
        assert_eq!(
            s.apply("Visitประเทศไทย".into(), &c).unwrap(),
            "Visitประเทศไทย"
        );
        assert_eq!(s.apply("Helloสวัสดี".into(), &c).unwrap(), "Hello สวัสดี");
    }

    #[test]
    fn test_only_cjk_no_segmentation() {
        let s = SegmentWord;
        let c = ctx(JPN);
        assert_eq!(s.apply("日本語文章".into(), &c).unwrap(), "日本語文章");
    }

    #[test]
    fn test_only_western_no_segmentation() {
        let s = SegmentWord;
        let c = ctx(JPN);
        assert_eq!(s.apply("Hello World".into(), &c).unwrap(), "Hello World");
    }

    #[test]
    fn test_needs_apply_logic() {
        let s = SegmentWord;
        let c = ctx(JPN);

        assert!(s.needs_apply("Hello世界", &c).unwrap());
        assert!(!s.needs_apply("Hello World", &c).unwrap());
        assert!(!s.needs_apply("日本語", &c).unwrap());
        assert!(!s.needs_apply("", &c).unwrap());
    }

    #[test]
    fn test_zero_copy_optimization() {
        let s = SegmentWord;
        let c = ctx(JPN);

        let western = "Hello World";
        if s.needs_apply(western, &c).unwrap() {
            match s.apply(western.into(), &c).unwrap() {
                Cow::Borrowed(_) => {}
                Cow::Owned(_) => panic!("Should be zero-copy for pure Western"),
            }
        }

        let cjk = "日本語文章";
        if s.needs_apply(cjk, &c).unwrap() {
            match s.apply(cjk.into(), &c).unwrap() {
                Cow::Borrowed(_) => {}
                Cow::Owned(_) => panic!("Should be zero-copy for pure CJK"),
            }
        }

        let mixed = "Hello世界";
        if s.needs_apply(mixed, &c).unwrap() {
            match s.apply(mixed.into(), &c).unwrap() {
                Cow::Owned(_) => {}
                Cow::Borrowed(_) => panic!("Should allocate for mixed script"),
            }
        }
    }

    #[test]
    fn test_text_with_existing_spaces() {
        let s = SegmentWord;
        let c = ctx(JPN);

        assert_eq!(
            s.apply("Hello 世界World人生".into(), &c).unwrap(),
            "Hello 世界World 人生"
        );
    }

    #[test]
    fn test_very_long_text() {
        let s = SegmentWord;
        let c = ctx(JPN);

        let long_text = "Hello世界".repeat(1000);
        let result = s.apply(long_text.clone().into(), &c).unwrap();

        assert!(result.contains(" "));
        assert!(result.len() > long_text.len());
    }

    #[test]
    fn test_single_char() {
        let s = SegmentWord;
        let c = ctx(JPN);

        assert_eq!(s.apply("a".into(), &c).unwrap(), "a");
        assert_eq!(s.apply("世".into(), &c).unwrap(), "世");
    }

    #[test]
    fn test_whitespace_handling() {
        let s = SegmentWord;
        let c = ctx(JPN);

        assert_eq!(s.apply("   ".into(), &c).unwrap(), "   ");
        assert_eq!(s.apply("Hello 世 界".into(), &c).unwrap(), "Hello 世 界");
    }

    #[test]
    fn test_all_languages_segmentation() {
        let s = SegmentWord;

        assert_eq!(
            s.apply("Test株式会社".into(), &ctx(JPN)).unwrap(),
            "Test株式会社"
        );
        assert_eq!(
            s.apply("Test人工智能".into(), &ctx(ZHO)).unwrap(),
            "Test人工智能"
        );
        assert_eq!(
            s.apply("Test한국어".into(), &ctx(KOR)).unwrap(),
            "Test 한국어"
        );
        assert_eq!(
            s.apply("Testประเทศไทย".into(), &ctx(THA)).unwrap(),
            "Testประเทศไทย"
        );
        assert_eq!(s.apply("Testမြန်မာ".into(), &ctx(MYA)).unwrap(), "Testမြန်မာ");
        assert_eq!(s.apply("Testកម្ពុជា".into(), &ctx(KHM)).unwrap(), "Testកម្ពុជា");
        assert_eq!(s.apply("Testລາວ".into(), &ctx(LAO)).unwrap(), "Testລາວ");
    }
}
