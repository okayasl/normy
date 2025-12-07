/// Unicode character classification utilities for Normy.
///
/// # Design Philosophy
///
/// Normy is a **zero-allocation**, **zero-copy** normalization layer for production NLP pipelines.
/// It deliberately sacrifices full UCD compliance for extreme performance and predictability.
///
/// - Covers >95% of characters in real-world multilingual web text
/// - Historical, constructed, and regional scripts (Deseret, Gothic, Ethiopic, Tifinagh, etc.)
///   are classified as `Other` → no unnecessary word breaks → preserves zero-copy path
/// - Extensibility: add missing scripts via `Context::with_modified` or custom stages
///
/// If you need 100% UCD fidelity, compose with `unicode-segmentation` as a dynamic stage.
/// Normy is not a Unicode museum. It is a tokenizer accelerator.
use phf::{phf_map, phf_set};

// Format control characters (General Category = Cf) and selected
// zero-width characters relevant to text normalization.
//
// These are removed by `RemoveFormatControls` and commonly appear
// in user-generated content, especially in mixed-script environments.
static FORMAT_CONTROLS: phf::Set<char> = phf_set! {
    '\u{200B}', // Zero-width space
    '\u{200C}', // Zero-width non-joiner
    '\u{200D}', // Zero-width joiner
    '\u{200E}', // LTR mark
    '\u{200F}', // RTL mark
    '\u{202A}', // LTR embedding
    '\u{202B}', // RTL embedding
    '\u{202C}', // Pop directional formatting
    '\u{202D}', // LTR override
    '\u{202E}', // RTL override
    '\u{2060}', // Word joiner
    '\u{2061}', // Invisible function application
    '\u{2062}', // Invisible times
    '\u{2063}', // Invisible separator
    '\u{2064}', // Invisible plus
    '\u{2066}', // LTR isolate
    '\u{2067}', // RTL isolate
    '\u{2068}', // First-strong isolate
    '\u{2069}', // Pop isolate
    '\u{206A}', // Inhibit symmetric swapping (deprecated)
    '\u{206B}', // Activate symmetric swapping (deprecated)
    '\u{206C}', // Inhibit Arabic shaping (deprecated)
    '\u{206D}', // Activate Arabic shaping (deprecated)
    '\u{206E}', // National digit shapes (deprecated)
    '\u{206F}', // Nominal digit shapes (deprecated)
    '\u{FEFF}', // Zero-width no-break space / BOM
};

// Unicode whitespace characters excluding ASCII space, tab, LF, CR.
//
// These are normalized to plain ASCII space when `normalize_unicode = true`.
// static UNICODE_WHITESPACE: phf::Set<char> = phf_set! {
//     '\u{00A0}', // No-break space
//     '\u{1680}', // Ogham space mark
//     '\u{2000}', // En quad
//     '\u{2001}', // Em quad
//     '\u{2002}', // En space
//     '\u{2003}', // Em space
//     '\u{2004}', // Three-per-em space
//     '\u{2005}', // Four-per-em space
//     '\u{2006}', // Six-per-em space
//     '\u{2007}', // Figure space
//     '\u{2008}', // Punctuation space
//     '\u{2009}', // Thin space
//     '\u{200A}', // Hair space
//     '\u{202F}', // Narrow no-break space
//     '\u{205F}', // Medium mathematical space
//     '\u{3000}', // Fullwidth / ideographic space
// };

// Fast scan to check for any format controls.
#[inline]
pub fn contains_format_controls(text: &str) -> bool {
    text.chars().any(is_format_control)
}

// Replace phf set lookup with an inlined, pattern/range-based check.
// This is very fast and avoids hash/table indirections.
#[inline(always)]
pub fn is_unicode_whitespace(c: char) -> bool {
    // Fast common-case checks (ASCII whitespace excluded here on purpose).
    // These ranges / code points cover the Unicode whitespace characters you listed:
    // U+00A0, U+1680, U+2000..=U+200A, U+2028, U+2029, U+202F, U+205F, U+3000
    //
    // Note: char::is_whitespace covers a superset; we only want the
    // additional "unicode whitespace mapped to ASCII space" set.
    matches!(
        c as u32,
        0x0085 | // NEXT LINE (NEL)
        0x00A0 | // NO-BREAK SPACE
        0x1680 | // OGHAM SPACE MARK
        0x2000
            ..=0x200A | // EN QUAD through HAIR SPACE
        0x2028 | // LINE SEPARATOR
        0x2029 | // PARAGRAPH SEPARATOR
        0x202F | // NARROW NO-BREAK SPACE
        0x205F | // MEDIUM MATHEMATICAL SPACE
        0x3000 // IDEOGRAPHIC SPACE
    )
}

