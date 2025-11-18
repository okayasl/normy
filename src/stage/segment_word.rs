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
        let mut result = String::with_capacity(text.len() + text.len() / 10);
        let mut prev_class = CharClass::Other;
        let text_str = text.as_ref();
        let char_indices = text_str.char_indices();

        for (idx, ch) in char_indices {
            let curr_class = classify(ch, ctx.lang);

            // Insert space BEFORE current char if boundary
            if should_insert_space(prev_class, curr_class, ctx.lang) {
                // Check if current position matches any exception
                let remaining = &text_str[idx..];

                let is_exception = ctx
                    .lang
                    .segment_exceptions()
                    .iter()
                    .any(|&e| remaining.starts_with(e));

                if !is_exception {
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

// ─────────────────────────────────────────────────────────────────────────────
// Pure, simple, correct iterator version (for CharMapper path)
// ─────────────────────────────────────────────────────────────────────────────

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

#[derive(Clone)]
struct SegmentIter<'a> {
    chars: std::str::Chars<'a>,
    prev_class: CharClass,
    pending_space: bool,
    pending_char: Option<char>, // ADD THIS FIELD
    lang: Lang,
}

impl<'a> SegmentIter<'a> {
    fn new(text: &'a str, lang: Lang) -> Self {
        Self {
            chars: text.chars(),
            prev_class: CharClass::Other,
            pending_space: false,
            pending_char: None, // ADD THIS
            lang,
        }
    }
}

impl<'a> Iterator for SegmentIter<'a> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        // First, return any pending space
        if self.pending_space {
            self.pending_space = false;
            return Some(' ');
        }

        // Then, return any pending char (that was saved before the space)
        if let Some(c) = self.pending_char.take() {
            return Some(c);
        }

        let c = self.chars.next()?;
        let curr = classify(c, self.lang);

        if should_insert_space(self.prev_class, curr, self.lang) {
            // Check if current char + remaining text starts with any exception
            let is_exception = self.lang.segment_exceptions().iter().any(|&e| {
                // The exception must start with current char
                if !e.starts_with(c) {
                    return false;
                }

                // Check if the rest of the exception matches what follows
                let suffix = &e[c.len_utf8()..];
                self.chars.as_str().starts_with(suffix)
            });

            if !is_exception {
                // Save the current char and insert space before it
                self.pending_char = Some(c);
                self.pending_space = true;
                self.prev_class = curr;
                // Return the space first, char will come on next call
                return self.next();
            }
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

    // ═══════════════════════════════════════════════════════════════════════
    // SegmentIter (CharMapper path) tests
    // ═══════════════════════════════════════════════════════════════════════

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

        // "互联网" is an exception, so no space before it
        let iter = SegmentIter::new("GitHub互联网平台", c.lang);
        let result: String = iter.collect();
        assert_eq!(result, "GitHub互联网平台");
    }

    #[test]
    fn test_segment_iter_thai_exception() {
        let c = ctx(THA);
        // "ประเทศไทย" is an exception, so NO space
        let iter = SegmentIter::new("Visitประเทศไทย", c.lang);
        let result: String = iter.collect();
        assert_eq!(result, "Visitประเทศไทย"); // NO space because exception
    }

    #[test]
    fn test_segment_iter_size_hint() {
        let c = ctx(JPN);
        let iter = SegmentIter::new("Hello世界", c.lang);
        let (low, high) = iter.size_hint();
        // Should have reasonable bounds
        assert!(low > 0);
        assert!(high.is_some());
        assert!(high.unwrap() >= low);
    }

    #[test]
    fn test_char_mapper_integration() {
        let s = SegmentWord;
        let c = ctx(JPN);

        // Test the CharMapper path through bind
        let iter = s.bind("MyCorp株式会社", &c);
        let result: String = iter.collect();
        assert_eq!(result, "MyCorp株式会社");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Edge cases
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_multiple_exceptions_in_text() {
        let s = SegmentWord;
        let c = ctx(JPN);
        // "大学" is an exception, so Tokyo大学 should NOT get space
        // "株式会社" is also an exception
        assert_eq!(
            s.apply("Tokyo大学とMyCorp株式会社".into(), &c).unwrap(),
            "Tokyo大学とMyCorp株式会社"
        );
    }

    #[test]
    fn test_exception_at_start() {
        let s = SegmentWord;
        let c = ctx(ZHO);
        // "人工智能" at start is exception, no space before it
        assert_eq!(
            s.apply("人工智能AIis cool".into(), &c).unwrap(),
            "人工智能AIis cool"
        );
    }

    #[test]
    fn test_exception_partial_match() {
        let s = SegmentWord;
        let c = ctx(ZHO);
        // "人工" is prefix of exception "人工智能", but not complete
        // Should insert space since full exception doesn't match
        assert_eq!(s.apply("AI人工".into(), &c).unwrap(), "AI 人工");
        // But full exception should NOT get space
        assert_eq!(s.apply("AI人工智能".into(), &c).unwrap(), "AI人工智能");
    }

    #[test]
    fn test_multiple_western_segments() {
        let s = SegmentWord;
        let c = ctx(JPN);
        // Use characters that are NOT exceptions
        assert_eq!(
            s.apply("Hello世界World人生".into(), &c).unwrap(),
            "Hello 世界World 人生"
        );
    }

    #[test]
    fn test_consecutive_boundaries() {
        let s = SegmentWord;
        let c = ctx(ZHO);
        // a世b界c should become "a 世b 界c"
        assert_eq!(s.apply("a世b界c".into(), &c).unwrap(), "a 世b 界c");
    }

    #[test]
    fn test_numbers_and_cjk() {
        let s = SegmentWord;
        let c = ctx(ZHO);
        assert_eq!(s.apply("2024年中国".into(), &c).unwrap(), "2024 年中国");
        // AI2023 ends in digit (Western), so space before 人工智能
        // But 人工智能 is exception!
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

    // ═══════════════════════════════════════════════════════════════════════
    // Language-specific tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_korean_hangul() {
        let s = SegmentWord;
        let c = ctx(KOR);
        // Hangul is ideographic, should trigger segmentation
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
        // Thai exception: ประเทศไทย - NO space because it's an exception
        assert_eq!(
            s.apply("Visitประเทศไทย".into(), &c).unwrap(),
            "Visitประเทศไทย" // NO space
        );
        // Non-exception Thai - DOES get space
        assert_eq!(s.apply("Helloสวัสดี".into(), &c).unwrap(), "Hello สวัสดี");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Boundary conditions
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_only_cjk_no_segmentation() {
        let s = SegmentWord;
        let c = ctx(JPN);
        // Pure CJK text should not be segmented
        assert_eq!(s.apply("日本語文章".into(), &c).unwrap(), "日本語文章");
    }

    #[test]
    fn test_only_western_no_segmentation() {
        let s = SegmentWord;
        let c = ctx(JPN);
        // Pure Western text should not be segmented
        assert_eq!(s.apply("Hello World".into(), &c).unwrap(), "Hello World");
    }

    #[test]
    fn test_needs_apply_logic() {
        let s = SegmentWord;

        // Should need segmentation for mixed script
        let c = ctx(JPN);
        assert!(s.needs_apply("Hello世界", &c).unwrap());

        // Should NOT need segmentation if already has spaces
        assert!(!s.needs_apply("Hello 世界", &c).unwrap());

        // Should NOT need segmentation for pure Western
        assert!(!s.needs_apply("Hello World", &c).unwrap());

        // Should need segmentation for CJK
        assert!(s.needs_apply("日本語", &c).unwrap());
    }
}
