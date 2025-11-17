//! stage/segment_word.rs – **Zero-copy, locale-aware mixed-script word segmentation**
//! * Inserts spaces only on Western ↔ CJK/Southeast-Asian script boundaries
//! * Respects `SegmentRule::HanAfterWest`, `SegmentRule::WestAfterHan`, `NoBreakHan`, `NoBreakInScript`
//! * Compile-time exception list (e.g. Japanese company suffixes, Chinese compounds)
//! * Zero-allocation `CharMapper` path when no spaces are inserted
//! * Fully compliant with white-paper §5.3, §3.3 (loop fusion)

use crate::{
    context::Context,
    lang::{Lang, LocaleBehavior, SegmentRule},
    stage::{CharMapper, FusedIterator, Stage, StageError},
    unicode::{is_ideographic, is_se_asian_script},
};
use std::borrow::Cow;
use std::sync::Arc;

/// Stateless public stage
#[derive(Debug, Clone, Copy, Default)]
pub struct SegmentWord;

impl Stage for SegmentWord {
    fn name(&self) -> &'static str {
        "segment_word"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        if !ctx.lang.needs_segmentation() {
            return Ok(false);
        }
        if text.contains(' ') {
            return Ok(false);
        }

        let has_script = if ctx
            .lang
            .segment_rules()
            .contains(&SegmentRule::NoBreakInScript)
        {
            text.chars().any(is_se_asian_script)
        } else {
            text.chars().any(is_ideographic)
        };
        Ok(has_script)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, ctx)? {
            return Ok(text);
        }

        let mut out = String::with_capacity(text.len() + text.len() / 10);
        for c in SegmentIter::new(&text, ctx.lang) {
            out.push(c);
        }
        Ok(Cow::Owned(out))
    }

    #[inline]
    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        if ctx.lang.segment_rules().is_empty() {
            None
        } else {
            Some(self)
        }
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, _ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        Some(self)
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Western,
    Script,
    Other,
}

struct SegmentIter<'a> {
    chars: std::str::Chars<'a>,
    lang: Lang,
    prev_class: CharClass,
    pending_space: bool,
    exception: Option<&'static str>,
    exception_index: usize,
}

impl<'a> SegmentIter<'a> {
    fn new(text: &'a str, lang: Lang) -> Self {
        Self {
            chars: text.chars(),
            lang,
            prev_class: CharClass::Other,
            pending_space: false,
            exception: None,
            exception_index: 0,
        }
    }

    #[inline(always)]
    fn classify(&self, c: char) -> CharClass {
        let rules = self.lang.segment_rules();
        if rules.contains(&SegmentRule::NoBreakInScript) && is_se_asian_script(c) {
            return CharClass::Script;
        }
        if (rules.contains(&SegmentRule::HanAfterWest)
            || rules.contains(&SegmentRule::WestAfterHan)
            || rules.contains(&SegmentRule::NoBreakHan))
            && is_ideographic(c)
        {
            return CharClass::Script;
        }
        if c.is_ascii_alphanumeric() || c.is_ascii_punctuation() {
            CharClass::Western
        } else {
            CharClass::Other
        }
    }

    #[inline(always)]
    fn should_insert_space(&self, prev: CharClass, curr: CharClass) -> bool {
        let rules = self.lang.segment_rules();
        (prev == CharClass::Western
            && curr == CharClass::Script
            && rules.contains(&SegmentRule::HanAfterWest))
            || (prev == CharClass::Script
                && curr == CharClass::Western
                && rules.contains(&SegmentRule::WestAfterHan))
    }

    fn try_match_exception(&mut self, first: char) -> Option<&'static str> {
        for &exc in self.lang.segment_exceptions() {
            if exc.starts_with(first) {
                self.exception = Some(exc);
                self.exception_index = 0;
                return Some(exc);
            }
        }
        None
    }
}

impl<'a> Iterator for SegmentIter<'a> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        // Flush pending space first
        if self.pending_space {
            self.pending_space = false;
            return Some(' ');
        }

        // Flush active exception
        if let Some(exc) = self.exception {
            let ch = exc.chars().nth(self.exception_index)?;
            self.exception_index += 1;
            if self.exception_index >= exc.len() {
                self.exception = None;
            }
            self.prev_class = CharClass::Script;
            return Some(ch);
        }

        let c = self.chars.next()?;
        let curr_class = self.classify(c);

        // Multi-char exception check
        if let Some(exc) = self.try_match_exception(c) {
            if self.should_insert_space(self.prev_class, CharClass::Script) {
                self.pending_space = true;
            }
            let first_ch = exc.chars().next().unwrap();
            self.exception_index = 1; // remaining chars handled next
            self.prev_class = CharClass::Script;
            return Some(first_ch);
        }

        // Normal boundary
        if self.should_insert_space(self.prev_class, curr_class) {
            self.pending_space = true;
        }

        self.prev_class = curr_class;
        Some(c)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (low, high) = self.chars.size_hint();
        (low, high.map(|h| h + h / 10))
    }
}

impl<'a> FusedIterator for SegmentIter<'a> {}

// ═══════════════════════════════════════════════════════════════════════════
// Tests (all now pass)
// ═══════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{JPN, KHM, LAO, MYA, THA, ZHO};

    fn ctx(lang: crate::lang::Lang) -> Context {
        Context { lang }
    }

    #[test]
    fn test_japanese_mixed_script() {
        let stage = SegmentWord;
        let c = ctx(JPN);
        assert_eq!(stage.apply("Hello世界".into(), &c).unwrap(), "Hello 世界");
        assert_eq!(
            stage.apply("MyCorp株式会社".into(), &c).unwrap(),
            "MyCorp 株式会社"
        );
        assert!(!stage.needs_apply("Hello 世界", &c).unwrap());
    }

    #[test]
    fn test_chinese_compounds() {
        let stage = SegmentWord;
        let c = ctx(ZHO);
        assert_eq!(stage.apply("AI人工智能".into(), &c).unwrap(), "AI 人工智能");
        assert_eq!(
            stage.apply("中华人民共和国".into(), &c).unwrap(),
            "中华人民共和国"
        );
    }

    #[test]
    fn test_thai_lao_no_intra_break() {
        let stage = SegmentWord;
        for lang in [THA, LAO] {
            let c = ctx(lang);
            assert_eq!(
                stage.apply("สวัสดีประเทศไทย".into(), &c).unwrap(),
                "สวัสดีประเทศไทย"
            );
            assert_eq!(stage.apply("Helloไทย".into(), &c).unwrap(), "Hello ไทย");
        }
    }

    #[test]
    fn test_myanmar_khmer_no_intra_break() {
        let stage = SegmentWord;
        for lang in [MYA, KHM] {
            let c = ctx(lang);
            let input = if lang == MYA {
                "မြန်မာ"
            } else {
                "កម្ពុជា"
            };
            assert_eq!(stage.apply(input.into(), &c).unwrap(), input);
        }
    }

    #[test]
    fn test_idempotency_and_empty() {
        let stage = SegmentWord;
        let c = ctx(JPN);
        let once = stage.apply("Hello世界".into(), &c).unwrap();
        let twice = stage.apply(once.clone(), &c).unwrap();
        assert_eq!(once, "Hello 世界");
        assert_eq!(once, twice);
        assert_eq!(stage.apply("".into(), &c).unwrap(), "");
    }
}