#[inline(always)]
pub fn is_any_whitespace(c: char) -> bool {
    // Use char::is_whitespace (covers ASCII + many Unicode WS)
    // plus our explicit set to capture any whitespace not included
    // by the standard predicate that we want to normalize.
    c.is_whitespace() || is_unicode_whitespace(c)
}

// Fast ASCII whitespace check using lookup table (unchanged semantics).
// Kept small and annotated for inlining.
static ASCII_WS_TABLE: [bool; 256] = {
    let mut table = [false; 256];
    table[b' ' as usize] = true;
    table[b'\t' as usize] = true;
    table[b'\n' as usize] = true;
    table[b'\r' as usize] = true;
    table[b'\x0B' as usize] = true; // Vertical tab
    table[b'\x0C' as usize] = true; // Form feed
    table
};

#[inline(always)]
pub fn is_ascii_whitespace_fast(b: u8) -> bool {
    // direct table lookup - extremely cheap
    ASCII_WS_TABLE[b as usize]
}

/// Fast check if a byte could be the start of Unicode whitespace in UTF-8.
///
/// This is intentionally conservative: it returns true for lead bytes
/// that commonly begin the target whitespace code points. It helps
/// avoid a full UTF-8/char decode for mostly-ASCII text.
///
/// It's kept intentionally simple (single-byte check) because it is
/// only used as a quick pre-screen in `needs_apply`.
#[inline(always)]
pub fn could_be_unicode_ws_start(b: u8) -> bool {
    // The UTF-8 lead bytes for our target whitespace codepoints:
    // 0xC2 -> U+00A0,
    // 0xE1 -> U+1680,
    // 0xE2 -> many U+2000.. range and 0x202F, 0x205F
    // 0xE3 -> U+3000
    matches!(b, 0xC2 | 0xE1 | 0xE2 | 0xE3)
}

#[inline(always)]
pub fn is_format_control(c: char) -> bool {
    FORMAT_CONTROLS.contains(&c)
}

// Control characters (Category Cc). Format controls (Cf) are handled separately.
#[inline(always)]
pub fn is_control(c: char) -> bool {
    let cp = c as u32;
    cp <= 0x1F || (0x7F..=0x9F).contains(&cp)
}

// Fullwidth Latin punctuation/letters in FF01–FF5E plus ideographic space.
#[inline(always)]
pub fn is_fullwidth(c: char) -> bool {
    let cp = c as u32;
    (0xFF01..=0xFF5E).contains(&cp) || cp == 0x3000
}

#[inline(always)]
pub fn fullwidth_to_halfwidth(c: char) -> char {
    let cp = c as u32;
    if (0xFF01..=0xFF5E).contains(&cp) {
        char::from_u32(cp - 0xFEE0).unwrap_or(c)
    } else if cp == 0x3000 {
        ' '
    } else {
        c
    }
}

// Punctuation normalization table.
static PUNCT_NORM: phf::Map<char, char> = phf_map! {
    '“' => '"', '”' => '"', '„' => '"', '«' => '"', '»' => '"',
    '‘' => '\'', '’' => '\'', '‚' => '\'',
    '–' => '-', '—' => '-', '─' => '-', '―' => '-',
    '…' => '.', '⋯' => '.', '․' => '.', '‧' => '.',
    '•' => '*', '·' => '*', '∙' => '*',
    '‹' => '<', '›' => '>',
    '′' => '"', '″' => '"',
};

#[inline(always)]
pub fn normalize_punctuation_char(c: char) -> char {
    PUNCT_NORM.get(&c).copied().unwrap_or(c)
}

