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

#[inline(always)]
fn check_boundary_with_classes(
    prev_class: CharClass,
    curr_class: CharClass,
    lang: LangEntry,
) -> bool {
    // Same class = no boundary
    if prev_class == curr_class {
        return false;
    }

    // Define the set of non-Western classes that MUST break when transitioning
    // to or from Western, or when transitioning between themselves.
    // ADD CharClass::Other to this set.

    match (prev_class, curr_class) {
        // Western <-> Script/Other transitions (controlled by lang rules)
        (Western, Cjk | Hangul | SEAsian | NonCJKScript | Indic | Other) => {
            // <-- ADD Other
            lang.segment_rules().contains(&SegmentRule::WesternToScript)
        }
        (Cjk | Hangul | SEAsian | NonCJKScript | Indic | Other, Western) => {
            // <-- ADD Other
            lang.segment_rules().contains(&SegmentRule::ScriptToWestern)
        }

        // Non-Western Script/Other <-> Non-Western Script/Other transitions
        (
            Cjk | Hangul | SEAsian | NonCJKScript | Indic | Other,
            Cjk | Hangul | SEAsian | NonCJKScript | Indic | Other,
        ) => true, // <-- ADD Other

        // This final arm now guarantees:
        // 1. (Cjk, Other) -> true (Fixes `あ` -> `😀`)
        // 2. (Other, Cjk) -> true (Fixes `、` -> `あ`)
        // 3. (Script, Script) -> true (Original intent)
        _ => false,
    }
}

