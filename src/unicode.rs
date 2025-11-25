//! Unicode character classification utilities for Normy.
//!
//! - Single source of truth for language-agnostic character categories
//! - Zero-cost abstractions, inlined for optimized segmentation pipelines
//! - Includes script detection, whitespace categories, fullwidth/halfwidth ranges,
//!   punctuation normalization, and control/format character checks.

use phf::{phf_map, phf_set};

/// Format control characters (General Category = Cf) and selected
/// zero-width characters relevant to text normalization.
///
/// These are removed by `RemoveFormatControls` and commonly appear
/// in user-generated content, especially in mixed-script environments.
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

/// Unicode whitespace characters excluding ASCII space, tab, LF, CR.
///
/// These are normalized to plain ASCII space when `normalize_unicode = true`.
static UNICODE_WHITESPACE: phf::Set<char> = phf_set! {
    '\u{00A0}', // No-break space
    '\u{1680}', // Ogham space mark
    '\u{2000}', // En quad
    '\u{2001}', // Em quad
    '\u{2002}', // En space
    '\u{2003}', // Em space
    '\u{2004}', // Three-per-em space
    '\u{2005}', // Four-per-em space
    '\u{2006}', // Six-per-em space
    '\u{2007}', // Figure space
    '\u{2008}', // Punctuation space
    '\u{2009}', // Thin space
    '\u{200A}', // Hair space
    '\u{202F}', // Narrow no-break space
    '\u{205F}', // Medium mathematical space
    '\u{3000}', // Fullwidth / ideographic space
};

#[inline(always)]
pub fn is_format_control(c: char) -> bool {
    FORMAT_CONTROLS.contains(&c)
}

#[inline(always)]
pub fn is_unicode_whitespace(c: char) -> bool {
    UNICODE_WHITESPACE.contains(&c)
}

#[inline(always)]
pub fn is_any_whitespace(c: char) -> bool {
    c.is_whitespace() || is_unicode_whitespace(c)
}

// /// Basic Latin + Latin-1 Supplement + Latin Extended-A/B.
// #[inline(always)]
// pub fn is_latin_letter(c: char) -> bool {
//     matches!(c as u32,
//         0x0041..=0x005A |   // A–Z
//         0x0061..=0x007A |   // a–z
//         0x00C0..=0x00FF |   // Latin-1 Supplement
//         0x0100..=0x02AF     // Latin Extended A/B
//     )
// }

// #[inline(always)]
// pub fn is_japanese_kana(c: char) -> bool {
//     is_hiragana(c) || is_katakana(c) || is_kana_supplement(c)
// }

// /// Characters treated as “script units” for word segmentation.
// #[inline(always)]
// pub fn is_segmentation_script_char(c: char) -> bool {
//     is_cjk_han_or_kana(c) || is_hangul(c) || is_se_asian_script(c)
// }

/// Control characters (Category Cc). Format controls (Cf) are handled separately.
#[inline(always)]
pub fn is_control(c: char) -> bool {
    let cp = c as u32;
    cp <= 0x1F || (0x7F..=0x9F).contains(&cp)
}

/// Fast scan to check for any format controls.
#[inline]
pub fn contains_format_controls(text: &str) -> bool {
    text.chars().any(is_format_control)
}

/// Fullwidth Latin punctuation/letters in FF01–FF5E plus ideographic space.
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

/// Hangul syllables + jamo + compatibility + extended ranges.
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

/// Hiragana block.
#[inline(always)]
pub fn is_hiragana(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x309F)
}

/// Katakana + small extensions.
#[inline(always)]
pub fn is_katakana(c: char) -> bool {
    matches!(c as u32,
        0x30A0..=0x30FF  | // Katakana
        0x31F0..=0x31FF    // Phonetic Extensions
    )
}

/// Kana Supplement (U+1B000+).
#[inline(always)]
pub fn is_kana_supplement(c: char) -> bool {
    matches!(c as u32, 0x1B000..=0x1B16F)
}

/// Unified Han blocks + extensions A–G + compatibility block.
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
        0xF900..=0xFAFF     // Compatibility
    )
}

#[inline(always)]
pub fn is_kangxi_radical(c: char) -> bool {
    matches!(c as u32, 0x2F00..=0x2FDF)
}

/// Han/Kana cluster excluding Hangul.
#[inline(always)]
pub fn is_cjk_han_or_kana(c: char) -> bool {
    is_cjk_unified_ideograph(c)
        || is_hiragana(c)
        || is_katakana(c)
        || is_kana_supplement(c)
        || is_kangxi_radical(c)
}

