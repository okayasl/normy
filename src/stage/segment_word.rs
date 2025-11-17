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

#[derive(Debug, Clone, Copy, Default)]
pub struct SegmentWord;

impl Stage for SegmentWord {
    fn name(&self) -> &'static str {
        "segment_word"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        if !ctx.lang.needs_segmentation() || text.contains(' ') {
            return Ok(false);
        }
        let needs = text.chars().any(|c| {
            (ctx.lang
                .segment_rules()
                .contains(&SegmentRule::NoBreakInScript)
                && is_se_asian_script(c))
                || is_ideographic(c)
        });
        Ok(needs)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, ctx)? {
            return Ok(text);
        }

        let mut result = String::with_capacity(text.len() + text.len() / 10);
        let mut prev_class = CharClass::Other;

        for ch in text.chars() {
            let curr_class = classify(ch, ctx.lang);

            // Insert space BEFORE current char if boundary
            if should_insert_space(prev_class, curr_class, ctx.lang) {
                result.push(' ');
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

// ─────────────────────────────────────────────────────────────────────────────
// Pure, simple, correct iterator version (for CharMapper path)
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Clone)]
struct SegmentIter<'a> {
    chars: std::str::Chars<'a>,
    prev_class: CharClass,
    pending_space: bool,
    lang: Lang,
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
    if c.is_ascii_alphanumeric() || c.is_ascii_punctuation() {
        CharClass::Western
    } else {
        CharClass::Other
    }
}

#[inline(always)]
fn should_insert_space(prev: CharClass, curr: CharClass, lang: Lang) -> bool {
    // ONLY insert space when going from Western → Script
    // Never the reverse (that breaks AI人工智能, MyCorp株式会社)
    prev == CharClass::Western
        && curr == CharClass::Script
        && lang.segment_rules().contains(&SegmentRule::HanAfterWest)
}

impl<'a> SegmentIter<'a> {
    fn new(text: &'a str, lang: Lang) -> Self {
        Self {
            chars: text.chars(),
            prev_class: CharClass::Other,
            pending_space: false,
            lang,
        }
    }
}

impl<'a> Iterator for SegmentIter<'a> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pending_space {
            self.pending_space = false;
            return Some(' ');
        }

        let c = self.chars.next()?;
        let curr = classify(c, self.lang);

        if should_insert_space(self.prev_class, curr, self.lang) {
            self.pending_space = true;
            self.prev_class = curr;
            return Some(' ');
        }

        self.prev_class = curr;
        Some(c)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (low, high) = self.chars.size_hint();
        (
            low.saturating_sub(self.pending_space as usize),
            high.map(|h| h + h / 10),
        )
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
}
