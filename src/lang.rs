mod entry;
mod data;
mod behaviour;

use paste::paste;
use phf::{Map, phf_map};

use crate::unicode::{CharClass, classify, is_any_whitespace, is_same_script_cluster};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentRule {
    /// Break on transition from Western (Latn, Grek, Cyrl…) → any other script
    WesternToScript,
    /// Break on transition from any other script → Western
    ScriptToWestern,
    /// Force unigram segmentation for consecutive CJK Unified Ideographs
    CJKIdeographUnigram,
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
    pub requires_peek_ahead: bool,
    pub fold_char_slice: &'static [char],
    pub diacritic_slice: Option<&'static [char]>,
    pub peek_pairs: &'static [PeekPair],
    pub segment_rules: &'static [SegmentRule],
    pub unigram_cjk: bool, // ← NEW: zero-cost gate for UnigramCJK stage
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
        peek_pairs: [ $( ($pa:expr, $pb:expr => $pto:expr) ),* $(,)? ],
        segment_rules: [ $($sr:expr),* $(,)? ],
        unigram_cjk: $unigram:expr
    ),* $(,)?) => {
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

                    pub static SEGMENT_RULES: &[SegmentRule] = &[$($sr),*];
                    pub const UNIGRAM_CJK: bool = $unigram;
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
                        diacritic_slice: if [<$code:lower _data>]::DIACRITIC_SLICE.is_empty() {
                            None
                        } else {
                            Some([<$code:lower _data>]::DIACRITIC_SLICE)
                        },
                        peek_pairs: [<$code:lower _data>]::PEEK_PAIRS,
                        segment_rules: [<$code:lower _data>]::SEGMENT_RULES,
                        unigram_cjk:         [<$code:lower _data>]::UNIGRAM_CJK,
                    }
                ),*
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
    // ──────────────────────────────────────────────────────────────────────
    // Turkish
    // ──────────────────────────────────────────────────────────────────────
    TUR,  "TUR", "Turkish",
        case: [ 'I' => 'ı', 'İ' => 'i' ],
        fold: [ 'I' => "ı", 'İ' => "i" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    // ──────────────────────────────────────────────────────────────────────
    // Germanic + Northern European (unchanged – correct as-is)
    // ──────────────────────────────────────────────────────────────────────
    DEU, "DEU", "German",
        case: [],
        fold: [ 'ß' => "ss", 'ẞ' => "ss" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    NLD, "NLD", "Dutch",
        case: [],
        fold: [ 'Ĳ' => "ij", 'ĳ' => "ij" ],
        diac: [],
        segment: false,
        peek_ahead: true,
        peek_pairs: [ ('I', 'J' => "ij") ],
        segment_rules: [],
        unigram_cjk: false,

    DAN, "DAN", "Danish",
        case: [],
        fold: [ 'Å' => "aa", 'å' => "aa" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    NOR, "NOR", "Norwegian",
        case: [],
        fold: [ 'Æ' => "ae", 'æ' => "ae", 'Ø' => "oe", 'ø' => "oe" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    SWE, "SWE", "Swedish",
        case: [],
        fold: [ 'Å' => "aa", 'å' => "aa", 'Ä' => "ae", 'ä' => "ae", 'Ö' => "oe", 'ö' => "oe" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    ARA, "ARA", "Arabic",
        case: [], fold: [], diac: [
            '\u{064E}', '\u{064F}', '\u{0650}', '\u{0651}', '\u{0652}',
            '\u{0653}', '\u{0654}', '\u{0670}', '\u{064B}', '\u{064C}',
            '\u{064D}'
        ],
        segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    HEB, "HEB", "Hebrew",
        case: [], fold: [], diac: [
            '\u{05B0}', '\u{05B1}', '\u{05B2}', '\u{05B3}', '\u{05B4}',
            '\u{05B5}', '\u{05B6}', '\u{05B7}', '\u{05B8}', '\u{05B9}',
            '\u{05BB}', '\u{05BC}', '\u{05BD}', '\u{05BF}', '\u{05C1}',
            '\u{05C2}'
        ],
        segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    VIE, "VIE", "Vietnamese",
        case: [], fold: [],
        diac: [
            '\u{0300}', '\u{0301}', '\u{0303}', '\u{0309}', '\u{0323}',
            '\u{0302}', '\u{0306}', '\u{031B}'
        ],
        segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    FRA, "FRA", "French",
        case: [],
        fold: [ 'Œ' => "oe", 'œ' => "oe", 'Æ' => "ae", 'æ' => "ae" ],
        diac: [ '\u{0301}', '\u{0300}', '\u{0302}', '\u{0308}', '\u{0327}' ],
        segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    CES, "CES", "Czech",
        case: [],
        fold: [ 'Ď' => "d", 'ď' => "d", 'Ť' => "t", 'ť' => "t", 'Ň' => "n", 'ň' => "n" ],
        diac: [ '\u{030C}', '\u{0301}', '\u{030A}' ],
        segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    SLK, "SLK", "Slovak",
        case: [],
        fold: [ 'Ľ' => "l", 'ľ' => "l", 'Ĺ' => "l", 'ĺ' => "l", 'Ŕ' => "r", 'ŕ' => "r" ],
        diac: [ '\u{030C}', '\u{0301}' ],
        segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    POL, "POL", "Polish",
        case: [],
        fold: [ 'Ł' => "l", 'ł' => "l" ],
        diac: [ '\u{0328}', '\u{0301}', '\u{0307}', '\u{02DB}' ],
        segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    CAT, "CAT", "Catalan",
        case: [], fold: [],
        diac: [ '\u{0301}', '\u{0300}', '\u{0308}', '\u{0327}' ],
        segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    SPA, "SPA", "Spanish",
        case: [], fold: [ 'Ñ' => "n", 'ñ' => "n" ],
        diac: [ '\u{0301}', '\u{0303}' ],
        segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    POR, "POR", "Portuguese",
        case: [], fold: [],
        diac: [ '\u{0301}', '\u{0300}', '\u{0303}', '\u{0302}', '\u{0327}' ],
        segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    ITA, "ITA", "Italian",
        case: [], fold: [],
        diac: [ '\u{0300}', '\u{0301}' ],
        segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    // ─────────────────────────────────────────────────────────────────────────────
    // CJK Segmentation Strategy — Normy's Official Position (2025-11-20)
    // ─────────────────────────────────────────────────────────────────────────────
    // • Chinese (ZHO): CJKIdeographUnigram = true
    //   → Matches Google, Baidu, Lucene, Elastic, Meilisearch, ICU tokenizer defaults
    //   → Maximizes recall for single-character queries ("北" finds "北京")
    //   → Expected by virtually all Chinese search/indexing systems
    //
    // • Japanese (JPN): CJKIdeographUnigram = false
    //   → Matches linguistic reality and UAX#29 default (no break in ALetter runs)
    //   → Avoids pathological tokens like 最 高
    //   → Users wanting unigram Japanese for IR must opt-in via explicit stage
    //
    // • This asymmetry is intentional and correct.
    //   Normalization ≠ tokenization. We give each language its expected default.
    // ─────────────────────────────────────────────────────────────────────────────
    // ──────────────────────────────────────────────────────────────────────
    // Asian scripts – segmentation only
    // ──────────────────────────────────────────────────────────────────────
    JPN, "JPN", "Japanese",
        case: [], fold: [], diac: [],
        segment: true, peek_ahead: false, peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,

        ],
        unigram_cjk: false,

    ZHO, "ZHO", "Chinese (Simplified)",
        case: [], fold: [], diac: [],
        segment: true, peek_ahead: false, peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::CJKIdeographUnigram,
            SegmentRule::ScriptToWestern,

        ],
        unigram_cjk: true,

    KOR, "KOR", "Korean",
        case: [], fold: [], diac: [],
        segment: true, peek_ahead: false, peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    // ──────────────────────────────────────────────────────────────────────
    // Southeast Asian Scripts (no unigram breaking, same cluster stays fused)
    // ──────────────────────────────────────────────────────────────────────
    THA, "THA", "Thai",
        case: [], fold: [], diac: [],
        segment: true, peek_ahead: false, peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    LAO, "LAO", "Lao",
        case: [], fold: [], diac: [],
        segment: true, peek_ahead: false, peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    MYA, "MYA", "Myanmar",
        case: [], fold: [], diac: [],
        segment: true, peek_ahead: false, peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    KHM, "KHM", "Khmer",
        case: [], fold: [], diac: [],
        segment: true, peek_ahead: false, peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    // ──────────────────────────────────────────────────────────────────────
    // Remaining languages (unchanged – correct)
    // ──────────────────────────────────────────────────────────────────────
    HUN, "HUN", "Hungarian",
        case: [], fold: [ 'Ő' => "oe", 'ő' => "oe", 'Ű' => "ue", 'ű' => "ue" ],
        diac: [], segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    HRV, "HRV", "Croatian",
        case: [], fold: [ 'ǈ' => "lj", 'ǉ' => "lj", 'ǋ' => "nj", 'ǌ' => "nj" ],
        diac: [], segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    SRP, "SRP", "Serbian",
        case: [], fold: [ 'Љ' => "lj", 'љ' => "lj", 'Њ' => "nj", 'њ' => "nj", 'Џ' => "dz", 'џ' => "dz" ],
        diac: [], segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    UKR, "UKR", "Ukrainian",
        case: [], fold: [ 'Ґ' => "g", 'ґ' => "g" ],
        diac: [], segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    BUL, "BUL", "Bulgarian",
        case: [], fold: [ 'Щ' => "sht", 'щ' => "sht" ],
        diac: [], segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    ENG, "ENG", "English",
        case: [], fold: [], diac: [],
        segment: false, peek_ahead: false, peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false
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
    /// Returns None if the mapping is multi-char.
    #[inline(always)]
    fn fold_char(&self, c: char) -> Option<char> {
        let fold_map = self.fold_map();

        if fold_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if c.is_ascii() {
                return Some(c.to_ascii_lowercase());
            }
            return c.to_lowercase().next();
        }

        // Check if character has language-specific mapping
        match fold_map.iter().find(|m| m.from == c) {
            Some(mapping) => {
                // Found in fold_map - verify it's 1→1
                let mut chars = mapping.to.chars();
                let first = chars.next()?;

                if chars.next().is_some() {
                    // Multi-char: cannot use in CharMapper
                    None
                } else {
                    // Single char: safe to use
                    Some(first)
                }
            }
            None => {
                // Not in fold_map - use Unicode lowercase
                c.to_lowercase().next()
            }
        }
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
    /// This is for the Lowercase stage, not FoldCase.
    #[inline(always)]
    fn lowercase_char(&self, c: char) -> char {
        let case_map = self.case_map();

        // Language-specific 1→1 (Turkish, etc.)
        if let Some(m) = case_map.iter().find(|m| m.from == c) {
            return m.to;
        }

        #[cfg(feature = "ascii-fast")]
        if c.is_ascii() {
            return c.to_ascii_lowercase();
        }

        // Unicode guarantees at least one char
        c.to_lowercase().next().unwrap_or(c)
    }

    /// Can this language use CharMapper (zero-copy path)?
    #[inline(always)]
    fn has_one_to_one_folds(&self) -> bool {
        LANG_TABLE
            .get(self.id().code())
            .map(|e| e.has_one_to_one_folds())
            .unwrap_or(true)
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
        // 1. Early-out for languages that never need peek-ahead
        if !self.requires_peek_ahead() {
            return None;
        }

        let next_char = next?;

        // --------------------------------------------------------------------
        // 2. Explicit peek-pairs (language-defined)
        // --------------------------------------------------------------------
        if let Some(entry) = LANG_TABLE.get(self.id().code()) {
            for p in entry.peek_pairs {
                // *** CASE-SENSITIVE MATCH ***
                if p.a == current && p.b == next_char {
                    return Some(p.to);
                }
            }
        }

        // --------------------------------------------------------------------
        // 3. Fallback heuristic – only for *single-char* expansions that
        //     happen to be identical for the two adjacent chars.
        // --------------------------------------------------------------------
        // This branch is **never taken for Dutch** because Dutch has no
        // single-char entries that expand to "ij".  It stays here for
        // future-proofness (e.g. a hypothetical language where both 'X' and
        // 'Y' map to "xy").
        let fold_map = self.fold_map();
        let cur = fold_map.iter().find(|m| m.from == current)?;
        let nxt = fold_map.iter().find(|m| m.from == next_char)?;

        // The heuristic must be **case-sensitive** as well – we only
        // consider the *exact* mapping, not a lower-cased version.
        if cur.to == nxt.to && cur.to.chars().count() > 1 {
            Some(cur.to)
        } else {
            None
        }
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

    /// Count foldable characters and *exact* extra bytes needed.
    /// Returns `(count, extra_bytes)`
    #[inline]
    fn count_foldable_bytes(&self, text: &str) -> (usize, usize) {
        let fold_map = self.fold_map();
        if fold_map.is_empty() {
            return (0, 0);
        }

        let mut count = 0;
        let mut extra = 0;
        for c in text.chars() {
            if let Some(m) = fold_map.iter().find(|m| m.from == c) {
                count += 1;
                let from_len = c.len_utf8();
                let to_len = m.to.len();
                if to_len > from_len {
                    extra += to_len - from_len;
                }
            }
        }
        (count, extra)
    }

    #[inline]
    fn count_diacritics(&self, text: &str) -> usize {
        if !self.has_diacritics() {
            return 0;
        }
        text.chars().filter(|&c| self.is_diacritic(c)).count()
    }

    #[inline(always)]
    fn needs_unigram_cjk(&self) -> bool {
        LANG_TABLE
            .get(self.id().code)
            .map(|e| e.unigram_cjk)
            .unwrap_or(false)
    }

    /// Returns the compile-time segmentation rules for this language.
    /// Empty slice = no special rules (fast path).
    #[inline(always)]
    fn segment_rules(&self) -> &'static [SegmentRule] {
        LANG_TABLE
            .get(self.id().code())
            .map(|e| e.segment_rules)
            .unwrap_or(&[])
    }

    /// Convenience: does this language need word segmentation at all?
    #[inline(always)]
    fn needs_word_segmentation(&self) -> bool {
        self.needs_segmentation() && !self.segment_rules().is_empty()
    }

    /// Determine whether a boundary (space) should be inserted between two characters.
    /// Returns `true` if a segmentation boundary is required, `false` otherwise.
    ///
    /// Rules:
    /// 1. Whitespace never produces boundaries.
    /// 2. Characters in the same script cluster (Western, CJK, Hangul, SE-Asian) do not produce boundaries.
    /// 3. Consecutive CJK ideographs do not produce boundaries.
    /// 4. Western → Script or Script → Western boundaries follow the language's segment rules.
    /// 5. Cross-script transitions produce a boundary.
    #[inline(always)]
    fn needs_boundary_between(&self, prev: char, curr: char) -> bool {
        // --- 1. Whitespace never produces boundaries ---
        if is_any_whitespace(prev) || is_any_whitespace(curr) {
            return false;
        }

        // --- 2. Same script cluster: no boundary ---
        if is_same_script_cluster(prev, curr) {
            return false;
        }

        // --- 3. Cross-cluster transitions ---
        let prev_class = classify(prev);
        let curr_class = classify(curr);

        match (prev_class, curr_class) {
            // Western → Script (CJK/Hangul/SEAsian/NonCJKScript)
            (
                CharClass::Western,
                CharClass::CJK | CharClass::Hangul | CharClass::SEAsian | CharClass::NonCJKScript,
            ) => self.segment_rules().contains(&SegmentRule::WesternToScript),

            // Script → Western
            (
                CharClass::CJK | CharClass::Hangul | CharClass::SEAsian | CharClass::NonCJKScript,
                CharClass::Western,
            ) => self.segment_rules().contains(&SegmentRule::ScriptToWestern),

            // Cross-script (CJK → Hangul/SEAsian/NonCJKScript etc.)
            (pc, cc) if pc != cc => true,

            // Everything else: no boundary
            _ => false,
        }
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

    #[inline(always)]
    fn segment_rules(&self) -> &'static [SegmentRule] {
        LANG_TABLE
            .get(self.code)
            .map(|e| e.segment_rules)
            .unwrap_or(&[])
    }

    #[inline(always)]
    fn needs_word_segmentation(&self) -> bool {
        self.needs_segmentation() && !self.segment_rules().is_empty()
    }

    #[inline(always)]
    fn needs_unigram_cjk(&self) -> bool {
        LANG_TABLE
            .get(self.code)
            .map(|e| e.unigram_cjk)
            .unwrap_or(false)
    }
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
        assert!(entry.has_one_to_one_folds());
        assert!(!TUR.requires_peek_ahead());
        assert!(TUR.needs_case_fold('İ'));
        assert!(TUR.needs_case_fold('I'));
        assert_eq!(TUR.fold_char('İ'), Some('i'));
        assert_eq!(TUR.fold_char('I'), Some('ı'));
    }

    #[test]
    fn test_german_metadata() {
        let entry = LANG_TABLE.get("DEU").unwrap();
        assert!(!entry.has_one_to_one_folds());
        assert!(!DEU.requires_peek_ahead());
        assert!(DEU.needs_case_fold('ß'));
    }

    #[test]
    fn test_dutch_metadata() {
        let entry = LANG_TABLE.get("NLD").unwrap();
        assert!(!entry.has_one_to_one_folds());
        assert!(NLD.requires_peek_ahead());

        // Only uppercase triggers peek-ahead
        assert_eq!(NLD.peek_ahead_fold('I', Some('J')), Some("ij"));
        assert_eq!(NLD.peek_ahead_fold('i', Some('j')), None); // ← FIXED
        assert_eq!(NLD.peek_ahead_fold('I', Some('K')), None);
        assert_eq!(NLD.peek_ahead_fold('I', None), None);
    }

    #[test]
    fn test_english_metadata() {
        let entry = LANG_TABLE.get("ENG").unwrap();
        assert!(entry.has_one_to_one_folds());
        assert!(!ENG.requires_peek_ahead());
        assert!(ENG.needs_case_fold('A'));
        assert_eq!(ENG.fold_char('A'), Some('a'));
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
    }

    #[test]
    fn test_all_languages_have_valid_metadata() {
        let langs = [
            TUR, DEU, NLD, DAN, NOR, SWE, ARA, HEB, VIE, JPN, ZHO, KOR, THA, MYA, KHM, FRA, CAT,
            HUN, POL, CES, SLK, HRV, SRP, UKR, BUL, ENG,
        ];

        for lang in langs {
            let entry = LANG_TABLE.get(lang.code()).expect("Entry exists");

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
        assert_eq!(NLD.peek_ahead_fold('i', Some('j')), None); // ← FIXED
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

    #[test]
    fn test_fold_char_rejects_multi_char() {
        // German: multi-char folds should return None
        assert_eq!(DEU.fold_char('ß'), None, "ß→ss is multi-char");
        assert_eq!(DEU.fold_char('ẞ'), None, "ẞ→ss is multi-char");

        // Dutch: multi-char folds (ligatures) should return None
        assert_eq!(NLD.fold_char('Ĳ'), None, "Ĳ→ij is multi-char");
        assert_eq!(NLD.fold_char('ĳ'), None, "ĳ→ij is multi-char");

        // But regular chars work
        assert_eq!(DEU.fold_char('A'), Some('a'));
        assert_eq!(NLD.fold_char('A'), Some('a'));
    }

    #[test]
    fn test_fold_char_accepts_one_to_one() {
        // Turkish: 1→1 folds should work
        assert_eq!(TUR.fold_char('İ'), Some('i'));
        assert_eq!(TUR.fold_char('I'), Some('ı'));

        // English: Unicode lowercase
        assert_eq!(ENG.fold_char('A'), Some('a'));
        assert_eq!(ENG.fold_char('Z'), Some('z'));
    }

    #[test]
    fn test_lowercase_char_always_one_to_one() {
        // German: lowercase is always 1→1 (ẞ→ß, not →"ss")
        assert_eq!(DEU.lowercase_char('ẞ'), 'ß');
        assert_eq!(DEU.lowercase_char('ß'), 'ß');

        // Turkish
        assert_eq!(TUR.lowercase_char('İ'), 'i');
        assert_eq!(TUR.lowercase_char('I'), 'ı');

        // English
        assert_eq!(ENG.lowercase_char('A'), 'a');
    }

    #[test]
    fn test_fold_vs_lowercase_difference() {
        // German ẞ (capital eszett)
        assert_eq!(DEU.lowercase_char('ẞ'), 'ß', "Lowercase: ẞ→ß");
        assert_eq!(
            DEU.fold_char('ẞ'),
            None,
            "Fold: ẞ→ss (multi-char, rejected)"
        );

        // German ß (lowercase eszett)
        assert_eq!(DEU.lowercase_char('ß'), 'ß', "Already lowercase");
        assert_eq!(
            DEU.fold_char('ß'),
            None,
            "Fold: ß→ss (multi-char, rejected)"
        );

        // This is why German can use CharMapper for Lowercase but not FoldCase
        assert!(!DEU.has_one_to_one_folds());
    }

    #[test]
    fn lowercase_char_is_infallible() {
        assert_eq!(TUR.lowercase_char('İ'), 'i');
        assert_eq!(TUR.lowercase_char('I'), 'ı');
        assert_eq!(ENG.lowercase_char('A'), 'a');
        assert_eq!(DEU.lowercase_char('ẞ'), 'ß');
        assert_eq!(ARA.lowercase_char('ا'), 'ا'); // unchanged
    }
    // Small helper for iterating character pairs
    fn assert_boundaries(lang: &Lang, pairs: &[(&str, &str)], expected: bool) {
        for &(a, b) in pairs {
            let chars: Vec<char> = a.chars().collect();
            let chars2: Vec<char> = b.chars().collect();
            assert_eq!(
                lang.needs_boundary_between(chars[0], chars2[0]),
                expected,
                "Failed: {} -> {} for {}",
                a,
                b,
                std::any::type_name::<Lang>()
            );
        }
    }

    #[test]
    fn test_whitespace_no_boundary() {
        let whitespace_pairs = &[(" ", "あ"), ("あ", " "), ("\n", "A"), ("A", "\t")];
        assert_boundaries(&JPN, whitespace_pairs, false);
    }

    #[test]
    fn test_western_script_breaks() {
        let pairs = &[
            ("A", "あ"),
            ("あ", "A"),
            ("A", "中"),
            ("文", "A"),
            ("A", "\u{AC00}"), // Hangul
            ("\u{AC00}", "A"),
        ];
        assert_boundaries(&JPN, &pairs[0..2], true);
        assert_boundaries(&ZHO, &pairs[2..4], true);
        assert_boundaries(&KOR, &pairs[4..6], true);
    }

    #[test]
    fn test_same_cluster_no_break() {
        let japanese = &[("あ", "ア")];
        let hangul = &[("\u{AC00}", "\u{AC01}")];
        let thai = &[("\u{0E01}", "\u{0E02}")];

        assert_boundaries(&JPN, japanese, false);
        assert_boundaries(&KOR, hangul, false);
        assert_boundaries(&THA, thai, false);
    }

    // #[test]
    // fn test_cjk_unigram() {
    //     let han_pairs = &[("中", "文"), ("日", "本")];
    //     let hangul_pair = &[("\u{AC00}", "\u{AC01}")];

    //     assert_boundaries(&ZHO, &han_pairs[0..1], true);
    //     assert_boundaries(&JPN, &han_pairs[1..2], true);
    //     assert_boundaries(&KOR, hangul_pair, false);
    // }

    #[test]
    fn test_punctuation_and_symbols() {
        let script_to_punct = &[
            ("日", ")"),
            ("文", "."),
            ("\u{0E01}", ","),
            ("\u{AC00}", "-"),
        ];
        let script_to_emoji = &[("あ", "😀"), ("😀", "あ"), ("A", "😃"), ("가", "🎉")];

        assert_boundaries(&JPN, &script_to_punct[0..2], true);
        assert_boundaries(&THA, &script_to_punct[2..3], true);
        assert_boundaries(&KOR, &script_to_punct[3..4], true);

        assert_boundaries(&JPN, &script_to_emoji[0..2], true);
        assert_boundaries(&ZHO, &script_to_emoji[2..3], true);
        assert_boundaries(&KOR, &script_to_emoji[3..4], true);
    }

    #[test]
    fn test_digits_break() {
        let pairs = &[("1", "あ"), ("あ", "1"), ("9", "中"), ("0", "\u{AC00}")];
        assert_boundaries(&JPN, &pairs[0..2], true);
        assert_boundaries(&ZHO, &pairs[2..3], true);
        assert_boundaries(&KOR, &pairs[3..4], true);
    }

    #[test]
    fn test_cross_script_clusters() {
        let pairs = &[
            ("A", "Я"),
            ("Z", "Ж"),
            ("あ", "\u{0E01}"),
            ("文", "\u{AC00}"),
        ];
        assert_boundaries(&JPN, &pairs[0..3], true);
        assert_boundaries(&KOR, &pairs[1..4], true);
    }

    #[test]
    fn test_edge_cjk_blocks() {
        // No break inside CJK blocks
        let no_break = &[("\u{2F00}", "\u{2F01}"), ("\u{2F00}", "\u{2F00}")];
        assert_boundaries(&JPN, no_break, false);

        // Break with CJK punctuation
        let break_pairs = &[("、", "あ"), ("日", "。")];
        assert_boundaries(&JPN, break_pairs, true);
    }

    #[test]
    fn test_western_and_digits() {
        let pairs = &[
            ("A", "B"), // Western → Western
            ("1", "2"), // Digit → Digit
            ("A", "1"), // Letter → Digit
            ("1", "A"), // Digit → Letter
        ];
        assert_boundaries(&JPN, &pairs[0..2], false); // Western→Western and digits: no break
        assert_boundaries(&JPN, &pairs[2..4], false); // Cross Western class: no break
    }

    #[test]
    fn test_ascii_to_cjk_and_back() {
        let pairs = &[
            ("H", "世"), // Western → CJK
            ("o", "世"), // Western → CJK
            ("世", "H"), // CJK → Western
            ("文", "A"), // CJK → Western
        ];
        // Western -> CJK: MUST insert space (true)
        assert_boundaries(&JPN, &pairs[0..2], true);

        // CJK -> Western: MUST insert space (true)
        assert_boundaries(&JPN, &pairs[2..4], true); // <-- FIX: Change false to true
    }

    // #[test]
    // fn test_cjk_and_clusters() {
    //     // Han-Han unigram → break
    //     assert_boundaries(&JPN, &[("日", "本")], true);

    //     // Hangul cluster → no break
    //     assert_boundaries(&KOR, &[("\u{AC00}", "\u{AC01}")], false);

    //     // Thai cluster → no break
    //     assert_boundaries(&THA, &[("\u{0E01}", "\u{0E02}")], false);
    // }
}