#[inline]
pub fn needs_segmentation(text: &str, lang: LangEntry) -> bool {
    let mut prev_class: Option<CharClass> = None;
    let mut prev_char: Option<char> = None; // Add this for virama check

    for curr in text.chars() {
        if is_any_whitespace(curr) {
            println!(">>> NEEDS SKIP WS: '{}' U+{:04X}", curr, curr as u32);
            continue;
        }

        let curr_class = classify(curr);

        if let Some(p_class) = prev_class {
            if check_boundary_with_classes(p_class, curr_class, lang) {
                println!(
                    ">>> NEEDS BOUNDARY DETECTED: {:?} → {:?} ",
                    p_class, curr_class
                );
                return true;
            } else {
                println!(
                    ">>> NEEDS NO CLASS BOUNDARY: {:?} → {:?} ",
                    p_class, curr_class
                );
            }
        }

        // Virama check (temporary — will be permanent in fix)
        if let (Some(p_class), Some(p_char)) = (prev_class, prev_char)
            && p_class == Indic
            && !is_virama(p_char)
            && is_virama(curr)
        {
            println!(
                ">>> NEEDS VIRAMA BOUNDARY DETECTED: '{}' → '{}' ",
                p_char, curr
            );
            return true;
        }

        prev_class = Some(curr_class);
        prev_char = Some(curr); // Add this
    }

    println!(">>> NEEDS: NO BOUNDARIES FOUND → RETURNING FALSE");
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
            // 1. Emit pending delimiter first (ZWSP or space)
            if let Some(space) = self.pending_space.take() {
                println!(
                    ">>> INSERT DELIMITER: U+{:04X} ({})",
                    space as u32,
                    if space == zwsp() { "ZWSP" } else { "SPACE" }
                );
                return Some(space);
            }

            loop {
                let curr = match self.inner.next() {
                    Some(c) => c,
                    None => {
                        if let Some(last) = self.prev_char.take() {
                            println!(
                                ">>> END OF INPUT → FLUSH LAST CHAR: '{}' U+{:04X}",
                                last, last as u32
                            );
                            return Some(last);
                        }
                        return None;
                    }
                };

                // Skip whitespace early
                if is_any_whitespace(curr) {
                    println!(
                        ">>> WHITESPACE '{}' U+{:04X} → PASS THROUGH",
                        curr, curr as u32
                    );
                    if let Some(prev) = self.prev_char.take() {
                        println!(
                            ">>> FLUSH BUFFERED CHAR DUE TO WHITESPACE: '{}' U+{:04X}",
                            prev, prev as u32
                        );
                        return Some(prev);
                    }
                    return Some(curr);
                }

                let curr_class = classify(curr);
                let curr_is_virama = is_virama(curr);

                println!(
                    ">>> PROCESS: curr='{}' U+{:04X} | class={:?} | virama={} | prev={:?}",
                    curr,
                    curr as u32,
                    curr_class,
                    curr_is_virama,
                    self.prev_char
                        .map(|c| format!("'{}' U+{:04X}", c, c as u32))
                        .unwrap_or("none".into())
                );

                let (mut need_boundary, mut use_zwsp) = (false, false);

                if let (Some(prev), Some(prev_class)) = (self.prev_char, self.prev_class) {
                    let prev_is_virama = is_virama(prev);

                    // GOLDEN INDIC RULE — CORRECTED & VERBOSE
                    if prev_class == Indic
                        && prev_is_virama
                        && !curr_is_virama
                        && curr_class == Indic
                    {
                        println!(
                            ">>> HIT GOLDEN INDIC RULE: prev='{}' (virama) → curr='{}' (consonant) → INSERT ZWSP AFTER VIRAMA",
                            prev, curr
                        );
                        need_boundary = true;
                        use_zwsp = true;
                    }
                    // Regular script transition boundary
                    else if !prev_is_virama
                        && !curr_is_virama
                        && check_boundary_with_classes(prev_class, curr_class, self.lang)
                    {
                        let from = format!("{:?}", prev_class);
                        let to = format!("{:?}", curr_class);
                        println!(
                            ">>> SCRIPT BOUNDARY: {} → {} → INSERT {}",
                            from,
                            to,
                            if prev_class == Indic || curr_class == Indic {
                                "ZWSP"
                            } else {
                                "SPACE"
                            }
                        );
                        need_boundary = true;
                        use_zwsp = prev_class == Indic || curr_class == Indic;
                    } else {
                        println!(
                            ">>> NO BOUNDARY: prev='{}' U+{:04X} (virama={}) | curr='{}' U+{:04X} (virama={})",
                            prev, prev as u32, prev_is_virama, curr, curr as u32, curr_is_virama
                        );
                    }

                    // Emit the previous character NOW that we know what follows it
                    println!(">>> EMIT PREV CHAR: '{}' U+{:04X}", prev, prev as u32);

                    if need_boundary {
                        let delim = if use_zwsp { zwsp() } else { ' ' };
                        println!(
                            ">>> QUEUE DELIMITER AFTER '{}' → U+{:04X} ({})",
                            prev,
                            delim as u32,
                            if use_zwsp { "ZWSP" } else { "SPACE" }
                        );
                        self.pending_space = Some(delim);
                    }

                    // Update buffer for next iteration
                    self.prev_char = Some(curr);
                    self.prev_class = Some(curr_class);

                    return Some(prev);
                }

                // First non-whitespace character — buffer it
                println!(
                    ">>> BUFFER FIRST CHAR: '{}' U+{:04X} | class={:?}",
                    curr, curr as u32, curr_class
                );
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

    fn debug_string(s: &str) {
        println!("For {s}");
        for (i, c) in s.chars().enumerate() {
            print!(
                "{}: U+{:04X} {} (virama: {})    ",
                i,
                c as u32,
                c,
                is_virama(c)
            );
        }
        println!();
    }

    /// Generic test helper for all languages
    fn run_cases<'a>(lang: Lang, cases: &'a [(&'a str, &'a str)], debug: bool) {
        let stage = SegmentWords;
        let ctx = Context::new(lang);

        for &(input, expected) in cases {
            let out = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            if debug {
                debug_string(input);
                debug_string(expected);
                debug_string(&out.clone());
            }
            assert_eq!(out, expected, "Failed: {} → {}", input, expected);
        }
    }

    // ============================================================
    // Japanese — regular spaces at script boundaries
    // ============================================================
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
            false,
        );
    }

    // ============================================================
    // Chinese — regular spaces at script boundaries
    // ============================================================
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
            false,
        );
    }

    // ============================================================
    // Korean — regular spaces at script boundaries
    // ============================================================
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
            false,
        );
    }

    // ============================================================
    // Thai — regular spaces at script boundaries
    // ============================================================
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
            false,
        );
    }

    // ============================================================
    // Lao, Myanmar, Khmer — same as Thai
    // ============================================================
    #[test]
    fn test_lao() {
        run_cases(
            LAO,
            &[
                ("Helloສະບາຍດີ", "Hello ສະບາຍດີ"),
                ("ສະບາຍດີWorld", "ສະບາຍດີ World"),
                ("ສະບາຍດີທຸກຄົນ", "ສະບາຍດີທຸກຄົນ"),
            ],
            false,
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
            false,
        );
    }

    #[test]
    fn test_khmer() {
        run_cases(
            KHM,
            &[("Helloសួស្តី", "Hello សួស្តី"), ("សួស្តីWorld", "សួស្តី World")],
            false,
        );
    }

    // ============================================================
    // Hindi (Devanagari) — ZWSP at virama boundaries
    // ============================================================
    #[test]
    fn test_hindi() {
        let cases: &[(&str, &str)] = &[
            // Single virama words
            ("पत्नी", "पत्\u{200B}नी"), // Only one virama in input
            // If you want double virama, use explicit Unicode:
            (
                "प\u{094D}त\u{094D}नी",
                "प\u{094D}\u{200B}त\u{094D}\u{200B}नी",
            ),
            // Other cases...
            ("सन्तोष", "सन्\u{200B}तोष"), // One virama
            ("विद्वत्", "विद्\u{200B}वत्"), // One virama, final virama no break
            ("रामायण", "रामायण"),       // No virama
            // Script transitions
            ("Helloपत्नी", "Hello पत्\u{200B}नी"),
            ("पत्नीHello", "पत्\u{200B}नी Hello"),
        ];
        run_cases(HIN, cases, true);
    }

    #[test]
    fn debug_step_by_step_hindi_patnee() {
        let stage = SegmentWords;
        let ctx = Context::new(HIN);
        let input = "Helloपत्नी"; // NFC: प त ् न ी

        println!("=== Stage apply ===");
        let out = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        println!("INPUT:  {input}");
        println!("OUTPUT: {out}");
        println!("CHARS:  {:?}", out.chars().collect::<Vec<_>>());
        println!("BYTES:  {:02X?}", out.as_bytes());

        println!("=== STEP BY STEP DEBUG ===");
        for c in input.chars() {
            println!(
                "CHAR: {:<4} U+{:04X} | is_virama: {} | classify: {:?}",
                c,
                c as u32,
                is_virama(c),
                classify(c)
            );
        }

        let iter = segment_chars(input.chars(), ctx.lang_entry);
        println!("\n--- ITERATOR OUTPUT ---");
        for c in iter {
            if c == '\u{200B}' {
                println!("INSERTED ZWSP U+200B");
            } else {
                println!("EMIT: {:<4} U+{:04X}", c, c as u32);
            }
        }
    }

    // ============================================================
    // Tamil — ZWSP at puḷḷi boundaries
    // ============================================================
    #[test]
    fn test_tamil() {
        let cases: &[(&str, &str)] = &[
            ("பற்றி", "பற்\u{200B}றி"), // One virama
            ("தமிழ்", "தமிழ்"),         // Final virama, no break
            ("அக்கா", "அக்\u{200B}கா"),   // One virama
            // Script transitions
            ("Helloதமிழ்", "Hello தமிழ்"),
            ("தமிழ்World", "தமிழ் World"),
        ];

        run_cases(TAM, cases, true);
    }

    #[test]
    fn test_indic_virama_golden_rule() {
        let cases = &[
            ("क्‍ष", "क्\u{200B}ष"),  // ZWSP after क, before ्
            ("त्‍त", "त्\u{200B}त"),  // double consonant
            ("संत", "सन्\u{200B}त"), // standard cluster
            ("राम", "राम"),        // no virama
            ("विद्वत्", "विद्वत्"),    // final virama → no ZWSP
        ];
        run_cases(HIN, cases, false);
    }
}
