use crate::{
    context::Context,
    lang::{JPN, Lang, LocaleBehavior, SegmentRule, ZHO},
    stage::{CharMapper, FusedIterator, Stage, StageError},
    unicode::{is_ascii_digit, is_ascii_punct, is_latin_letter, is_script_char},
};
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug, Default, Clone, Copy)]
pub struct SegmentWord;

impl Stage for SegmentWord {
    fn name(&self) -> &'static str {
        "segment_word"
    }

    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        // Early exit if no possible boundary
        let mut has_western = false;
        let mut has_script = false;
        for c in text.chars() {
            match classify(c) {
                CharClass::Western => has_western = true,
                CharClass::Script => has_script = true,
                _ => {}
            }
            if has_western && has_script {
                break;
            }
        }
        if !has_western || !has_script {
            return Ok(false);
        }

        // Only if mixed script, do the real check
        let processed: String = SegmentIter::new(text, ctx.lang).collect();
        Ok(processed != text)
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
    // Treat whitespace explicitly as Other to ensure proper handling
    if c.is_whitespace() {
        return CharClass::Other;
    }
    if is_latin_letter(c) || is_ascii_digit(c) || is_ascii_punct(c) {
        CharClass::Western
    } else if is_script_char(c) {
        CharClass::Script
    } else {
        CharClass::Other
    }
}

/// Check if the text ending at this position matches an exception pattern
#[inline]
fn is_exception_boundary(text_before: &str, lang: Lang) -> bool {
    for exc in lang.segment_exceptions() {
        if text_before.ends_with(exc) {
            return true;
        }
    }
    false
}

#[inline(always)]
fn should_insert_space(
    prev_class: CharClass,
    curr_class: CharClass,
    curr_char: char,
    next_char: Option<char>,
    lang: Lang,
) -> bool {
    match (prev_class, curr_class) {
        (CharClass::Western, CharClass::Script) => {
            lang.segment_rules().contains(&SegmentRule::HanAfterWest)
        }
        (CharClass::Script, CharClass::Western) => {
            if matches!(lang, JPN | ZHO) && curr_char.is_ascii_alphabetic() {
                if let Some(next) = next_char {
                    let next_class = classify(next);
                    next_class == CharClass::Western
                } else {
                    false
                }
            } else {
                lang.segment_rules().contains(&SegmentRule::WestAfterHan)
            }
        }
        _ => false,
    }
}

struct SegmentIter<'a> {
    text: &'a str,
    chars: Vec<char>,
    lang: Lang,
    index: usize,
    byte_offset: usize,
    pending_char: Option<char>,
    last_emitted_class: CharClass,
}

impl<'a> SegmentIter<'a> {
    fn new(text: &'a str, lang: Lang) -> Self {
        Self {
            text,
            chars: text.chars().collect(),
            lang,
            index: 0,
            byte_offset: 0,
            pending_char: None,
            last_emitted_class: CharClass::Other,
        }
    }
}

