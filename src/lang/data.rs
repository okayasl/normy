use paste::paste;
use phf::{Map, phf_map};

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