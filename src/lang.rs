//! lang.rs – Compile-time language metadata with zero-runtime-cost helpers
//!
//! Design: single macro invocation as source-of-truth, compile-time metadata,
//! slices for tiny O(k) lookups, explicit peek-pairs for context-sensitive folds.

use paste::paste;
use phf::{Map, phf_map};

/// ---------------------------------------------------------------------------
/// 1. Public Language Identifier
/// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lang {
    code: &'static str,
    name: &'static str,
}

impl Lang {
    #[inline(always)]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[inline(always)]
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

/// Default language (used when none is supplied).
pub const DEFAULT_LANG: Lang = Lang {
    code: "ENG",
    name: "English",
};

/// ---------------------------------------------------------------------------
/// 2. Data Types
/// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug)]
pub struct CaseMap {
    pub from: char,
    pub to: char,
}

#[derive(Clone, Copy, Debug)]
pub struct FoldMap {
    pub from: char,
    pub to: &'static str,
}

pub type DiacriticSet = &'static [char];

/// peek pair triple: (first_char, second_char, target_str)
#[derive(Clone, Copy, Debug)]
pub struct PeekPair {
    pub a: char,
    pub b: char,
    pub to: &'static str,
}

/// ---------------------------------------------------------------------------
/// 3. Language Entry (compile-time metadata)
/// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug)]
pub struct LangEntry {
    pub case_map: &'static [CaseMap],
    pub fold_map: &'static [FoldMap],
    pub diacritics: Option<DiacriticSet>,
    pub needs_segmentation: bool,

    // Explicit compile-time flags / small slices for O(k) checks
    pub requires_peek_ahead: bool,
    pub fold_char_slice: &'static [char],
    pub diacritic_slice: Option<&'static [char]>,
    pub peek_pairs: &'static [PeekPair],
}

impl LangEntry {
    /// Check if all fold mappings are 1→1 characters.
    #[inline]
    pub fn has_one_to_one_folds(&self) -> bool {
        if self.fold_map.is_empty() {
            return true;
        }
        self.fold_map.iter().all(|m| m.to.chars().count() == 1)
    }

    /// Check if any fold mapping expands to multiple characters.
    #[inline]
    pub fn has_multi_char_folds(&self) -> bool {
        self.fold_map.iter().any(|m| m.to.chars().count() > 1)
    }
}

/// ---------------------------------------------------------------------------
/// 4. Macro – generates everything from a single table
///    Fields: peek_pairs: [ ($a, $b => $target),* ]
/// ---------------------------------------------------------------------------
macro_rules! define_languages {
    ($(
        $code:ident, $code_str:literal, $name:literal,
        case: [ $($cfrom:expr => $cto:expr),* $(,)? ],
        fold: [ $($ffrom:expr => $fto:expr),* $(,)? ],
        diac: [ $($d:expr),* $(,)? ],
        segment: $segment:expr,
        peek_ahead: $peek:expr,
        peek_pairs: [ $( ($pa:expr, $pb:expr => $pto:expr) ),* $(,)? ]
    );* $(;)?) => {
        // 4.1 Public `Lang` constants
        $(
            pub const $code: Lang = Lang { code: $code_str, name: $name };
        )*

        // 4.2 Per-language static data modules
        $(
            paste! {
                mod [<$code:lower _data>] {
                    use super::*;

                    pub static CASE: &[CaseMap] = &[
                        $(CaseMap { from: $cfrom, to: $cto }),*
                    ];

                    pub static FOLD: &[FoldMap] = &[
                        $(FoldMap { from: $ffrom, to: $fto }),*
                    ];

                    pub static DIAC: &[char] = &[$($d),*];

                    pub const NEEDS_SEGMENTATION: bool = $segment;
                    pub const REQUIRES_PEEK_AHEAD: bool = $peek;

                    // small slices (always valid even if empty)
                    pub static FOLD_CHAR_SLICE: &[char] = &[$($ffrom),*];
                    pub static DIACRITIC_SLICE: &[char] = &[$($d),*];

                    pub static PEEK_PAIRS: &[PeekPair] = &[
                        $( PeekPair { a: $pa, b: $pb, to: $pto } ),*
                    ];
                }
            }
        )*

        // 4.3 Global lookup table (public)
        paste! {
            pub static LANG_TABLE: Map<&'static str, LangEntry> = phf_map! {
                $(
                    $code_str => LangEntry {
                        case_map: [<$code:lower _data>]::CASE,
                        fold_map: [<$code:lower _data>]::FOLD,
                        diacritics: if [<$code:lower _data>]::DIAC.is_empty() {
                            None
                        } else {
                            Some([<$code:lower _data>]::DIAC)
                        },
                        needs_segmentation: [<$code:lower _data>]::NEEDS_SEGMENTATION,
                        requires_peek_ahead: [<$code:lower _data>]::REQUIRES_PEEK_AHEAD,
                        fold_char_slice: [<$code:lower _data>]::FOLD_CHAR_SLICE,
                        diacritic_slice: if [<$code:lower _data>]::DIAC.is_empty() {
                            None
                        } else {
                            Some([<$code:lower _data>]::DIACRITIC_SLICE)
                        },
                        peek_pairs: [<$code:lower _data>]::PEEK_PAIRS,
                    },
                )*
            };
        }

        // 4.4 Helper: `Lang::from_code`
        pub fn from_code(code: &str) -> Option<Lang> {
            let upper = code.to_uppercase();
            match upper.as_str() {
                $(
                    $code_str => Some($code),
                )*
                _ => None,
            }
        }
    };
}