impl Iterator for SegmentIter<'_> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(c) = self.pending_char.take() {
            return Some(c);
        }
        if self.index >= self.chars.len() {
            return None;
        }

        let c = self.chars[self.index];
        let curr_class = classify(c);
        let next_char = self.chars.get(self.index + 1).copied();

        // === CANONICAL SPACE INSERTION LOGIC ===
        let should_insert = self.last_emitted_class != CharClass::Other
            && self.last_emitted_class != curr_class
            && should_insert_space(self.last_emitted_class, curr_class, c, next_char, self.lang);

        let blocked_by_exception = if should_insert {
            let text_before = &self.text[..self.byte_offset];
            let remaining = &self.text[self.byte_offset..];
            is_exception_boundary(text_before, self.lang)
                || self.lang.is_segment_exception(remaining)
        } else {
            false
        };

        if should_insert && !blocked_by_exception {
            self.pending_char = Some(c);
            self.index += 1;
            self.byte_offset += c.len_utf8();
            self.last_emitted_class = CharClass::Other;
            return Some(' ');
        }

        // === EMIT REAL CHAR ===
        self.index += 1;
        self.byte_offset += c.len_utf8();
        self.last_emitted_class = curr_class;
        Some(c)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.chars.len() - self.index;
        let add = self.pending_char.is_some() as usize;
        (remaining + add, Some(remaining + add + 8))
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
    fn test_idempotency_and_canonical_form() {
        let s = SegmentWord;
        let c = ctx(JPN);

        let inputs = [
            "Hello世界World",
            "Hello 世界World",
            "Hello世界 World",
            "Hello 世界 World",
        ];

        let canonical = "Hello 世界 World";
        for input in inputs {
            let once = s.apply(input.into(), &c).unwrap();
            assert_eq!(once, canonical);
            assert_eq!(s.apply(once.clone(), &c).unwrap(), canonical); // idempotent
        }

        assert_eq!(s.apply("".into(), &c).unwrap(), "");
    }

    #[test]
    fn test_segment_iter_canonical_output() {
        let c = ctx(JPN);
        let iter = SegmentIter::new("Hello世界World", c.lang);
        assert_eq!(iter.collect::<String>(), "Hello 世界 World");

        let iter = SegmentIter::new("MyCorp株式会社", c.lang);
        assert_eq!(iter.collect::<String>(), "MyCorp株式会社");
    }

    #[test]
    fn test_chinese_western_run_no_space_when_internal() {
        let s = SegmentWord;
        let c = ctx(ZHO);

        // Internal Western runs (bounded by CJK) → no space
        assert_eq!(s.apply("人工AI智能".into(), &c).unwrap(), "人工AI智能");
        assert_eq!(
            s.apply("深度学习DL模型".into(), &c).unwrap(),
            "深度学习DL模型"
        );
        assert_eq!(
            s.apply("自然语言NLP处理".into(), &c).unwrap(),
            "自然语言NLP处理"
        );
        assert_eq!(
            s.apply("推荐系统RecSys架构".into(), &c).unwrap(),
            "推荐系统RecSys架构"
        );

        // Western at boundary → space
        assert_eq!(s.apply("AI技术2024".into(), &c).unwrap(), "AI技术 2024");
        assert_eq!(s.apply("2024AI峰会".into(), &c).unwrap(), "2024 AI峰会");

        // Mixed with punctuation
        assert_eq!(
            s.apply("AI(人工智能)技术".into(), &c).unwrap(),
            "AI(人工智能)技术"
        );
    }
    #[test]
    fn test_complex_japanese_exceptions() {
        let s = SegmentWord;
        let c = ctx(JPN);
        assert_eq!(
            s.apply("Tokyo大学とMyCorp株式会社".into(), &c).unwrap(),
            "Tokyo大学と MyCorp株式会社"
        );
    }

    #[test]
    fn test_korean_and_numbers() {
        let s = SegmentWord;
        let c = ctx(KOR);
        assert_eq!(
            s.apply("Samsung한국123".into(), &c).unwrap(),
            "Samsung 한국 123"
        );
        assert_eq!(s.apply("Hello한국".into(), &c).unwrap(), "Hello 한국");
    }

    #[test]
    fn test_punctuation_and_boundaries() {
        let s = SegmentWord;
        let c = ctx(JPN);
        assert_eq!(s.apply("Hello!世界".into(), &c).unwrap(), "Hello! 世界");
        assert_eq!(s.apply("AI,人工".into(), &c).unwrap(), "AI, 人工");
    }

    #[test]
    fn test_only_homogeneous_text_is_unchanged() {
        let s = SegmentWord;
        let c = ctx(JPN);

        assert_eq!(s.apply("日本語文章".into(), &c).unwrap(), "日本語文章");
        assert_eq!(s.apply("Hello World".into(), &c).unwrap(), "Hello World");
    }

    #[test]
    fn test_needs_apply_detects_non_canonical_spacing() {
        let s = SegmentWord;
        let c = ctx(JPN);

        // Already perfectly normalized → no apply
        assert!(!s.needs_apply("Hello 世界 World", &c).unwrap());
        assert!(!s.needs_apply("日本語文章", &c).unwrap());
        assert!(!s.needs_apply("Hello World", &c).unwrap());
        assert!(!s.needs_apply("", &c).unwrap());

        // Needs normalization
        assert!(s.needs_apply("Hello世界World", &c).unwrap());
        assert!(s.needs_apply("Hello 世界World", &c).unwrap());
        assert!(s.needs_apply("Hello世界 World", &c).unwrap());
    }

    #[test]
    fn test_all_languages_preserve_exceptions() {
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
    fn test_whitespace_and_edge_cases() {
        let s = SegmentWord;
        let c = ctx(JPN);
        assert_eq!(s.apply(" ".into(), &c).unwrap(), " ");
        assert_eq!(s.apply(" a ".into(), &c).unwrap(), " a ");
        assert_eq!(s.apply("Hello 世 界".into(), &c).unwrap(), "Hello 世 界");
    }

    #[test]
    fn test_size_hint_is_sane() {
        let c = ctx(JPN);
        let iter = SegmentIter::new("Hello世界World", c.lang);
        let (low, high) = iter.size_hint();
        assert!(low > 0);
        assert!(high.is_some());
        assert!(high.unwrap() >= low);
    }

    #[test]
    fn test_char_mapper_integration() {
        let s = SegmentWord;
        let c = ctx(JPN);
        let result: String = s.bind("Hello世界World", &c).collect();
        assert_eq!(result, "Hello 世界 World");
    }

    #[test]
    fn test_extreme_cases() {
        let s = SegmentWord;
        let c = ctx(JPN);
        let long = "Hello世界".repeat(1000);
        let result = s.apply(long.clone().into(), &c).unwrap();
        assert!(result.contains(" 世界 "));
        assert!(result.len() > long.len());
    }
}