/// Southeast Asian scripts with syllable-based segmentation.
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

/// ASCII alphabetic only.
#[inline(always)]
pub fn is_ascii_letter(c: char) -> bool {
    matches!(c as u32,
        0x0041..=0x005A | // A–Z
        0x0061..=0x007A   // a–z
    )
}

// /// Script cluster test used by segmentation.
// #[inline(always)]
// pub fn is_same_script_cluster(a: char, b: char) -> bool {
//     match (classify(a), classify(b)) {
//         (CharClass::Western, CharClass::Western) => true,
//         (CharClass::Cjk, CharClass::Cjk) => true,
//         (CharClass::Hangul, CharClass::Hangul) => true,
//         (CharClass::SEAsian, CharClass::SEAsian) => true,
//         _ if is_cjk_unified_ideograph(a) && is_cjk_unified_ideograph(b) => true,
//         _ => false,
//     }
// }

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
}

// Key fix in unicode.rs - classify() function

#[inline(always)]
pub fn classify(c: char) -> CharClass {
    // 1. Fast ASCII
    if c.is_ascii() {
        if c.is_ascii_whitespace() {
            return CharClass::Whitespace;
        }
        // CRITICAL FIX: Include ASCII punctuation in Western class
        // This is essential for proper segmentation boundaries
        // Industry standard (ICU, UAX #29): digits and punctuation
        // should trigger boundaries when adjacent to script characters
        if c.is_ascii_alphanumeric() || c.is_ascii_punctuation() {
            return CharClass::Western;
        }
        // Other ASCII symbols → Other
        return CharClass::Other;
    }

    // 2. Unicode whitespace
    if is_unicode_whitespace(c) {
        return CharClass::Whitespace;
    }

    // 3. CJK cluster
    if is_cjk_han_or_kana(c) {
        return CharClass::Cjk;
    }

    // 4. Hangul
    if is_hangul(c) {
        return CharClass::Hangul;
    }

    // 5. Southeast Asian scripts
    if is_se_asian_script(c) {
        return CharClass::SEAsian;
    }

    // 6. Western extended letters (Latin-1 / Extended-A/B)
    if ('\u{00C0}'..='\u{02AF}').contains(&c) || is_ascii_letter(c) {
        return CharClass::Western;
    }

    // 7. Everything else that is a script character (Cyrillic, Greek, Arabic, etc.)
    if c.is_alphabetic() {
        return CharClass::NonCJKScript;
    }

    // 8. Fallback
    CharClass::Other
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

    // #[test]
    // fn latin_letters() {
    //     assert!(is_latin_letter('é'));
    //     assert!(is_latin_letter('Ĝ'));
    //     assert!(!is_latin_letter('あ'));
    // }

    // #[test]
    // fn ideographic_characters() {
    //     assert!(is_ideographic('漢'));
    //     assert!(is_ideographic('가'));
    //     assert!(!is_ideographic('A'));
    // }

    #[test]
    fn control_characters() {
        assert!(is_control('\0'));
        assert!(is_control('\u{001F}'));
        assert!(is_control('\u{007F}'));
        assert!(!is_control(' '));
    }

    #[test]
    fn char_classification() {
        // --- Western ASCII
        for c in &['h', '5', '!', 'é'] {
            assert_eq!(classify(*c), CharClass::Western, "Failed for {}", c);
        }

        // --- Whitespace
        for c in &[' ', '\t', '\u{00A0}', '\u{3000}'] {
            assert_eq!(classify(*c), CharClass::Whitespace, "Failed for {:?}", c);
        }

        // --- CJK
        for c in &['日', 'ア', '漢'] {
            assert_eq!(classify(*c), CharClass::Cjk, "Failed for {}", c);
        }

        // --- Hangul
        for c in &['가', '각', '똠'] {
            assert_eq!(classify(*c), CharClass::Hangul, "Failed for {}", c);
        }

        // --- Southeast Asian scripts
        for c in &['ก', 'ข', 'ກ', 'ຂ', 'က', 'ခ'] {
            assert_eq!(classify(*c), CharClass::SEAsian, "Failed for {}", c);
        }

        // --- Non-CJK scripts (Greek, Cyrillic, Arabic, etc.)
        for c in &['Я', 'α', 'م'] {
            assert_eq!(classify(*c), CharClass::NonCJKScript, "Failed for {}", c);
        }

        // --- Other (symbols, ZWSP, emojis)
        for c in &['\u{200B}', '🎉', '©', '™'] {
            assert_eq!(classify(*c), CharClass::Other, "Failed for {:?}", c);
        }
    }
}