// ---------------------------------------------------------------------------
// 5. Language definitions (single source of truth)
//    Note: peek_pairs provided only where needed (Dutch IJ as example)
// ---------------------------------------------------------------------------
define_languages! {
    TUR, "TUR", "Turkish",
        case: [ 'I' => 'ı', 'İ' => 'i' ],
        fold: [ 'I' => "ı", 'İ' => "i" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    DEU, "DEU", "German",
        case: [],
        fold: [ 'ß' => "ss", 'ẞ' => "ss" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    NLD, "NLD", "Dutch",
        case: [],
        // only the canonical ligature entries in fold map
        fold: [ 'Ĳ' => "ij", 'ĳ' => "ij" ],
        diac: [],
        segment: false,
        // We want contextual IJ handling -> provide peek pairs
        peek_ahead: true,
        peek_pairs: [
            // uppercase and lowercase variants
            ('I', 'J' => "ij"),
            ('i', 'j' => "ij"),
        ];

    DAN, "DAN", "Danish",
        case: [],
        fold: [ 'Å' => "aa", 'å' => "aa" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    NOR, "NOR", "Norwegian",
        case: [],
        fold: [ 'Æ' => "ae", 'æ' => "ae", 'Ø' => "oe", 'ø' => "oe" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    SWE, "SWE", "Swedish",
        case: [],
        fold: [ 'Å' => "aa", 'å' => "aa", 'Ä' => "ae", 'ä' => "ae", 'Ö' => "oe", 'ö' => "oe" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    LIT, "LIT", "Lithuanian",
        case: [],
        fold: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    LAV, "LAV", "Latvian",
        case: [],
        fold: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    ARA, "ARA", "Arabic",
        case: [],
        fold: [],
        diac: [ 'َ', 'ِ', 'ُ', 'ً', 'ٌ', 'ٍ', 'ْ', 'ّ', 'ٓ', 'ٔ', 'ٕ' ],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    HEB, "HEB", "Hebrew",
        case: [],
        fold: [],
        diac: [ 'ּ', 'ְ', 'ֱ', 'ֲ', 'ֳ', 'ִ', 'ֵ', 'ֶ', 'ַ', 'ָ', 'ֹ', 'ֻ', 'ֿ', 'ׁ', 'ׂ' ],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    VIE, "VIE", "Vietnamese",
        case: [],
        fold: [],
        diac: [ '̉', '̃', '́', '̀', '̣', '̂', '̄', '̆', '̛' ],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    JPN, "JPN", "Japanese",
        case: [],
        fold: [],
        diac: [],
        segment: true,
        peek_ahead: false,
        peek_pairs: [];

    ZHO, "ZHO", "Chinese",
        case: [],
        fold: [],
        diac: [],
        segment: true,
        peek_ahead: false,
        peek_pairs: [];

    KOR, "KOR", "Korean",
        case: [],
        fold: [],
        diac: [],
        segment: true,
        peek_ahead: false,
        peek_pairs: [];

    THA, "THA", "Thai",
        case: [],
        fold: [],
        diac: [],
        segment: true,
        peek_ahead: false,
        peek_pairs: [];

    MYA, "MYA", "Myanmar",
        case: [],
        fold: [],
        diac: [],
        segment: true,
        peek_ahead: false,
        peek_pairs: [];

    KHM, "KHM", "Khmer",
        case: [],
        fold: [],
        diac: [],
        segment: true,
        peek_ahead: false,
        peek_pairs: [];

    FRA, "FRA", "French",
        case: [],
        fold: [ 'Œ' => "oe", 'œ' => "oe" ],
        diac: [ '́', '̀', '̂', '̈', '̧' ],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    CAT, "CAT", "Catalan",
        case: [],
        fold: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    HUN, "HUN", "Hungarian",
        case: [],
        fold: [ 'Ő' => "oe", 'ő' => "oe", 'Ű' => "ue", 'ű' => "ue" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    POL, "POL", "Polish",
        case: [],
        fold: [ 'Ł' => "l", 'ł' => "l" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    CES, "CES", "Czech",
        case: [],
        fold: [ 'Ď' => "d", 'ď' => "d", 'Ť' => "t", 'ť' => "t" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    SLK, "SLK", "Slovak",
        case: [],
        fold: [ 'Ľ' => "l", 'ľ' => "l", 'Ĺ' => "l", 'ĺ' => "l" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    HRV, "HRV", "Croatian",
        case: [],
        fold: [ 'ǈ' => "lj", 'ǉ' => "lj", 'ǋ' => "nj", 'ǌ' => "nj" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    SRP, "SRP", "Serbian",
        case: [],
        fold: [ 'Љ' => "lj", 'љ' => "lj", 'Њ' => "nj", 'њ' => "nj", 'Џ' => "dz", 'џ' => "dz" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    UKR, "UKR", "Ukrainian",
        case: [],
        fold: [ 'Ґ' => "g", 'ґ' => "g" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    BUL, "BUL", "Bulgarian",
        case: [],
        fold: [ 'Щ' => "sht", 'щ' => "sht" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];

    ENG, "ENG", "English",
        case: [],
        fold: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [];
}

/// ---------------------------------------------------------------------------
/// 6. LocaleBehavior – zero-runtime-cost trait with helper methods
/// ---------------------------------------------------------------------------
pub trait LocaleBehavior {
    // Core accessors
    fn id(&self) -> Lang;
    fn case_map(&self) -> &'static [CaseMap];
    fn fold_map(&self) -> &'static [FoldMap];
    fn diacritics(&self) -> Option<DiacriticSet>;
    fn needs_segmentation(&self) -> bool;

    // -------------------------------------------------------------------------
    // Case folding helpers
    // -------------------------------------------------------------------------

    /// Does this character need case folding in this language?
    /// O(k) check on a tiny slice then fallback to Unicode lowercasing.
    #[inline(always)]
    fn needs_case_fold(&self, c: char) -> bool {
        if let Some(e) = LANG_TABLE.get(self.id().code())
            && e.fold_char_slice.contains(&c)
        {
            return true;
        }
        // Unicode-level change (this checks if lowercase differs)
        c.to_lowercase().next() != Some(c)
    }

    /// Fold a single character (1→1 only).
    /// **Panics** in debug mode if the mapping is multi-char.
    #[inline(always)]
    fn fold_char(&self, c: char) -> char {
        let fold_map = self.fold_map();

        // Fast path: no language-specific rules
        if fold_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if c.is_ascii() {
                return c.to_ascii_lowercase(); // ✨ Add this
            }
            return c.to_lowercase().next().unwrap_or(c);
        }

        // Language-specific mapping
        fold_map
            .iter()
            .find(|m| m.from == c)
            .map(|m| {
                let mut chars = m.to.chars();
                let first = chars.next().expect("Empty fold target");
                debug_assert!(
                    chars.next().is_none(),
                    "fold_char called on multi-char mapping: {} -> {}",
                    c,
                    m.to
                );
                first
            })
            .unwrap_or_else(|| c.to_lowercase().next().unwrap_or(c))
    }

    #[inline(always)]
    fn needs_lowercase(&self, c: char) -> bool {
        let case_map = self.case_map();

        // Check language-specific case_map first (O(k) where k is tiny, ~2 for Turkish)
        if case_map.iter().any(|m| m.from == c) {
            return true;
        }

        // Fallback to Unicode lowercase check
        c.to_lowercase().next() != Some(c)
    }

    // Add this after fold_char()

    /// Lowercase a single character (1→1 always, uses case_map).
    /// This is for the Lowercase stage, not CaseFold.
    #[inline(always)]
    fn lowercase_char(&self, c: char) -> char {
        let case_map = self.case_map();

        // Fast path: no language-specific rules
        if case_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if c.is_ascii() {
                return c.to_ascii_lowercase();
            }
            return c.to_lowercase().next().unwrap_or(c);
        }

        // Language-specific mapping (Turkish: 'İ' → 'i', 'I' → 'ı')
        case_map
            .iter()
            .find(|m| m.from == c)
            .map(|m| m.to)
            .unwrap_or_else(|| c.to_lowercase().next().unwrap_or(c))
    }

    /// Can this language use CharMapper (zero-copy path)?
    #[inline(always)]
    fn has_one_to_one_folds(&self) -> bool {
        LANG_TABLE
            .get(self.id().code())
            .map(|e| e.has_one_to_one_folds())
            .unwrap_or(true)
    }

    /// Does any fold mapping expand to multiple characters?
    #[inline(always)]
    fn has_multi_char_folds(&self) -> bool {
        LANG_TABLE
            .get(self.id().code())
            .map(|e| e.has_multi_char_folds())
            .unwrap_or(false)
    }

    /// Does this language need context-sensitive folding (peek-ahead)?
    #[inline(always)]
    fn requires_peek_ahead(&self) -> bool {
        LANG_TABLE
            .get(self.id().code())
            .map(|e| e.requires_peek_ahead)
            .unwrap_or(false)
    }

    // -------------------------------------------------------------------------
    // Context-sensitive folding helpers
    // -------------------------------------------------------------------------

    /// Check if a two-character sequence needs special handling.
    /// Returns the target string if this is a context-sensitive fold.
    #[inline]
    fn peek_ahead_fold(&self, current: char, next: Option<char>) -> Option<&'static str> {
        if !self.requires_peek_ahead() {
            return None;
        }
        let next_char = next?;

        // First: consult explicit peek_pairs (language-defined)
        if let Some(entry) = LANG_TABLE.get(self.id().code()) {
            for p in entry.peek_pairs {
                if p.a == current && p.b == next_char {
                    return Some(p.to);
                }
            }
        }

        // Second: fallback to fold_map-based heuristic:
        // If both chars map individually to the same multi-char target, return it.
        let fold_map = self.fold_map();
        let current_mapping = fold_map.iter().find(|m| m.from == current)?;
        let next_mapping = fold_map.iter().find(|m| m.from == next_char)?;
        if current_mapping.to == next_mapping.to && current_mapping.to.chars().count() > 1 {
            Some(current_mapping.to)
        } else {
            None
        }
    }

    /// Convenience: should we consume the next char as part of a two-char fold?
    #[inline]
    fn should_consume_next(&self, current: char, next: Option<char>) -> bool {
        self.peek_ahead_fold(current, next).is_some()
    }

    // -------------------------------------------------------------------------
    // Deprecated: Dutch-specific helpers (kept for backward compatibility)
    // -------------------------------------------------------------------------
    #[inline(always)]
    #[deprecated(
        since = "0.1.0",
        note = "Use peek_ahead_fold() for language-agnostic code"
    )]
    fn is_ij_start(&self, c: char, next: Option<char>) -> bool {
        self.peek_ahead_fold(c, next).is_some() && c == 'I' && next == Some('J')
    }

    #[inline(always)]
    #[deprecated(
        since = "0.1.0",
        note = "Use peek_ahead_fold() for language-agnostic code"
    )]
    fn is_ij_start_lower(&self, c: char, next: Option<char>) -> bool {
        self.peek_ahead_fold(c, next).is_some() && c == 'i' && next == Some('j')
    }

    // -------------------------------------------------------------------------
    // Diacritic helpers
    // -------------------------------------------------------------------------
    #[inline(always)]
    fn has_diacritics(&self) -> bool {
        self.diacritics().is_some()
    }

    #[inline(always)]
    fn is_diacritic(&self, c: char) -> bool {
        LANG_TABLE
            .get(self.id().code())
            .and_then(|e| e.diacritic_slice)
            .map(|slice| slice.contains(&c))
            .unwrap_or(false)
    }

    #[inline]
    fn contains_diacritics(&self, text: &str) -> bool {
        if !self.has_diacritics() {
            return false;
        }
        text.chars().any(|c| self.is_diacritic(c))
    }

    // -------------------------------------------------------------------------
    // General helpers
    // -------------------------------------------------------------------------
    #[inline]
    fn needs_trim(&self, text: &str) -> bool {
        text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace)
    }

    #[inline]
    fn count_foldable_chars(&self, text: &str) -> usize {
        text.chars().filter(|&c| self.needs_case_fold(c)).count()
    }

    #[inline]
    fn count_diacritics(&self, text: &str) -> usize {
        if !self.has_diacritics() {
            return 0;
        }
        text.chars().filter(|&c| self.is_diacritic(c)).count()
    }
}

impl LocaleBehavior for Lang {
    #[inline(always)]
    fn id(&self) -> Lang {
        *self
    }

    #[inline(always)]
    fn case_map(&self) -> &'static [CaseMap] {
        LANG_TABLE.get(self.code).map(|e| e.case_map).unwrap_or(&[])
    }

    #[inline(always)]
    fn fold_map(&self) -> &'static [FoldMap] {
        LANG_TABLE.get(self.code).map(|e| e.fold_map).unwrap_or(&[])
    }

    #[inline(always)]
    fn diacritics(&self) -> Option<DiacriticSet> {
        LANG_TABLE.get(self.code).and_then(|e| e.diacritics)
    }

    #[inline(always)]
    fn needs_segmentation(&self) -> bool {
        LANG_TABLE
            .get(self.code)
            .map(|e| e.needs_segmentation)
            .unwrap_or(false)
    }
}

/// ---------------------------------------------------------------------------
/// 7. Format control characters (for RemoveFormatControls stage)
/// ---------------------------------------------------------------------------
pub static FORMAT_CONTROLS: &[char] = &[
    '\u{200B}', // Zero-width space
    '\u{200C}', // Zero-width non-joiner
    '\u{200D}', // Zero-width joiner
    '\u{200E}', // Left-to-right mark
    '\u{200F}', // Right-to-left mark
    '\u{202A}', // Left-to-right embedding
    '\u{202B}', // Right-to-left embedding
    '\u{202C}', // Pop directional formatting
    '\u{202D}', // Left-to-right override
    '\u{202E}', // Right-to-left override
    '\u{2060}', // Word joiner
    '\u{2061}', // Function application
    '\u{2062}', // Invisible times
    '\u{2063}', // Invisible separator
    '\u{2064}', // Invisible plus
    '\u{206A}', // Inhibit symmetric swapping
    '\u{206B}', // Activate symmetric swapping
    '\u{206C}', // Inhibit Arabic form shaping
    '\u{206D}', // Activate Arabic form shaping
    '\u{206E}', // National digit shapes
    '\u{206F}', // Nominal digit shapes
    '\u{FEFF}', // Zero-width no-break space (BOM)
];

pub static FORMAT_CONTROLS_SLICE: &[char] = &[
    '\u{200B}', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202B}', '\u{202C}',
    '\u{202D}', '\u{202E}', '\u{2060}', '\u{2061}', '\u{2062}', '\u{2063}', '\u{2064}', '\u{206A}',
    '\u{206B}', '\u{206C}', '\u{206D}', '\u{206E}', '\u{206F}', '\u{FEFF}',
];

/// Is this character a format control?
#[inline(always)]
pub fn is_format_control(c: char) -> bool {
    FORMAT_CONTROLS_SLICE.contains(&c)
}

/// Does this text contain any format controls?
#[inline]
pub fn contains_format_controls(text: &str) -> bool {
    text.chars().any(is_format_control)
}

// ---------------------------------------------------------------------------
// 8. Tests (kept largely in your original spirit; adjust as needed)
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turkish_metadata() {
        let entry = LANG_TABLE.get("TUR").unwrap();
        assert!(!entry.has_multi_char_folds());
        assert!(entry.has_one_to_one_folds());
        assert!(!TUR.requires_peek_ahead());
        assert!(TUR.needs_case_fold('İ'));
        assert!(TUR.needs_case_fold('I'));
        assert_eq!(TUR.fold_char('İ'), 'i');
        assert_eq!(TUR.fold_char('I'), 'ı');
    }

    #[test]
    fn test_german_metadata() {
        let entry = LANG_TABLE.get("DEU").unwrap();
        assert!(entry.has_multi_char_folds());
        assert!(!entry.has_one_to_one_folds());
        assert!(!DEU.requires_peek_ahead());
        assert!(DEU.needs_case_fold('ß'));
    }

    #[test]
    fn test_dutch_metadata() {
        let entry = LANG_TABLE.get("NLD").unwrap();
        assert!(entry.has_multi_char_folds());
        assert!(!entry.has_one_to_one_folds());
        assert!(NLD.requires_peek_ahead());

        // IJ peek-pairs provided (both upper & lower)
        assert_eq!(NLD.peek_ahead_fold('I', Some('J')), Some("ij"));
        assert_eq!(NLD.peek_ahead_fold('i', Some('j')), Some("ij"));

        assert_eq!(NLD.peek_ahead_fold('I', Some('K')), None);
        assert_eq!(NLD.peek_ahead_fold('I', None), None);
    }

    #[test]
    fn test_english_metadata() {
        let entry = LANG_TABLE.get("ENG").unwrap();
        assert!(!entry.has_multi_char_folds());
        assert!(entry.has_one_to_one_folds());
        assert!(!ENG.requires_peek_ahead());
        assert!(ENG.needs_case_fold('A'));
        assert_eq!(ENG.fold_char('A'), 'a');
    }

    #[test]
    fn test_arabic_diacritics() {
        assert!(ARA.has_diacritics());
        assert!(ARA.is_diacritic('َ'));
        assert!(!ARA.is_diacritic('ا'));
        assert!(ARA.contains_diacritics("مَرْحَبًا"));
        assert!(!ARA.contains_diacritics("مرحبا"));
    }

    #[test]
    fn test_format_controls() {
        assert!(is_format_control('\u{200B}'));
        assert!(is_format_control('\u{FEFF}'));
        assert!(!is_format_control('a'));
        assert!(contains_format_controls("hello\u{200B}world"));
        assert!(!contains_format_controls("hello world"));
    }

    #[test]
    fn test_from_code() {
        assert_eq!(from_code("TUR"), Some(TUR));
        assert_eq!(from_code("tur"), Some(TUR));
        assert_eq!(from_code("ENG"), Some(ENG));
        assert_eq!(from_code("XXX"), None);
    }

    #[test]
    fn test_needs_trim() {
        assert!(ENG.needs_trim(" hello"));
        assert!(ENG.needs_trim("hello "));
        assert!(ENG.needs_trim(" hello "));
        assert!(!ENG.needs_trim("hello"));
    }

    #[test]
    fn test_count_foldable_chars() {
        assert_eq!(ENG.count_foldable_chars("HELLO"), 5);
        assert_eq!(ENG.count_foldable_chars("hello"), 0);
        assert_eq!(ENG.count_foldable_chars("HeLLo"), 3);
        assert_eq!(TUR.count_foldable_chars("İSTANBUL"), 8);
    }

    #[test]
    fn test_count_diacritics() {
        assert_eq!(ARA.count_diacritics("مَرْحَبًا"), 4);
        assert_eq!(ARA.count_diacritics("مرحبا"), 0);
        assert_eq!(ENG.count_diacritics("hello"), 0);
    }

    #[test]
    fn test_byte_vs_char_length() {
        let mapping = TUR.fold_map();
        let i_mapping = mapping.iter().find(|m| m.from == 'I').unwrap();
        assert_eq!(i_mapping.to.chars().count(), 1);
        let entry = LANG_TABLE.get("TUR").unwrap();
        assert!(entry.has_one_to_one_folds());
        assert!(!entry.has_multi_char_folds());
    }

    #[test]
    fn test_all_languages_have_valid_metadata() {
        let langs = [
            TUR, DEU, NLD, DAN, NOR, SWE, LIT, LAV, ARA, HEB, VIE, JPN, ZHO, KOR, THA, MYA, KHM,
            FRA, CAT, HUN, POL, CES, SLK, HRV, SRP, UKR, BUL, ENG,
        ];

        for lang in langs {
            let entry = LANG_TABLE.get(lang.code()).expect("Entry exists");

            if entry.has_one_to_one_folds() {
                assert!(!entry.has_multi_char_folds());
            }

            if entry.requires_peek_ahead {
                assert!(!entry.fold_map.is_empty() || !entry.peek_pairs.is_empty());
            }

            for fold in entry.fold_map {
                let char_count = fold.to.chars().count();

                if entry.has_one_to_one_folds() {
                    assert_eq!(
                        char_count,
                        1,
                        "{}: fold {} -> {} should be 1→1",
                        lang.code(),
                        fold.from,
                        fold.to
                    );
                }

                if char_count > 1 {
                    assert!(
                        entry.has_multi_char_folds(),
                        "{}: fold {} -> {} is multi-char but flag not set",
                        lang.code(),
                        fold.from,
                        fold.to
                    );
                }
            }

            if entry.diacritics.is_some() {
                assert!(entry.diacritic_slice.is_some());
                assert!(lang.has_diacritics());
            }
        }
    }

    #[test]
    fn test_segmentation_languages() {
        assert!(JPN.needs_segmentation());
        assert!(ZHO.needs_segmentation());
        assert!(KOR.needs_segmentation());
        assert!(THA.needs_segmentation());
        assert!(!ENG.needs_segmentation());
        assert!(!TUR.needs_segmentation());
    }

    #[test]
    fn test_case_map_only_turkish() {
        assert!(!TUR.case_map().is_empty());
        assert!(ENG.case_map().is_empty());
        assert!(DEU.case_map().is_empty());
        assert!(ARA.case_map().is_empty());
    }

    #[test]
    fn test_idempotency_metadata() {
        let langs = [
            TUR, DEU, NLD, DAN, NOR, SWE, FRA, HUN, POL, CES, SLK, HRV, SRP, UKR, BUL,
        ];

        for lang in langs {
            for fold in lang.fold_map() {
                let target_lower: String = fold.to.chars().flat_map(|c| c.to_lowercase()).collect();
                assert_eq!(
                    fold.to,
                    target_lower,
                    "{}: fold target '{}' should already be lowercase",
                    lang.code(),
                    fold.to
                );
            }
        }
    }

    #[test]
    fn test_dutch_ij_variants() {
        assert_eq!(NLD.peek_ahead_fold('I', Some('J')), Some("ij"));
        assert_eq!(NLD.peek_ahead_fold('i', Some('j')), Some("ij"));
        assert_eq!(NLD.peek_ahead_fold('I', Some('K')), None);
        assert_eq!(NLD.peek_ahead_fold('I', None), None);
        assert_eq!(ENG.peek_ahead_fold('I', Some('J')), None);
        assert_eq!(TUR.peek_ahead_fold('I', Some('J')), None);
    }

    #[test]
    fn test_peek_ahead_fold_is_generalized() {
        assert_eq!(NLD.peek_ahead_fold('I', Some('J')), Some("ij"));
        assert_eq!(NLD.peek_ahead_fold('A', Some('B')), None);
    }

    #[test]
    fn test_performance_o1_lookup() {
        let text = "AAAAAAAAAA";
        let count = text.chars().filter(|&c| ENG.needs_case_fold(c)).count();
        assert_eq!(count, 10);

        let turkish_text = "İİİİİİİİİİ";
        let count = turkish_text
            .chars()
            .filter(|&c| TUR.needs_case_fold(c))
            .count();
        assert_eq!(count, 10);
    }
}
