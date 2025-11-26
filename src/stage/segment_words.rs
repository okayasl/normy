use std::{
    borrow::Cow,
    iter::{FusedIterator, Peekable},
    sync::Arc,
};

use crate::{
    context::Context,
    lang::{LangEntry, SegmentRule},
    stage::{CharMapper, Stage, StageError},
    unicode::{
        CharClass::{self, Cjk, Hangul, Indic, NonCJKScript, Other, SEAsian, Western},
        classify, is_any_whitespace, is_virama, zwsp,
    },
};

/// Language-aware word segmentation — inserts spaces at script and orthographic boundaries.
///
/// `SegmentWords` transforms unsegmented or mixed-script text into space-separated tokens
/// using **only** the current language's explicit segmentation rules — no dictionaries,
/// no statistical models, no heap allocation in the common case.
///
/// # Core Guarantee
///
/// > "Zero-copy when processing Western text"
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
/// | Thai, Lao, Khmer, Myanmar| Insert space at defined syllable / orthographic breaks                            |
/// | CJK + Latin              | Insert space at script transitions (e.g. "Hello世界" → "Hello 世界")               |
/// | Hindi, Tamil (Indic)     | Insert ZWSP after virama+consonant for word-break opportunities                   |
/// | Mixed scripts            | Spaces inserted only at language-defined boundaries                               |
///
/// # Indic Script Handling (Golden Rule)
///
/// For Hindi, Tamil, and other Indic scripts:
/// - Insert **ZWSP** (U+200B) after virama when followed by a consonant (non-final position)
/// - Insert **space** (U+0020) at script transitions (Indic ↔ Western)
/// - No break after word-final virama
///
/// This provides consistent word-break opportunities for search engines, line breaking,
/// and NLP tokenization without preventing ligature rendering in modern fonts.
///
/// # Performance Characteristics
///
/// | Scenario                            | Path                    | Allocation | Notes |
/// |-------------------------------------|-------------------------|------------|-------|
/// | Western-only text                   | Direct `text.chars()`   | None       | Fully elided |
/// | No boundaries needed                | Early return             | None       | Zero-copy |
/// | Thai/Khmer/Indic                   | Fused `CharMapper`      | None       | Inlined space injection |
/// | Rare complex cases                   | `apply()` fallback       | One        | Extremely rare |
///
/// # Example
///
/// ```text
/// "Helloโลกสวัสดี"    → "Hello โลก สวัสดี"
/// "normy很棒"         → "normy 很 棒"
/// "पत्नी"             → "पत्‍नी" (ZWSP after virama)
/// "Helloपत्नी"        → "Hello पत्‍नी" (space + ZWSP)
/// ```
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
            None
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

#[inline(always)]
fn check_boundary_with_classes(
    prev_class: CharClass,
    curr_class: CharClass,
    lang: LangEntry,
) -> bool {
    if prev_class == curr_class {
        return false;
    }
    match (prev_class, curr_class) {
        (Western, Cjk | Hangul | SEAsian | NonCJKScript | Indic | Other) => {
            lang.segment_rules().contains(&SegmentRule::WesternToScript)
        }
        (Cjk | Hangul | SEAsian | NonCJKScript | Indic | Other, Western) => {
            lang.segment_rules().contains(&SegmentRule::ScriptToWestern)
        }
        (
            Cjk | Hangul | SEAsian | NonCJKScript | Indic | Other,
            Cjk | Hangul | SEAsian | NonCJKScript | Indic | Other,
        ) => true,
        _ => false,
    }
}

