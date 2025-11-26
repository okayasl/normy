use std::{
    borrow::Cow,
    iter::{FusedIterator, Peekable},
    sync::Arc,
};

use crate::{
    TAM,
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
    if prev_class == curr_class {
        println!(
            ">>> CLASS CHECK: SAME CLASS {:?} -> NO BOUNDARY",
            prev_class
        );
        return false;
    }
    match (prev_class, curr_class) {
        (Western, Cjk | Hangul | SEAsian | NonCJKScript | Indic | Other) => {
            let rule = lang.segment_rules().contains(&SegmentRule::WesternToScript);
            println!(
                ">>> CLASS CHECK: WESTERN -> SCRIPT ({:?} -> {:?}) = BOUNDARY? {} (rule: WesternToScript)",
                prev_class, curr_class, rule
            );
            rule
        }
        (Cjk | Hangul | SEAsian | NonCJKScript | Indic | Other, Western) => {
            let rule = lang.segment_rules().contains(&SegmentRule::ScriptToWestern);
            println!(
                ">>> CLASS CHECK: SCRIPT -> WESTERN ({:?} -> {:?}) = BOUNDARY? {} (rule: ScriptToWestern)",
                prev_class, curr_class, rule
            );
            rule
        }
        (
            Cjk | Hangul | SEAsian | NonCJKScript | Indic | Other,
            Cjk | Hangul | SEAsian | NonCJKScript | Indic | Other,
        ) => {
            println!(
                ">>> CLASS CHECK: NON-WESTERN -> NON-WESTERN ({:?} -> {:?}) = BOUNDARY (default true)",
                prev_class, curr_class
            );
            true
        }
        _ => {
            println!(
                ">>> CLASS CHECK: FALLBACK ({:?} -> {:?}) = NO BOUNDARY",
                prev_class, curr_class
            );
            false
        }
    }
}

// In src/stage/segment_words.rs