// Hangul syllables + jamo + compatibility + extended ranges.
#[inline(always)]
pub fn is_hangul(c: char) -> bool {
    matches!(c as u32,
        0xAC00..=0xD7AF  | // Syllables
        0x1100..=0x11FF  | // Jamo
        0x3130..=0x318F  | // Compatibility Jamo
        0xA960..=0xA97F  | // Jamo Ext A
        0xD7B0..=0xD7FF    // Jamo Ext B
    )
}

// Hiragana block.
#[inline(always)]
pub fn is_hiragana(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x309F)
}

// Katakana + small extensions.
#[inline(always)]
pub fn is_katakana(c: char) -> bool {
    matches!(c as u32,
        0x30A0..=0x30FF  | // Katakana
        0x31F0..=0x31FF    // Phonetic Extensions
    )
}

// Kana Supplement (U+1B000+).
#[inline(always)]
pub fn is_kana_supplement(c: char) -> bool {
    matches!(c as u32, 0x1B000..=0x1B16F)
}

// Unified Han blocks + extensions A–G + compatibility block.
#[inline(always)]
pub fn is_cjk_unified_ideograph(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF   | // Unified
        0x3400..=0x4DBF   | // Ext A
        0x20000..=0x2A6DF | // Ext B
        0x2A700..=0x2B73F | // Ext C
        0x2B740..=0x2B81F | // Ext D
        0x2B820..=0x2CEAF | // Ext E
        0x2CEB0..=0x2EBEF | // Ext F
        0x30000..=0x3134F | // Ext G
        0x31350..=0x323AF | // Extension I (Unicode 16.0)
        0xF900..=0xFAFF     // Compatibility
    )
}

#[inline(always)]
pub fn is_kangxi_radical(c: char) -> bool {
    matches!(c as u32, 0x2F00..=0x2FDF)
}

// Han/Kana cluster excluding Hangul.
#[inline(always)]
pub fn is_cjk_han_or_kana(c: char) -> bool {
    is_cjk_unified_ideograph(c)
        || is_hiragana(c)
        || is_katakana(c)
        || is_kana_supplement(c)
        || is_kangxi_radical(c)
}

// Southeast Asian scripts with syllable-based segmentation.
#[inline(always)]
pub fn is_se_asian_script(c: char) -> bool {
    matches!(c as u32,
        0x0E00..=0x0E7F  | // Thai
        0x0E80..=0x0EFF  | // Lao
        0x1000..=0x109F  | // Myanmar
        0xAA60..=0xAA7F  | // Myanmar Ext-A
        0xA9E0..=0xA9FF  | // Myanmar Ext-B
        0x1780..=0x17FF  | // Khmer
        0x19E0..=0x19FF  | // Khmer Symbols
        0x1A00..=0x1AAF    // Tai Tham
    )
}

#[inline(always)]
pub fn is_indic_script(c: char) -> bool {
    matches!(c as u32,
        0x0900..=0x097F | // Devanagari
        0x0980..=0x09FF | // Bengali
        0x0A00..=0x0A7F | // Gurmukhi
        0x0A80..=0x0AFF | // Gujarati
        0x0B00..=0x0B7F | // Oriya
        0x0B80..=0x0BFF | // Tamil
        0x0C00..=0x0C7F | // Telugu
        0x0C80..=0x0CFF | // Kannada
        0x0D00..=0x0D7F | // Malayalam
        0x0D80..=0x0DFF | // Sinhala
        0xA8E0..=0xA8FF | // Devanagari Extended
        0x11FC0..=0x11FFF // Tamil Supplement
    )
}

pub const fn is_virama(c: char) -> bool {
    matches!(
        c as u32,
        0x094D | // Devanagari
        0x09CD | // Bengali
        0x0A4D | // Gurmukhi
        0x0ACD | // Gujarati
        0x0B4D | // Oriya
        0x0BCD | // Tamil
        0x0C4D | // Telugu
        0x0CCD | // Kannada
        0x0D4D | // Malayalam
        0x0DCA | // Sinhala
        0x103A | // Myanmar
        0x17D2 | // Khmer
        0x1BAA | // Tai Tham
        0x1B44 // Balinese
    )
}

