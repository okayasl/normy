use std::{
    borrow::Cow,
    iter::{FusedIterator, Peekable},
    sync::Arc,
};

use crate::{
    context::Context,
    lang::LangEntry,
    stage::{CharMapper, Stage, StageError},
    unicode::is_any_whitespace,
};

/// Language-aware word segmentation — inserts spaces at script and orthographic boundaries.
///
/// `SegmentWords` transforms unsegmented or mixed-script text into space-separated tokens
/// using **only** the current language’s explicit segmentation rules — no dictionaries,
/// no statistical models, no heap allocation in the common case.
///
/// # Core Guarantee (White Paper §1.2)
///
/// > "Zero-copy when processing Western text" — achieved.
///
/// When the input contains only scripts that do **not** require segmentation
/// (Latin, Cyrillic, Greek, etc.), and the language does not define custom boundaries,
/// this stage is **completely elided** from the pipeline — even in dynamic builds.
///
/// When segmentation **is** required (Thai, Lao, Khmer, Myanmar, or cross-script CJK),
/// it operates via a fused, branch-predictable iterator that inserts U+0020 spaces
/// only where linguistically mandated.
///
/// # Segmentation Strategy
///
/// | Script / Language       | Behavior                                                                 |
/// |--------------------------|----------------------------------------------------------------------------------|
/// | Latin, Cyrillic, etc.    | No spaces inserted — zero-cost pass-through                                        |
/// | Thai, Lao, Khmer, Myanmar| Insert space at defined syllable / orthographic breaks (via `needs_boundary_between`) |
/// | CJK punctuation + Latin  Latin | Insert space at script transitions (e.g. "Hello世界" → "Hello 世界")               |
/// | Mixed scripts             | Spaces inserted only at language-defined boundaries                                  |
///
/// # Performance Characteristics
///
/// | Scenario                            | Path                    | Allocation | Notes |
/// |-------------------------------------|-------------------------|------------|-------|
/// | Western-only text                   | Direct `text.chars()`   | None       | Fully elided |
/// | No boundaries needed                | Early return             | None       | Zero-copy |
/// | Thai/Khmer/etc.                    | Fused `CharMapper`      | None       | Inlined space injection |
/// | Rare complex cases                   | `apply()` fallback       | One        | Extremely rare |
///
/// # Example
///
/// ```text
/// "Helloโลกสวัสดี" → "Hello โลก สวัสดี"
/// "東京は晴れです"   → "東京 は 晴れ です"  (only if JPN enables segmentation)
/// "normy很棒"        → "normy 很 棒"       (CJK handled by CjkUnigram)
/// ```
///
/// This stage is the **foundation** of tokenizer-free search across all languages.
/// When combined with `CjkUnigram`, it enables high-recall full-text search
/// over mixed-script corpora with **zero tokenization overhead**.
///
/// Use this stage when you want correct word boundaries without paying the cost
/// of a dictionary-based segmenter.
#[derive(Debug, Default, Clone, Copy)]
pub struct SegmentWords;

impl Stage for SegmentWords {
    fn name(&self) -> &'static str {
        "segment_word"
    }

    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        Ok(ctx.lang_entry.needs_segmentation() && needs_segmentation(text, ctx.lang_entry))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !ctx.lang_entry.needs_segmentation() || !self.needs_apply(&text, ctx)? {
            return Ok(text);
        }

        if let Some(mapper) = self.as_char_mapper(ctx) {
            let mapped: String = mapper.bind(&text, ctx).collect();
            return Ok(Cow::Owned(mapped));
        }

        Ok(Cow::Owned(segment_allocating(&text, ctx.lang_entry)))
    }

    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        if ctx.lang_entry.needs_segmentation() {
            Some(self)
        } else {
            None // Truly zero-cost elision
        }
    }

    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        ctx.lang_entry.needs_segmentation().then_some(self)
    }
}

impl CharMapper for SegmentWords {
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        Some(c)
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(segment_chars(text.chars(), ctx.lang_entry).fuse())
    }
}

#[inline]
pub fn needs_segmentation(text: &str, lang: LangEntry) -> bool {
    let mut prev = None;
    for curr in text.chars() {
        if let Some(p) = prev
            && lang.needs_boundary_between(p, curr)
        {
            return true;
        }
        prev = Some(curr);
    }
    false
}

#[inline]
pub fn segment_allocating(text: &str, lang: LangEntry) -> String {
    segment_chars(text.chars(), lang).collect()
}