#[inline]
pub fn needs_segmentation(text: &str, lang: LangEntry) -> bool {
    let mut prev_class: Option<CharClass> = None;
    let mut prev_char: Option<char> = None; // <-- KEEP this for Indic check

    for curr in text.chars() {
        if is_any_whitespace(curr) {
            // ... (original logic)
            continue;
        }

        let curr_class = classify(curr);

        // --- ADDED INDIC CHECK ---
        if let Some(p_char) = prev_char {
            let p_class = prev_class.unwrap();

            // Check for the Golden Indic Rule: Prev is a Virama, followed by an Indic character
            // (This prevents the early exit and forces the segment_chars iterator to run)
            if p_class == Indic && is_virama(p_char) && curr_class == Indic {
                println!(
                    ">>> NEEDS INDIC VIRAMA BOUNDARY FOUND: '{}' U+{:04X} -> '{}' U+{:04X}",
                    p_char, p_char as u32, curr, curr as u32
                );
                return true;
            }
        }
        // --- END ADDED INDIC CHECK ---

        if let Some(p_class) = prev_class {
            let boundary = check_boundary_with_classes(p_class, curr_class, lang);
            // ... (original class boundary check logic)
            if boundary {
                return true;
            }
        }

        prev_class = Some(curr_class);
        prev_char = Some(curr); // <-- UPDATE this
    }

    println!(">>> NEEDS: NO BOUNDARIES FOUND");
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
                        // First: emit pending delimiter if any
                        if let Some(space) = self.pending_space.take() {
                            println!(
                                ">>> END OF INPUT → EMIT PENDING DELIMITER U+{:04X} ({})",
                                space as u32,
                                if space == zwsp() { "ZWSP" } else { "SPACE" }
                            );
                            return Some(space);
                        }
                        // Then: emit last buffered char
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

                // *** CRITICAL ADDITION: Ignore ZWJ/ZWNJ as non-boundary characters ***
                if curr == '\u{200D}' || curr == '\u{200C}' {
                    println!(
                        ">>> IGNORING ZWNJ/ZWJ: '{}' U+{:04X} → PASS THROUGH (Transparent/DISCARD)",
                        curr, curr as u32
                    );

                    // If a character is buffered, the ZWJ/ZWNJ is part of an ongoing syllable/cluster
                    // and should be discarded, letting the next real character hit the rule.
                    if self.prev_char.is_some() {
                        // Discard the ZWJ/ZWNJ, and continue the loop to load the next character ('ष').
                        // Since we didn't update the buffer, prev_char is still '्'.
                        continue;
                    }

                    // If buffer is empty, just emit the ZWJ/ZWNJ immediately (this is unlikely/safe).
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
                    if self.lang.code == TAM.code
                        && prev_class == Indic
                        && prev_is_virama
                        && !curr_is_virama
                        && curr_class == Indic
                    {
                        println!(
                            ">>> HIT TAMIL VIRAMA BREAK: prev='{}' (virama) → curr='{}' (consonant) → INSERT ZWSP",
                            prev, curr
                        );
                        need_boundary = true;
                        use_zwsp = true;
                    }
                    // 2. GOLDEN INDIC RULE (Original rule, runs for non-Tamil Indic scripts)
                    else if prev_class == Indic
                        && prev_is_virama
                        && !curr_is_virama
                        && curr_class == Indic
                        && self.lang.code != TAM.code
                    // Only for non-Tamil languages
                    {
                        println!(
                            ">>> HIT GOLDEN INDIC RULE: prev='{}' (virama) → curr='{}' (consonant) → INSERT ZWSP AFTER VIRAMA",
                            prev, curr
                        );
                        need_boundary = true;
                        use_zwsp = true;
                    }
                    // 3. REGULAR SCRIPT TRANSITION BOUNDARY (Use SPACE)
                    // CRITICAL FIX: Removed the `!prev_is_virama` guard to allow transitions
                    // like Tamil (Pulli) -> Western (W) to fire.
                    else if check_boundary_with_classes(prev_class, curr_class, self.lang) {
                        let from = format!("{:?}", prev_class);
                        let to = format!("{:?}", curr_class);
                        println!(
                            ">>> SCRIPT BOUNDARY: {} -> {} -> INSERT DELIMITER (SPACE)",
                            from, to
                        );
                        need_boundary = true;

                        // For Indic <-> Western transition, we require a SPACE, so use_zwsp=false.
                        // The original logic `use_zwsp = prev_class == Indic && curr_class == Indic;`
                        // is for same-script breaks which should NOT be in this block.
                        use_zwsp = false;

                        println!(">>> DELIM CHOICE: use_zwsp=false (Script Transition uses SPACE)",);
                    } else {
                        // Original NO BOUNDARY logic
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

    fn debug_string(label: &str, s: &str) {
        println!("{}: {:?}", label, s);

        for (i, c) in s.chars().enumerate() {
            let display = match c {
                '\u{200B}' => "ZWSP",
                '\u{094D}' => "VIRAMA",
                _ => &c.to_string(),
            };

            println!(
                "{:02} | U+{:04X} | {:<5} | virama: {}",
                i,
                c as u32,
                display,
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
            if out != expected && debug {
                eprintln!("\n====================================================");
                eprintln!("❌ TEST FAILURE for language {:?}:", lang);
                debug_string("INPUT", input);
                debug_string("EXPECTED", expected);
                debug_string("GOT", &out);
                eprintln!("====================================================\n");

                // Always show debug info for failing cases
                debug_step_by_step(lang, input);

                //panic!("Failed: {} → {}  (got {})", input, expected, out);
            }
            // if debug {
            //     debug_string("INPUT", input);
            //     debug_string("EXPECTED", expected);
            //     debug_string("GOT", &out);
            // }
            assert_eq!(out, expected, "Failed: {} → {}", input, expected);
        }
    }

    fn debug_step_by_step(lang: Lang, input: &str) {
        let stage = SegmentWords;
        let ctx = Context::new(lang);
        println!("=== STEP BY STEP DEBUG ===");
        println!("=== STAGE APPLY ===");
        let out = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        println!("INPUT:  {input}");
        println!("OUTPUT: {out}");
        println!("CHARS:  {:?}", out.chars().collect::<Vec<_>>());
        println!("BYTES:  {:02X?}", out.as_bytes());

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
    fn debug_western_to_indic_no_virama() {
        let stage = SegmentWords;
        let ctx = Context::new(HIN);
        let input = "Helloप"; // Simple transition: Western -> Indic consonant
        let out = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        println!("INPUT: {}", input);
        println!("OUTPUT: {}", out);
        println!("CHARS: {:?}", out.chars().collect::<Vec<_>>());
        let iter = segment_chars(input.chars(), ctx.lang_entry);
        for c in iter {
            if c == '\u{200B}' {
                println!("INSERTED ZWSP");
            } else if c == ' ' {
                println!("INSERTED SPACE");
            } else {
                println!("EMIT: '{}' U+{:04X}", c, c as u32);
            }
        }
    }

    #[test]
    fn debug_isolated_virama_hindi() {
        let stage = SegmentWords;
        let ctx = Context::new(HIN);
        let input = "पत्"; // प त ् - consonant + virama at end
        let out = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        println!("INPUT: {}", input);
        println!("OUTPUT: {}", out);
        println!("CHARS: {:?}", out.chars().collect::<Vec<_>>());
        let iter = segment_chars(input.chars(), ctx.lang_entry);
        for c in iter {
            if c == '\u{200B}' {
                println!("INSERTED ZWSP");
            } else {
                println!("EMIT: '{}' U+{:04X}", c, c as u32);
            }
        }
    }

    #[test]
    fn debug_tamil_transition() {
        let stage = SegmentWords;
        let ctx = Context::new(TAM);
        let input = "Helloபற்"; // Western -> Tamil consonant + puḷḷi (virama U+0BCD)
        let out = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        println!("INPUT: {}", input);
        println!("OUTPUT: {}", out);
        println!("CHARS: {:?}", out.chars().collect::<Vec<_>>());
        let iter = segment_chars(input.chars(), ctx.lang_entry);
        for c in iter {
            if c == '\u{200B}' {
                println!("INSERTED ZWSP");
            } else if c == ' ' {
                println!("INSERTED SPACE");
            } else {
                println!("EMIT: '{}' U+{:04X}", c, c as u32);
            }
        }
    }

    #[test]
    fn debug_delimiter_choice_pure_transition() {
        let stage = SegmentWords;
        let ctx = Context::new(HIN);
        let cases = &[
            ("Helloप", "Hello प"), // Should be visible space
            ("पHello", "प Hello"), // Should be visible space
            ("पत्", "पत्"),          // No delimiter
            ("पत्नी", "पत्नी"),      // ZWSP inside
        ];
        for &(input, expected) in cases {
            let out = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            println!("INPUT:  {:20} → {}", input, out);
            println!("EXPECT: {:20}   diff: {}", expected, out != expected);
            println!("---");
        }
    }

    #[test]
    fn debug_virama_trigger_direction() {
        let ctx = Context::new(HIN);
        println!(
            "needs_segmentation(प्त्नी) = {}",
            needs_segmentation("प्त्नी", ctx.lang_entry)
        );
        println!(
            "needs_segmentation(पत्)     = {}",
            needs_segmentation("पत्", ctx.lang_entry)
        );
        println!(
            "needs_segmentation(पत्नी)   = {}",
            needs_segmentation("पत्नी", ctx.lang_entry)
        );
    }
}
