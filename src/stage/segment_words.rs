use std::{borrow::Cow, iter::FusedIterator};

use crate::{
    DEU, ENG, FRA, HIN, JPN, KOR, SPA, ZHO,
    context::Context,
    lang::{Lang, LangEntry, SegmentRule},
    stage::{FusableStage, Stage, StageError, StaticFusableStage},
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
        "segment_words"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        let entry = ctx.lang_entry;

        // 1. Language doesn't want any segmentation at all → skip forever
        if !entry.needs_segmentation() {
            return Ok(false);
        }

        // 2. No rules defined → nothing to do
        if entry.segment_rules().is_empty() {
            return Ok(false);
        }

        // 3. Pure ASCII and language doesn't break Western→Script → zero-cost skip
        if text.is_ascii() {
            let rules = entry.segment_rules();
            if !rules.contains(&SegmentRule::WesternToScript)
                && !rules.contains(&SegmentRule::ScriptToWestern)
            {
                return Ok(false);
            }
        }

        // 4. Actually scan — this is the only place that knows the truth
        Ok(needs_segmentation(text, &entry))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let entry = ctx.lang_entry;
        let mut out = String::with_capacity(self.expected_capacity(text.len()));

        let mut prev_char: Option<char> = None;
        let mut prev_class: Option<CharClass> = None;
        let mut prev_is_virama = false;

        for curr in text.chars() {
            // 1. Whitespace/ZWSP resets state and passes through
            if is_any_whitespace(curr) || curr == zwsp() {
                out.push(curr);
                prev_char = None;
                prev_class = None;
                prev_is_virama = false;
                continue;
            }

            // 2. Joiners pass through but don't reset state
            if curr == '\u{200D}' || curr == '\u{200C}' {
                out.push(curr);
                continue;
            }

            let curr_class = classify(curr);
            let curr_is_virama = is_virama(curr);

            // 3. Boundary Check Logic
            if let (Some(_p_char), Some(p_class)) = (prev_char, prev_class) {
                let mut need_boundary = false;
                let mut use_zwsp = false;

                // Indic Rule
                if p_class == Indic && prev_is_virama && !curr_is_virama && curr_class == Indic {
                    if !(entry.code() == HIN.code && should_prevent_indic_break(curr)) {
                        need_boundary = true;
                        use_zwsp = true;
                    }
                }
                // Script Boundary Rule
                else if check_boundary_with_classes(p_class, curr_class, &entry) {
                    need_boundary = true;
                }

                if need_boundary {
                    out.push(if use_zwsp { zwsp() } else { ' ' });
                }
            }

            // 4. Update state and push current
            out.push(curr);
            prev_char = Some(curr);
            prev_class = Some(curr_class);
            prev_is_virama = curr_is_virama;
        }

        Ok(Cow::Owned(out))
    }

    /// SegmentWords is always fusable - checking needs_apply on the original text
    /// is always a safe approximation since it only inserts characters (spaces/ZWSP).
    #[inline]
    fn safe_skip_approximation(&self) -> bool {
        true
    }

    /// SegmentWords is always fusable since it only inserts delimiters (spaces or ZWSP)
    /// between characters. No character transformations or multi-character expansions.
    #[inline]
    fn as_fusable(&self) -> Option<&dyn FusableStage> {
        Some(self)
    }

    #[inline]
    fn expected_capacity(&self, input_len: usize) -> usize {
        // Heuristic: segmentation adds roughly 10-20% length for mixed scripts,
        // but up to 100% for CJK unigrams.
        (input_len * 12) >> 3 // ~1.5x to be safe
    }
}

impl StaticFusableStage for SegmentWords {
    type Adapter<'a, I>
        = SegmentWordsAdapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    #[inline(always)]
    fn supports_static_fusion(&self) -> bool {
        true
    }

    #[inline(always)]
    fn static_fused_adapter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::Adapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        SegmentWordsAdapter {
            input,
            lang: &ctx.lang_entry,
            prev_char: None,
            prev_class: None,
            prev_is_virama: false,
            pending_space: None,
        }
    }
}

pub struct SegmentWordsAdapter<'a, I> {
    input: I,
    lang: &'a LangEntry,
    prev_char: Option<char>,
    prev_class: Option<CharClass>,
    prev_is_virama: bool,
    pending_space: Option<char>,
}