// Codepoints for the most common non-breaking conjunct initial consonants in Devanagari (Hindi)
// 'र' (ra), 'य' (ya), 'व' (va), 'ह' (ha)
// The heuristic prevents ZWSP insertion when the Virama is followed by one of these characters.
#[inline(always)]
pub const fn should_prevent_indic_break(c: char) -> bool {
    matches!(
        c as u32,
        0x0930 | // Devanagari 'र' (ra)
        0x092F | // Devanagari 'य' (ya)
        0x0935 | // Devanagari 'व' (va)
        0x0939 // Devanagari 'ह' (ha)
    )
}

#[inline(always)]
const fn is_modern_alphabetic_script(cp: u32) -> bool {
    matches!(cp,
        0x0370..=0x03FF | // Greek + Coptic
        0x0400..=0x052F | // Cyrillic + Supplement
        0x0530..=0x058F | // Armenian
        0x0590..=0x05FF | // Hebrew
        0x0600..=0x06FF | // Arabic + Syriac
        0x0700..=0x074F | // Syriac
        0x0750..=0x077F | // Arabic Supplement
        0x0870..=0x089F | // Arabic Extended-B
        0x08A0..=0x08FF | // Arabic Extended-A
        0x10A0..=0x10FF | // Georgian
        0x13A0..=0x13FF   // Cherokee
    )
}

#[inline(always)]
pub fn zwsp() -> char {
    '\u{200B}'
}