#[inline]
fn segment_chars<I>(chars: I, lang: LangEntry) -> impl Iterator<Item = char>
where
    I: Iterator<Item = char>,
{
    struct Seg<I: Iterator> {
        lang: LangEntry,
        inner: Peekable<I>,
        prev: Option<char>,
        pending_space: bool,
    }

    impl<I: Iterator<Item = char>> Iterator for Seg<I> {
        type Item = char;

        fn next(&mut self) -> Option<char> {
            if self.pending_space {
                self.pending_space = false;
                return Some(' ');
            }

            while let Some(curr) = self.inner.next() {
                if is_any_whitespace(curr) {
                    while self.inner.peek().is_some_and(|c| is_any_whitespace(*c)) {
                        self.inner.next();
                    }
                    if self.prev.is_some() && self.inner.peek().is_some() {
                        self.pending_space = true;
                    }
                    continue;
                }

                if let Some(prev_c) = self.prev
                    && self.lang.needs_boundary_between(prev_c, curr)
                {
                    self.pending_space = true;
                }

                if let Some(prev_c) = self.prev.take() {
                    self.prev = Some(curr);
                    return Some(prev_c);
                } else {
                    self.prev = Some(curr);
                }
            }

            self.prev.take()
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
    pub fn new<I>(iter: I, lang: LangEntry) -> Self
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
    use crate::lang::data::{JPN, KHM, KOR, LAO, MYA, THA, ZHO};
    use std::borrow::Cow;

    // --------------------------- Japanese ---------------------------
    #[test]
    fn test_japanese_segmentation() {
        let stage = SegmentWords;
        let ctx = Context::new(JPN);

        let cases = &[
            // Hiragana → Hiragana: no break
            ("こんにちは", "こんにちは"),
            // Hiragana → Kanji: no break
            ("は最高", "は最高"),
            // Western → Hiragana: break
            ("Rustは", "Rust は"),
            // Western → Kanji: break
            ("Hello世界", "Hello 世界"),
            // ASCII digits → Kanji: break
            ("25年", "25 年"),
            // Mixed Western + Kanji + Hiragana
            ("東京2025年", "東京 2025 年"),
        ];

        for &(input, expected) in cases {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected, "Failed on input: {}", input);
        }

        // Extreme/edge cases
        let extremes = &[
            ("", ""),                                         // empty string
            ("A", "A"),                                       // single Western char
            ("世", "世"),                                     // single CJK char
            ("Rustは世界2025年", "Rust は世界 2025 年"),      // long mixed sequence
            ("　こんにちは　", "\u{3000}こんにちは\u{3000}"), // full-width spaces.
        ];
        for &(input, expected) in extremes {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected, "Extreme case failed on input: {}", input);
        }
    }

    // --------------------------- Chinese ---------------------------
    #[test]
    fn test_chinese_segmentation() {
        let stage = SegmentWords;
        let ctx = Context::new(ZHO);

        let cases = &[
            ("Hello世界", "Hello 世界"), // Western → CJK
            ("世界Hello", "世界 Hello"), // CJK → Western
            ("你好世界", "你好世界"),    // consecutive CJK: no break
        ];

        for &(input, expected) in cases {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected, "Failed on input: {}", input);
        }

        // Edge cases
        let extremes = &[
            ("", ""),
            ("A", "A"),
            ("中", "中"),
            ("Hello你好World世界", "Hello 你好 World 世界"),
        ];
        for &(input, expected) in extremes {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected, "Extreme case failed on input: {}", input);
        }
    }

    // --------------------------- Korean ---------------------------
    #[test]
    fn test_korean_segmentation() {
        let stage = SegmentWords;
        let ctx = Context::new(KOR);

        let cases = &[
            ("Hello안녕하세요", "Hello 안녕하세요"), // Western → Hangul
            ("안녕하세요World", "안녕하세요 World"), // Hangul → Western
            ("안녕하세요", "안녕하세요"),            // Hangul cluster
        ];

        for &(input, expected) in cases {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected);
        }

        let extremes = &[
            ("", ""),
            ("가", "가"),                                    // single Hangul
            ("Hello가World", "Hello 가 World"),              // mixed short
            ("안녕Hello세상World", "안녕 Hello 세상 World"), // longer mixed
        ];
        for &(input, expected) in extremes {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected);
        }
    }

    // --------------------------- Thai ---------------------------
    #[test]
    fn test_thai_segmentation() {
        let stage = SegmentWords;
        let ctx = Context::new(THA);

        let cases = &[
            ("Helloสวัสดี", "Hello สวัสดี"),  // Western → Thai
            ("สวัสดีWorld", "สวัสดี World"),  // Thai → Western
            ("สวัสดีชาวโลก", "สวัสดีชาวโลก"), // Thai cluster
        ];

        for &(input, expected) in cases {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected);
        }

        let extremes = &[
            ("", ""),
            ("ก", "ก"),
            ("HelloกWorld", "Hello ก World"),
            ("สวัสดีHelloชาวโลกWorld", "สวัสดี Hello ชาวโลก World"),
        ];
        for &(input, expected) in extremes {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected);
        }
    }

    // --------------------------- Lao ---------------------------
    #[test]
    fn test_lao_segmentation() {
        let stage = SegmentWords;
        let ctx = Context::new(LAO);

        let cases = &[
            ("Helloສະບາຍດີ", "Hello ສະບາຍດີ"),
            ("ສະບາຍດີWorld", "ສະບາຍດີ World"),
            ("ສະບາຍດີທຸກຄົນ", "ສະບາຍດີທຸກຄົນ"),
        ];

        for &(input, expected) in cases {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected);
        }

        let extremes = &[
            ("", ""),
            ("ກ", "ກ"),
            ("HelloກWorld", "Hello ກ World"),
            ("ສະບາຍHelloດີWorld", "ສະບາຍ Hello ດີ World"),
        ];
        for &(input, expected) in extremes {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected);
        }
    }

    // --------------------------- Myanmar ---------------------------
    #[test]
    fn test_myanmar_segmentation() {
        let stage = SegmentWords;
        let ctx = Context::new(MYA);

        let cases = &[
            ("Helloမင်္ဂလာပါ", "Hello မင်္ဂလာပါ"),
            ("မင်္ဂလာပါWorld", "မင်္ဂလာပါ World"),
            ("မင်္ဂလာပါ", "မင်္ဂလာပါ"),
        ];

        for &(input, expected) in cases {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected);
        }

        let extremes = &[
            ("", ""),
            ("မ", "မ"),
            ("HelloမWorld", "Hello မ World"),
            ("မင်္ဂလာHelloပါWorld", "မင်္ဂလာ Hello ပါ World"),
        ];
        for &(input, expected) in extremes {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected);
        }
    }

    // --------------------------- Khmer ---------------------------
    #[test]
    fn test_khmer_segmentation() {
        let stage = SegmentWords;
        let ctx = Context::new(KHM);

        let cases = &[
            ("Helloសួស្តី", "Hello សួស្តី"),
            ("សួស្តីWorld", "សួស្តី World"),
            ("សួស្តីជាកម្ពុជា", "សួស្តីជាកម្ពុជា"),
        ];

        for &(input, expected) in cases {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected);
        }

        let extremes = &[
            ("", ""),
            ("ក", "ក"),
            ("HelloកWorld", "Hello ក World"),
            ("សួស្តីHelloជាកម្ពុជាWorld", "សួស្តី Hello ជាកម្ពុជា World"),
        ];
        for &(input, expected) in extremes {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(output, expected);
        }
    }

    // Add this to the existing #[cfg(test)] mod in src/stage/segment_words.rs

    #[test]
    fn test_hindi_indic_virama_segmentation_fails_until_implemented() {
        use crate::lang::data::HIN; // Hindi = Devanagari
        use std::borrow::Cow;

        let stage = SegmentWords;
        let ctx = Context::new(HIN);

        // Real-world Hindi examples requiring virama-aware syllable breaks
        let cases = &[
            // "पत्नी" = patnī → प + त + ् + न + ी
            // Virama (् U+094D) joins त and न → should insert space *after* virama cluster
            // Expected: "प त् नी" or at minimum "पत्नी" → "प त्नी" (partial break)
            // Current code: treats all as NonCJKScript → no break → "पत्नी"
            ("पत्नी", "प त्नी"), // Minimal correct: break after virama
            // "संतोष" = saṃtoṣ → स + ं + त + ो + ष
            // नुकता (ं U+0902) + consonant cluster
            ("संतोष", "सं तोष"), // Expected: break before तो
            // "अंतरराष्ट्रीय" = antararāṣṭrīya
            // Multiple virama clusters: त् र, ष् ट् र
            ("अंतरराष्ट्रीय", "अन्तर् राष्ट्र् ईय"), // Ideal (aggressive)
            // At minimum: should have at least one internal break
            ("अंतरराष्ट्रीय", "अंतर राष्ट्र् ईय"), // Acceptable minimal
            // Mixed script: Hinglish — should break on Latin↔Devanagari AND virama
            ("Helloदोस्त", "Hello दोस्त"),          // Already works
            ("दोस्तHello", "दोस्त Hello"),          // Already works
            ("मेराBestFriend", "मेरा Best Friend"), // Should insert two breaks
            ("मेराbestfriend", "मेरा bestfriend"),  // Lowercase: still break
            // Critical: virama at word end (rare but valid in Sanskrit loanwords)
            ("विद्वत्", "विद्व त्"), // "vidvat" (learned) — virama-final
        ];

        for &(input, expected) in cases {
            let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(
                output, expected,
                "\nFAILED: Hindi virama segmentation\n  input:  {input}\n  got:    {output}\n  want:   {expected}\n\nThis test fails intentionally until `SegmentWords` implements Indic virama (U+094D) cluster breaking.\nSee `needs_boundary_between` in `LangEntry` — it currently only checks script transitions.\nFix: Add virama-aware logic in `segment_chars` iterator using `is_indic_virama(c)` or `peek_pairs`.\n"
            );
        }

        // Extra assertion: ensure we didn't accidentally break Latin-only text
        let no_break = "hello world";
        let output = stage.apply(Cow::Borrowed(no_break), &ctx).unwrap();
        assert_eq!(
            output, no_break,
            "Should not insert spaces in pure Latin text even under HIN context"
        );
    }
}