impl<'a, I: Iterator<Item = char>> Iterator for SegmentWordsAdapter<'a, I> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        // 1. First priority: emit any boundary character we've decided to inject
        if let Some(space) = self.pending_space.take() {
            return Some(space);
        }

        loop {
            let curr = match self.input.next() {
                Some(c) => c,
                None => {
                    // End of stream: return the last buffered character if it exists
                    return self.prev_char.take();
                }
            };

            // 2. Whitespace/ZWSP resets boundary detection state
            if is_any_whitespace(curr) || curr == zwsp() {
                if let Some(prev) = self.prev_char.take() {
                    self.prev_class = None;
                    self.prev_is_virama = false;
                    self.pending_space = Some(curr);
                    return Some(prev);
                }
                return Some(curr);
            }

            // 3. Joiners are transparent: emit but don't reset script state
            if curr == '\u{200D}' || curr == '\u{200C}' {
                if self.prev_char.is_some() {
                    continue;
                }
                return Some(curr);
            }

            let curr_class = classify(curr);
            let curr_is_virama = is_virama(curr);

            // 4. Check for boundaries if we have a previous character
            if let (Some(prev), Some(p_class)) = (self.prev_char, self.prev_class) {
                let mut need_boundary = false;
                let mut use_zwsp = false;

                // Indic Rule
                if p_class == Indic && self.prev_is_virama && !curr_is_virama && curr_class == Indic
                {
                    if !(self.lang.code() == HIN.code && should_prevent_indic_break(curr)) {
                        need_boundary = true;
                        use_zwsp = true;
                    }
                }
                // Script Boundary Rule
                else if check_boundary_with_classes(p_class, curr_class, self.lang) {
                    need_boundary = true;
                }

                if need_boundary {
                    self.pending_space = Some(if use_zwsp { zwsp() } else { ' ' });
                }

                // Buffer current and return previous
                self.prev_char = Some(curr);
                self.prev_class = Some(curr_class);
                self.prev_is_virama = curr_is_virama;
                return Some(prev);
            }

            // 5. Initial character buffering
            self.prev_char = Some(curr);
            self.prev_class = Some(curr_class);
            self.prev_is_virama = curr_is_virama;
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.input.size_hint();
        // This stage can double the size in the worst case (Cjk Unigrams)
        (lower, upper.map(|u| u.saturating_mul(2)))
    }
}

impl<'a, I: FusedIterator<Item = char>> FusedIterator for SegmentWordsAdapter<'a, I> {}

// ============================================================================
// FusableStage Implementation - Dynamic Iterator Fusion
// ============================================================================

impl FusableStage for SegmentWords {
    fn dyn_fused_adapter<'a>(
        &self,
        input: Box<dyn FusedIterator<Item = char> + 'a>,
        ctx: &'a Context,
    ) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(SegmentWordsAdapter {
            input,
            lang: &ctx.lang_entry,
            prev_char: None,
            prev_class: None,
            prev_is_virama: false,
            pending_space: None,
        })
    }
}

#[inline(always)]
fn check_boundary_with_classes(
    prev_class: CharClass,
    curr_class: CharClass,
    lang: &LangEntry,
) -> bool {
    // CJK unigram mode: break between every CJK character
    if prev_class == Cjk && curr_class == Cjk && lang.needs_unigram_cjk() {
        return true;
    }

    // No boundary if same class
    if prev_class == curr_class {
        return false;
    }

    // Check segment rules for script transitions
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
pub fn needs_segmentation(text: &str, lang: &LangEntry) -> bool {
    let mut prev_class: Option<CharClass> = None;
    let mut prev_char: Option<char> = None;

    for curr in text.chars() {
        // Whitespace/ZWSP resets boundary detection
        if curr == ' ' || curr == zwsp() {
            prev_class = None;
            prev_char = None;
            continue;
        }

        let curr_class = classify(curr);

        // Virama + consonant → needs ZWSP (Indic scripts)
        if let Some(p_char) = prev_char
            && prev_class == Some(Indic)
            && is_virama(p_char)
            && curr_class == Indic
            && !(lang.code() == HIN.code && should_prevent_indic_break(curr))
        {
            return true;
        }

        // Check for script transition boundaries
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
    fn test_english_no_segmentation() {
        let stage = SegmentWords;
        let ctx = Context::new(ENG);

        // English doesn't need segmentation
        assert!(!stage.needs_apply("Hello world", &ctx).unwrap());

        let result = stage.apply(Cow::Borrowed("Hello world"), &ctx).unwrap();
        assert_eq!(result, "Hello world");
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
                ("こんにちは世界", "こんにちは世界"),
                ("Rustは最高", "Rust は最高"),
                ("人工知能", "人工知能"),
                ("私は学生です", "私は学生です"),
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
                ("Rust编程语言", "Rust 编 程 语 言"),
                ("2025年北京奥运", "2025 年 北 京 奥 运"),
                ("你好，世界！", "你 好 ， 世 界 ！"),
                ("「你好」他说道", "「 你 好 」 他 说 道"),
                ("中中中", "中 中 中"),
                ("  你好  世界  ", "  你 好  世 界  "),
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

    #[test]
    fn test_ascii_fast_path() {
        let stage = SegmentWords;

        // English ASCII text
        let ctx = Context::new(ENG);
        let input = "hello world 123";
        assert!(!stage.needs_apply(input, &ctx).unwrap());
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert_eq!(result.to_string(), "hello world 123");

        // Chinese with ASCII (should segment)
        let ctx = Context::new(ZHO);
        let input = "Hello";
        // Pure ASCII but language segments Western→Script
        assert!(!stage.needs_apply(input, &ctx).unwrap()); // No actual script transition
    }
}