#[inline(always)]
pub fn is_extended_latin(c: char) -> bool {
    matches!(c as u32, 0x00C0..=0x02AF) // Latin-1 Supplement + Extended A/B
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum CharClass {
    Other,        // Symbols, emojis, non-script, etc.
    Whitespace,   // ASCII + Unicode spaces
    Western,      // ASCII letters/digits/punct
    Cjk,          // Han ideographs + Kana + Kangxi radicals
    Hangul,       // Hangul syllables & Jamo
    SEAsian,      // Thai, Lao, Myanmar, Khmer, Tai Tham
    NonCJKScript, // Greek, Cyrillic, Arabic, Hebrew, etc.
    Indic,
}

#[inline(always)]
pub fn classify(c: char) -> CharClass {
    let cp = c as u32;

    // Fast path: pure ASCII → zero-copy golden path
    if cp < 0x80 {
        if c.is_ascii_whitespace() {
            return CharClass::Whitespace;
        }
        if c.is_ascii_alphanumeric() || c.is_ascii_punctuation() {
            return CharClass::Western;
        }
        return CharClass::Other;
    }

    if is_any_whitespace(c) {
        return CharClass::Whitespace;
    }
    if is_cjk_han_or_kana(c) {
        return CharClass::Cjk;
    }
    if is_hangul(c) {
        return CharClass::Hangul;
    }
    if is_se_asian_script(c) {
        return CharClass::SEAsian;
    }
    if is_indic_script(c) {
        return CharClass::Indic;
    }
    if is_extended_latin(c) {
        return CharClass::Western;
    }

    if is_modern_alphabetic_script(cp) {
        return CharClass::NonCJKScript;
    }

    CharClass::Other // Ancient scripts, symbols, emojis, medieval Latin
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn format_controls_are_correctly_detected() {
        for c in ['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}', '\u{2066}'] {
            assert!(
                is_format_control(c),
                "Missed format control U+{:04X}",
                c as u32
            );
        }
        assert!(!is_format_control('A'));
        assert!(!is_format_control(' '));
    }

    #[test]
    fn unicode_whitespace_is_correctly_detected() {
        assert!(is_unicode_whitespace('\u{00A0}'));
        assert!(is_unicode_whitespace('\u{3000}'));
        assert!(!is_unicode_whitespace(' '));
    }

    #[test]
    fn control_characters() {
        assert!(is_control('\0'));
        assert!(is_control('\u{001F}'));
        assert!(is_control('\u{007F}'));
        assert!(!is_control(' '));
    }

    #[test]
    fn char_classification() {
        // Western zero-copy path
        for c in ['h', '5', '!', 'é', 'Ā', 'ſ', 'Ə', 'ǃ'] {
            assert_eq!(
                classify(c),
                CharClass::Western,
                "Western failed U+{:04X}",
                c as u32
            );
        }

        // Modern scripts that need script transitions
        for c in ['Я', 'α', 'מ', 'م', 'ა', 'Բ', 'ܐ'] {
            // Cyrillic, Greek, Hebrew, Arabic, Georgian, Armenian, Syriac
            assert_eq!(
                classify(c),
                CharClass::NonCJKScript,
                "Modern script failed U+{:04X}",
                c as u32
            );
        }

        // Historical, ancient, symbols → Other (no breaks)
        for c in ['𐍈', 'Ꝛ', 'ᚠ', '☭', '𐤀', '𒀀', '𐌰', 'አ', '\u{200B}'] {
            assert_eq!(
                classify(c),
                CharClass::Other,
                "Should be Other U+{:04X}",
                c as u32
            );
        }

        // Whitespace
        for c in [' ', '\t', '\u{00A0}', '\u{3000}', '\u{1680}'] {
            assert_eq!(classify(c), CharClass::Whitespace);
        }
    }

    #[test]
    fn classify_is_exhaustive_and_correct() {
        use CharClass::*;

        // Helper macro to reduce repetition
        macro_rules! assert_class {
            ($c:expr, $expected:expr) => {
                assert_eq!(
                    classify($c),
                    $expected,
                    "U+{:04X} '{}' misclassified",
                    $c as u32,
                    $c
                );
            };
        }

        // --- ASCII ---
        assert_class!('A', Western);
        assert_class!('5', Western);
        assert_class!('!', Western);

        // --- Extended Latin (the hard-coded range!) ---
        assert_class!('À', Western); // U+00C0
        assert_class!('ÿ', Western); // U+00FF
        assert_class!('Ā', Western); // U+0100
        assert_class!('ſ', Western); // U+017F long s
        assert_class!('Ə', Western); // U+018F schwa (Azeri/Turkish)
        assert_class!('ǃ', Western); // U+01C3 click (Khoisan orthographies)

        // --- Full CJK coverage ---
        assert_class!('𱁬', Cjk); // CJK Ext H (U+3106C) — very new
        assert_class!('𲁨', Cjk); // CJK Ext I (U+32068)
        assert_class!('豈', Cjk); // Compatibility Ideograph F900
        assert_class!('㐀', Cjk); // CJK Compatibility (U+3400)
        assert_class!('㐀', Cjk); // CJK Ext A (U+3400)
        assert_class!('世', Cjk);
        assert_class!('界', Cjk);

        // --- Full Indic coverage ---
        assert_class!('অ', Indic); // Bengali
        assert_class!('ਅ', Indic); // Gurmukhi
        assert_class!('અ', Indic); // Gujarati
        assert_class!('ଅ', Indic); // Oriya
        assert_class!('అ', Indic); // Telugu
        assert_class!('ಕ', Indic); // Kannada
        assert_class!('മ', Indic); // Malayalam
        assert_class!('අ', Indic); // Sinhala

        // --- Myanmar extended ---
        assert_class!('ꩠ', SEAsian); // Myanmar Ext B

        // --- Khmer symbols ---
        assert_class!('᧠', SEAsian); // Khmer symbol

        // --- Non-alphabetic scripts that must NOT be NonCJKScript ---
        assert_class!('𐐧', Other); // U+10427 Deseret long I
        assert_class!('𐐀', Other); // U+10400 Deseret capital H
        assert_class!('Ꝛ', Other); // U+A75A (medieval abbreviations)
        assert_class!('★', Other);
        assert_class!('☭', Other);
        assert_class!('𐍈', Other); // Gothic

        // --- Zero-width & format controls ---
        assert_class!('\u{200B}', Other); // ZWSP
        assert_class!('\u{2060}', Other); // Word joiner
        assert_class!('\u{FEFF}', Other); // BOM

        // --- Whitespace edge cases ---
        assert_class!('\u{1680}', Whitespace); // Ogham space mark
        assert_class!('\u{2028}', Whitespace); // Line separator (treated as whitespace by is_whitespace())
    }
}
