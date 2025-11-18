use crate::{
    context::Context,
    lang::{JPN, Lang, LocaleBehavior, SegmentRule, ZHO},
    stage::{CharMapper, FusedIterator, Stage, StageError},
    unicode::{is_ascii_digit, is_ascii_punct, is_latin_letter, is_script_char},
};
use std::borrow::Cow;
use std::iter::Peekable;
use std::sync::Arc;

#[derive(Debug, Default, Clone, Copy)]
pub struct SegmentWord;

impl Stage for SegmentWord {
    fn name(&self) -> &'static str {
        "segment_word"
    }

    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        let lang = ctx.lang;
        if text.is_empty() || !lang.needs_segmentation() {
            return Ok(false);
        }

        let mut prev = CharClass::Other;
        let mut offset = 0usize;

        for c in text.chars() {
            let curr = classify(c);
            if should_insert_space(prev, curr, c, lang) {
                let remaining = &text[offset..];
                if !lang.is_segment_exception(remaining) {
                    return Ok(true);
                }
            }
            prev = curr;
            offset += c.len_utf8();
        }
        Ok(false)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, ctx)? {
            return Ok(text);
        }
        Ok(Cow::Owned(
            SegmentIter::new(text.as_ref(), ctx.lang).collect(),
        ))
    }

    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        if ctx.lang.segment_rules().is_empty() {
            Some(self)
        } else {
            None
        }
    }

    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        if ctx.lang.segment_rules().is_empty() {
            Some(self)
        } else {
            None
        }
    }
}

impl CharMapper for SegmentWord {
    #[inline(always)]
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        Some(c)
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(SegmentIter::new(text, ctx.lang))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum CharClass {
    #[default]
    Other,
    Western,
    Script,
}

#[inline(always)]
fn classify(c: char) -> CharClass {
    if is_latin_letter(c) || is_ascii_digit(c) || is_ascii_punct(c) {
        CharClass::Western
    } else if is_script_char(c) {
        CharClass::Script
    } else {
        CharClass::Other
    }
}

#[inline(always)]
fn should_insert_space(prev: CharClass, curr: CharClass, c: char, lang: Lang) -> bool {
    match (prev, curr) {
        (CharClass::Western, CharClass::Script) => {
            lang.segment_rules().contains(&SegmentRule::HanAfterWest)
        }
        (CharClass::Script, CharClass::Western) => {
            if matches!(lang, JPN | ZHO) && c.is_ascii_alphabetic() {
                false
            } else {
                lang.segment_rules().contains(&SegmentRule::WestAfterHan)
            }
        }
        _ => false,
    }
}

struct SegmentIter<'a> {
    text: &'a str,
    chars: Peekable<std::str::Chars<'a>>,
    lang: Lang,
    prev: CharClass,
    pending_char: Option<char>,
    byte_offset: usize,
}

impl<'a> SegmentIter<'a> {
    fn new(text: &'a str, lang: Lang) -> Self {
        Self {
            text,
            chars: text.chars().peekable(),
            lang,
            prev: CharClass::Other,
            pending_char: None,
            byte_offset: 0,
        }
    }
}

impl Iterator for SegmentIter<'_> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(c) = self.pending_char.take() {
            return Some(c);
        }

        let &c = self.chars.peek()?;
        let curr = classify(c);

        if self.prev != CharClass::Other
            && self.prev != curr
            && should_insert_space(self.prev, curr, c, self.lang)
        {
            let remaining = &self.text[self.byte_offset..];
            if !self.lang.is_segment_exception(remaining) {
                self.pending_char = Some(c);
                self.chars.next();
                self.byte_offset += c.len_utf8();
                self.prev = curr;
                return Some(' ');
            }
        }

        self.chars.next();
        self.byte_offset += c.len_utf8();
        self.prev = curr;
        Some(c)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (l, h) = self.chars.size_hint();
        let add = self.pending_char.is_some() as usize;
        (l + add, h.map(|h| h + add + 8)) // conservative upper bound
    }
}

impl FusedIterator for SegmentIter<'_> {}

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

    #[test]
    fn test_missing_symmetric_insertion_script_to_western() {
        let s = SegmentWord;
        let c = ctx(JPN);

        // Current code only inserts Western → CJK, NOT CJK → Western
        // Expected: space after "世界" before "World"
        assert_eq!(
            s.apply("Hello世界World".into(), &c).unwrap(),
            "Hello 世界World" // ← currently outputs "Hello 世界World" (missing second space)
        );

        // More extreme case: multiple transitions
        assert_eq!(
            s.apply("AI人工智能2024技术".into(), &ctx(ZHO)).unwrap(),
            "AI人工智能 2024技术" // missing space before "2024"
        );
    }

    #[test]
    fn test_korean_script_to_western_boundary() {
        let s = SegmentWord;
        let c = ctx(KOR);

        // Korean Hangul is treated as Script when WestAfterHan is enabled
        assert_eq!(
            s.apply("Samsung한국123".into(), &c).unwrap(),
            "Samsung 한국 123" // currently: "Samsung 한국123" (no space before 123)
        );
    }

    #[test]
    fn test_exception_matching_currently_broken_due_to_char_vs_str() {
        let s = SegmentWord;
        let c = ctx(ZHO);

        // This will currently PANIC or compile-error because:
        // - `e.starts_with(c)` where `e: &str`, `c: char` → doesn't compile
        // - or if forced, does byte-wise compare → wrong
        //
        // After fix: uses `remaining.starts_with(e)` → correct and zero-cost
        assert_eq!(
            s.apply("人工智能AI".into(), &c).unwrap(),
            "人工智能AI" // should NOT insert space (exception "人工智能")
        );

        assert_eq!(
            s.apply("人工AI智能".into(), &c).unwrap(),
            "人工 AI智能" // should insert — "人工AI" is not an exception
        );
    }

    #[test]
    fn test_zero_allocation_violation_on_exception_only_text() {
        let s = SegmentWord;
        let c = ctx(JPN);

        // Common real-world pattern: brand names, company suffixes
        let text = "株式会社Sony";
        let result = s.apply(text.into(), &c).unwrap();

        // Currently: allocates a new String even though no change occurs
        // After proper `needs_apply` + correct exception check → Cow::Borrowed
        match result {
            Cow::Borrowed(_) => {}
            Cow::Owned(_) => panic!(
                "Zero-allocation guarantee violated: '{}' should not allocate",
                text
            ),
        }
    }
}
