use std::{borrow::Cow, iter::FusedIterator, sync::Arc};

use crate::{
    DEU, ENG, FRA, HIN, JPN, KOR, SPA, ZHO,
    context::Context,
    lang::{Lang, LangEntry, SegmentRule},
    stage::{CharMapper, Stage, StageError},
    testing::stage_contract::StageTestConfig,
    unicode::{
        CharClass::{self, Cjk, Hangul, Indic, NonCJKScript, Other, SEAsian, Western},
        classify, is_any_whitespace, is_virama, should_prevent_indic_break, zwsp,
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
/// ## By Script and Language
///
/// | Script / Language          | Behavior                                                                 |
/// |----------------------------|--------------------------------------------------------------------------|
/// | Latin, Cyrillic, etc.      | No spaces inserted — zero-cost pass-through                             |
/// | **Chinese (ZH)**           | **Unigram breaking**: space between EVERY CJK character                 |
/// | **Japanese (JA)**          | Space at script transitions ONLY (kanji+kana stay together)             |
/// | **Korean (KO)**            | Space at script transitions ONLY (Hangul blocks stay together)          |
/// | Thai, Lao, Khmer, Myanmar  | Space at script transitions (no dictionary-based syllable breaking)     |
/// | Hindi, Tamil (Indic)       | ZWSP after virama+consonant; space at script transitions                |
///
/// ## Script Transition Rules
///
/// Spaces are inserted when transitioning between different script families:
/// - Western (Latin, digits) ↔ CJK/Hangul/Southeast Asian/Indic
/// - Between different non-Western scripts (e.g., Thai ↔ Khmer)
///
/// **Exception**: Whitespace, ZWJ (U+200D), and ZWNJ (U+200C) are transparent and reset boundaries.
///
/// # CJK Unigram Mode (Chinese Only)
///
/// For Chinese (`unigram_cjk = true`), **every consecutive CJK ideograph becomes a separate token**:
///
/// ```text
/// Input:  "你好世界"
/// Output: "你 好 世 界"  (4 tokens)
/// ```
///
/// This aggressive tokenization enables downstream processors to handle Chinese text
/// without dictionary lookup, treating each character as a semantic unit.
///
/// **Japanese and Korean do NOT use unigram mode** (`unigram_cjk = false`):
///
/// ```text
/// Japanese: "こんにちは世界" → "こんにちは世界"  (no spaces within Japanese)
/// Korean:   "안녕하세요세계"   → "안녕하세요세계"  (no spaces within Korean)
/// ```
///
/// Mixed script examples:
///
/// ```text
/// Chinese:  "Hello世界"    → "Hello 世 界"     (transition + unigram)
/// Japanese: "Hello世界"    → "Hello 世界"     (transition only)
/// Chinese:  "你好World"    → "你 好 World"    (unigram + transition)
/// Japanese: "世界World"    → "世界 World"     (transition only)
/// ```
///
/// # Indic Script Handling (Linguistic Heuristic)
///
/// For Indic scripts, segmentation applies a character-based rule to manage word-break opportunities.
///
/// The rules operate as follows:
///
/// 1.  **Universal ZWSP Break:** A ZWSP (U+200B) is inserted after a **virama** when followed by a consonant.
///     This provides essential break points for tokenization and line wrapping.
///     
///     ```text
///     Tamil:  "பற்றி" → "பற்\u{200B}றி"
///     ```
///
/// 2.  **Devanagari (Hindi) Exception:** For **Devanagari (Hindi)**, a minimal, zero-cost **heuristic** prevents
///     ZWSP insertion where the **virama** is followed by a consonant known to form a mandatory, non-breaking
///     **conjunct** (specifically: `र` /ra/, `य` /ya/, `व` /va/, `ह` /ha/).
///     
///     This ensures complex words like `विद्वत्` remain unsegmented, resolving a major flaw found in naive segmenters.
///     
///     ```text
///     Hindi:  "विद्वत्"  → "विद्वत्"           (conjunct preserved, no ZWSP)
///     Hindi:  "पत्नी"    → "पत्\u{200B}नी"     (non-conjunct, ZWSP inserted)
///     Tamil:  "பற்றி"    → "பற்\u{200B}றி"     (no Hindi exception applies)
///     ```
///
/// 3.  **Script Transitions:** A standard space (U+0020) is inserted at script transitions (Indic ↔ Western).
///
///     ```text
///     "Helloपत्नी" → "Hello पत्\u{200B}नी"  (space at transition + ZWSP at virama)
///     ```
///
/// This approach prioritizes **performance** and **Devanagari linguistic accuracy**, treating the generic
/// **virama** break as correct for all other supported Indic scripts.
///
/// # Performance Characteristics
///
/// | Scenario                            | Path                    | Allocation | Notes |
/// |-------------------------------------|-------------------------|------------|-------|
/// | Western-only text                   | Stage skipped           | None       | Fully elided via `needs_apply` |
/// | No boundaries needed                | Stage skipped           | None       | Zero-copy pass-through |
/// | Segmentation required               | Fused `CharMapper`      | One        | Single `String` allocation at `collect()` |
/// | Fallback path                       | `segment_allocating`    | One        | Used when `CharMapper` unavailable |
///
/// The iterator uses a 3-state buffer (`prev_char`, `prev_class`, `pending_space`) with minimal overhead.
/// All boundary detection is `#[inline(always)]` for maximum compiler optimization.
///
/// # Examples
///
/// ## Chinese (Unigram Mode)
/// ```text
/// "你好世界"              → "你 好 世 界"
/// "Hello世界"            → "Hello 世 界"
/// "AI+区块链=未来"       → "AI+ 区 块 链 = 未 来"
/// ```
///
/// ## Japanese (Script Transitions Only)
/// ```text
/// "こんにちは世界"        → "こんにちは世界"        (no space within Japanese)
/// "Rustは最高"           → "Rust は最高"           (space at Latin→Japanese only)
/// "東京2025年"           → "東京 2025 年"          (spaces at transitions)
/// ```
///
/// ## Korean (Script Transitions Only)
/// ```text
/// "안녕하세요세계"        → "안녕하세요세계"        (no space within Korean)
/// "Hello안녕하세요"      → "Hello 안녕하세요"      (space at transition)
/// ```
///
/// ## Thai/Lao/Khmer/Myanmar (Script Transitions Only)
/// ```text
/// "Helloสวัสดี"          → "Hello สวัสดี"          (space at transition)
/// "สวัสดีชาวโลก"         → "สวัสดีชาวโลก"         (no syllable breaking without dictionary)
/// ```
///
/// ## Indic Scripts (ZWSP + Conjunct Heuristic)
/// ```text
/// Hindi:   "पत्नी"       → "पत्\u{200B}नी"        (ZWSP after virama)
/// Hindi:   "विद्वत्"     → "विद्वत्"              (conjunct preserved)
/// Tamil:   "பற்றி"       → "பற்\u{200B}றி"        (ZWSP after virama, no Hindi exception)
/// ```
///
/// ## Edge Cases
/// ```text
/// "  你好  世界  "        → "  你 好  世 界  "     (whitespace preserved)
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
        if let Some(mapper) = self.as_char_mapper(ctx) {
            // Collect into a String and compare with original
            let mut out = String::with_capacity(text.len());
            let mut changed = false;

            let mut original_chars = text.chars();
            for segmented_char in mapper.bind(&text, ctx) {
                out.push(segmented_char);

                // Check if this char matches the original sequence
                // If the iterator inserted a space/ZWSP, original_chars won't match
                if let Some(orig) = original_chars.next() {
                    if orig != segmented_char {
                        changed = true;
                    }
                } else {
                    // Iterator produced more chars than original (impossible for SegmentWords)
                    changed = true;
                }
            }

            // Check if there are leftover chars in original (iterator consumed fewer)
            if original_chars.next().is_some() {
                changed = true;
            }

            return if changed {
                Ok(Cow::Owned(out))
            } else {
                Ok(text)
            };
        }

        // Fallback path - always allocates but track changes
        let segmented = segment_allocating(&text, ctx.lang_entry);
        if segmented == text.as_ref() {
            Ok(text)
        } else {
            Ok(Cow::Owned(segmented))
        }
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
    if prev_class == Cjk && curr_class == Cjk && lang.unigram_cjk {
        return true;
    }

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
        if curr == ' ' || curr == zwsp() {
            prev_class = None;
            prev_char = None;
            continue;
        }

        let curr_class = classify(curr);

        // Virama + consonant → needs ZWSP
        if let Some(p_char) = prev_char
            && prev_class == Some(Indic)
            && is_virama(p_char)
            && curr_class == Indic
            && !(lang.code == HIN.code && should_prevent_indic_break(curr))
        {
            return true;
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
    // The Seg struct remains unchanged
    struct Seg<I: Iterator> {
        lang: LangEntry,
        inner: I, // CHANGED: Removed Peekable
        prev_char: Option<char>,
        prev_class: Option<CharClass>,
        prev_is_virama: bool, // CHANGED: Added cache
        pending_space: Option<char>,
    }

    // The Iterator implementation for Seg
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
                        if let Some(space) = self.pending_space.take() {
                            return Some(space);
                        }
                        return self.prev_char.take();
                    }
                };

                if is_any_whitespace(curr) || curr == zwsp() {
                    if let Some(prev) = self.prev_char.take() {
                        self.prev_class = None;
                        self.prev_is_virama = false;
                        self.pending_space = Some(curr); // ← preserve ZWSP!
                        return Some(prev);
                    }
                    return Some(curr); // ← don't skip ZWSP
                }

                // Ignore ZWJ/ZWNJ (transparent joiners)
                if curr == '\u{200D}' || curr == '\u{200C}' {
                    if self.prev_char.is_some() {
                        continue;
                    }
                    return Some(curr);
                }

                let curr_class = classify(curr);
                let curr_is_virama = is_virama(curr); // Computed once

                let (mut need_boundary, mut use_zwsp) = (false, false);

                if let (Some(prev), Some(prev_class)) = (self.prev_char, self.prev_class) {
                    // Golden Indic Rule: Insert ZWSP after virama + consonant
                    if prev_class == Indic
                    && self.prev_is_virama  // CHANGED: Use cached value
                    && !curr_is_virama
                    && curr_class == Indic
                    {
                        if !(self.lang.code == HIN.code && should_prevent_indic_break(curr)) {
                            need_boundary = true;
                            use_zwsp = true;
                        }
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
                    self.prev_is_virama = curr_is_virama; // CHANGED: Cache for next iteration

                    return Some(prev);
                }

                // First non-whitespace character — buffer it
                self.prev_char = Some(curr);
                self.prev_class = Some(curr_class);
                self.prev_is_virama = curr_is_virama; // CHANGED: Initialize cache
            }
        }
    }

    Seg {
        lang,
        inner: chars, // CHANGED: Removed .peekable()
        prev_char: None,
        prev_class: None,
        prev_is_virama: false, // CHANGED: Initialize
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

impl StageTestConfig for SegmentWords {
    fn one_to_one_languages() -> &'static [Lang] {
        &[] // Inserts spaces/ZWSP
    }

    fn samples(lang: Lang) -> &'static [&'static str] {
        match lang {
            ENG => &["Hello world", "123 !@#", "", " "],
            ZHO => &["你好世界", "Hello世界", "AI+区块链"],
            JPN => &["こんにちは世界", "Rustは最高", "東京2025年"],
            HIN => &["पत्नी", "विद्वत्", "Helloपत्नी"],
            _ => &["Hello World 123", " déjà-vu ", "TEST", ""],
        }
    }

    fn should_pass_through(lang: Lang) -> &'static [&'static str] {
        match lang {
            ENG | FRA | SPA | DEU => &["hello world", "test 123", ""], // Western text
            ZHO | JPN | KOR | HIN => &[], // These always need segmentation
            _ => &["hello world", ""],
        }
    }

    fn should_transform(lang: Lang) -> &'static [(&'static str, &'static str)] {
        match lang {
            ZHO => &[
                ("你好", "你 好"), // Unigram
                ("世界", "世 界"),
                ("Hello世界", "Hello 世 界"),
            ],
            JPN => &[
                ("Hello世界", "Hello 世界"), // Script transition
                ("東京2025年", "東京 2025 年"),
            ],
            HIN => &[
                ("पत्नी", "पत्\u{200B}नी"), // ZWSP after virama
                ("Helloपत्नी", "Hello पत्\u{200B}नी"),
            ],
            _ => &[],
        }
    }

    fn skip_needs_apply_test() -> bool {
        true
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Universal contract tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;
    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(SegmentWords);
    }
}
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
                ("こんにちは世界", "こんにちは世界"), // ← FIX: No space expected
                ("Rustは最高", "Rust は最高"),
                ("人工知能", "人工知能"),
                ("私は学生です", "私は学生です"), // ← FIX: No space expected
            ],
        );
    }

    #[test]
    fn test_chinese() {
        run_cases(
            ZHO,
            &[
                ("Hello世界", "Hello 世 界"),
                ("世界Hello", "世 界 Hello"),
                ("你好世界", "你 好 世 界"),
                ("", ""),
                ("A", "A"),
                ("中", "中"),
                ("Hello你好World世界", "Hello 你 好 World 世 界"),
                ("中华人民共和国", "中 华 人 民 共 和 国"),
                ("人工智能是未来", "人 工 智 能 是 未 来"),
                ("我爱你", "我 爱 你"),
                ("今天天气很好", "今 天 天 气 很 好"),
                // Mixed Latin + CJK — only break inside CJK blocks
                ("Rust编程语言", "Rust 编 程 语 言"),
                ("2025年北京奥运", "2025 年 北 京 奥 运"),
                // CJK punctuation should NOT trigger extra breaks (treated as Other)
                ("你好，世界！", "你 好 ， 世 界 ！"),
                ("「你好」他说道", "「 你 好 」 他 说 道"),
                // Edge cases
                ("", ""),
                ("  ", "  "),       // whitespace preserved
                ("中", "中"),       // single ideograph → no space
                ("Hello", "Hello"), // pure Western → zero-copy
                ("中中中", "中 中 中"),
                ("  你好  世界  ", "  你 好  世 界  "), // whitespace preserved
                ("AI+区块链=未来", "AI+ 区 块 链 = 未 来"),
                ("2025年，你好！", "2025 年 ， 你 好 ！"),
                ("Rust×中文＝强大", "Rust× 中 文 ＝ 强 大"),
                ("「人工智能」2025", "「 人 工 智 能 」 2025"),
                ("Hello,世界!", "Hello, 世 界 !"),
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
                ("안녕하세요세계", "안녕하세요세계"),
                ("서울2025년", "서울 2025 년"),
                ("인공지능", "인공지능"),
                ("저는학생입니다", "저는학생입니다"),
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
                ("विद्वत्", "विद्वत्"),             // no break,
                ("विद्वत्त्व", "विद्वत्\u{200B}त्व"), // non-final virama followed by consonant → break
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