#[inline]
pub fn needs_segmentation(text: &str, lang: LangEntry) -> bool {
    let mut prev_class: Option<CharClass> = None;
    let mut prev_char: Option<char> = None;

    for curr in text.chars() {
        if is_any_whitespace(curr) {
            continue;
        }

        let curr_class = classify(curr);

        // Check for Indic virama boundary
        if let Some(p_char) = prev_char {
            let p_class = prev_class.unwrap();
            if p_class == Indic && is_virama(p_char) && curr_class == Indic {
                return true;
            }
        }

        if let Some(p_class) = prev_class
            && check_boundary_with_classes(p_class, curr_class, lang)
        {
            return true;
        }

        prev_class = Some(curr_class);
        prev_char = Some(curr);
    }

    false
}

#[inline]
pub fn segment_allocating(text: &str, lang: LangEntry) -> String {
    segment_chars(text.chars(), lang).collect()
}

#[inline]
pub fn segment_chars<I>(chars: I, lang: LangEntry) -> impl Iterator<Item = char>
where
    I: Iterator<Item = char>,
{
    struct Seg<I: Iterator> {
        lang: LangEntry,
        inner: Peekable<I>,
        prev_char: Option<char>,
        prev_class: Option<CharClass>,
        pending_space: Option<char>,
    }

    impl<I: Iterator<Item = char>> Iterator for Seg<I> {
        type Item = char;

        #[inline(always)]
        fn next(&mut self) -> Option<Self::Item> {
            // Emit pending delimiter first (ZWSP or space)
            if let Some(space) = self.pending_space.take() {
                return Some(space);
            }

            loop {
                let curr = match self.inner.next() {
                    Some(c) => c,
                    None => {
                        // Emit pending delimiter if any
                        if let Some(space) = self.pending_space.take() {
                            return Some(space);
                        }
                        // Emit last buffered char
                        if let Some(last) = self.prev_char.take() {
                            return Some(last);
                        }
                        return None;
                    }
                };

                // Skip whitespace
                if is_any_whitespace(curr) {
                    if let Some(prev) = self.prev_char.take() {
                        return Some(prev);
                    }
                    return Some(curr);
                }

                // Ignore ZWJ/ZWNJ (transparent joiners)
                if curr == '\u{200D}' || curr == '\u{200C}' {
                    if self.prev_char.is_some() {
                        // Discard and continue to next character
                        continue;
                    }
                    return Some(curr);
                }

                let curr_class = classify(curr);
                let curr_is_virama = is_virama(curr);

                let (mut need_boundary, mut use_zwsp) = (false, false);

                if let (Some(prev), Some(prev_class)) = (self.prev_char, self.prev_class) {
                    let prev_is_virama = is_virama(prev);

                    // Golden Indic Rule: Insert ZWSP after virama + consonant
                    if prev_class == Indic
                        && prev_is_virama
                        && !curr_is_virama
                        && curr_class == Indic
                    {
                        need_boundary = true;
                        use_zwsp = true;
                    }
                    // Script transition boundary (use space)
                    else if check_boundary_with_classes(prev_class, curr_class, self.lang) {
                        need_boundary = true;
                        use_zwsp = false;
                    }

                    if need_boundary {
                        let delim = if use_zwsp { zwsp() } else { ' ' };
                        self.pending_space = Some(delim);
                    }

                    // Update buffer for next iteration
                    self.prev_char = Some(curr);
                    self.prev_class = Some(curr_class);

                    return Some(prev);
                }

                // First non-whitespace character — buffer it
                self.prev_char = Some(curr);
                self.prev_class = Some(curr_class);
            }
        }
    }

    Seg {
        lang,
        inner: chars.peekable(),
        prev_char: None,
        prev_class: None,
        pending_space: None,
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
    use crate::{
        HIN, TAM,
        context::Context,
        lang::{
            Lang,
            data::{JPN, KHM, KOR, LAO, MYA, THA, ZHO},
        },
    };
    use std::borrow::Cow;

    /// Generic test helper for all languages
    fn run_cases(lang: Lang, cases: &[(&str, &str)]) {
        let stage = SegmentWords;
        let ctx = Context::new(lang);

        for &(input, expected) in cases {
            let out = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(out, expected, "Failed: {} → {}", input, expected);
        }
    }

    #[test]
    fn test_japanese() {
        run_cases(
            JPN,
            &[
                ("こんにちは", "こんにちは"),
                ("は最高", "は最高"),
                ("Rustは", "Rust は"),
                ("Hello世界", "Hello 世界"),
                ("25年", "25 年"),
                ("東京2025年", "東京 2025 年"),
                ("", ""),
                ("A", "A"),
                ("世", "世"),
                ("Rustは世界2025年", "Rust は世界 2025 年"),
                ("\u{3000}こんにちは\u{3000}", "\u{3000}こんにちは\u{3000}"),
            ],
        );
    }

    #[test]
    fn test_chinese() {
        run_cases(
            ZHO,
            &[
                ("Hello世界", "Hello 世界"),
                ("世界Hello", "世界 Hello"),
                ("你好世界", "你好世界"),
                ("", ""),
                ("A", "A"),
                ("中", "中"),
                ("Hello你好World世界", "Hello 你好 World 世界"),
            ],
        );
    }

    #[test]
    fn test_korean() {
        run_cases(
            KOR,
            &[
                ("Hello안녕하세요", "Hello 안녕하세요"),
                ("안녕하세요World", "안녕하세요 World"),
                ("안녕하세요", "안녕하세요"),
                ("", ""),
                ("가", "가"),
                ("Hello가World", "Hello 가 World"),
                ("안녕Hello세상World", "안녕 Hello 세상 World"),
            ],
        );
    }

    #[test]
    fn test_thai() {
        run_cases(
            THA,
            &[
                ("Helloสวัสดี", "Hello สวัสดี"),
                ("สวัสดีWorld", "สวัสดี World"),
                ("สวัสดีชาวโลก", "สวัสดีชาวโลก"),
                ("", ""),
                ("ก", "ก"),
                ("HelloกWorld", "Hello ก World"),
                ("สวัสดีHelloชาวโลกWorld", "สวัสดี Hello ชาวโลก World"),
            ],
        );
    }

    #[test]
    fn test_lao() {
        run_cases(
            LAO,
            &[
                ("Helloສະບາຍດີ", "Hello ສະບາຍດີ"),
                ("ສະບາຍດີWorld", "ສະບາຍດີ World"),
                ("ສະບາຍດີທຸກຄົນ", "ສະບາຍດີທຸກຄົນ"),
            ],
        );
    }

    #[test]
    fn test_myanmar() {
        run_cases(
            MYA,
            &[
                ("Helloမင်္ဂလာပါ", "Hello မင်္ဂလာပါ"),
                ("မင်္ဂလာပါWorld", "မင်္ဂလာပါ World"),
            ],
        );
    }

    #[test]
    fn test_khmer() {
        run_cases(KHM, &[("Helloសួស្តី", "Hello សួស្តី"), ("សួស្តីWorld", "សួស្តី World")]);
    }

    #[test]
    fn test_hindi() {
        run_cases(
            HIN,
            &[
                ("पत्नी", "पत्\u{200B}नी"),
                (
                    "प\u{094D}त\u{094D}नी",
                    "प\u{094D}\u{200B}त\u{094D}\u{200B}नी",
                ),
                ("सन्तोष", "सन्\u{200B}तोष"),
                ("विद्वत्", "विद्\u{200B}वत्"),
                ("रामायण", "रामायण"),
                ("Helloपत्नी", "Hello पत्\u{200B}नी"),
                ("पत्नीHello", "पत्\u{200B}नी Hello"),
            ],
        );
    }

    #[test]
    fn test_tamil() {
        run_cases(
            TAM,
            &[
                ("பற்றி", "பற்\u{200B}றி"),
                ("தமிழ்", "தமிழ்"),
                ("அக்கா", "அக்\u{200B}கா"),
                ("Helloதமிழ்", "Hello தமிழ்"),
                ("தமிழ்World", "தமிழ் World"),
            ],
        );
    }
}
