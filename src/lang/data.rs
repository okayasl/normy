use crate::lang::{CaseMap, FoldMap, Lang, LangEntry, PeekPair, PreComposedToBaseMap, SegmentRule};

use paste::paste;
use phf::{Map, phf_map};

/// ---------------------------------------------------------------------------
///    Macro – generates everything from a single table
/// ---------------------------------------------------------------------------
macro_rules! define_languages {
    ($(
        $code:ident, $code_str:literal, $name:literal,
        case: [ $($cfrom:expr => $cto:expr),* $(,)? ],
        fold: [ $($ffrom:expr => $fto:expr),* $(,)? ],
        transliterate: [ $($tfrom:expr => $tto:expr),* $(,)? ],
        precomposed_to_base: [ $($sfrom:expr => $sto:expr),* $(,)? ],
        spacing_diacritics: [ $($d:expr),* $(,)? ],
        needs_word_segmentation: $needs_word_segmentation:expr,
        requires_peek_ahead: $requires_peek_ahead:expr,
        peek_pairs: [ $( ($pa:expr, $pb:expr => $pto:expr) ),* $(,)? ],
        segment_rules: [ $($sr:expr),* $(,)? ],
        unigram_cjk: $unigram:expr
    ),* $(,)?) => {
        $(
            #[doc = concat!(
                "Language **", stringify!($code), "** — ", $name, "\n\n",
                "- **Case map:** [", stringify!($($cfrom => $cto),*), "]\n",
                "- **Fold map:** [", stringify!($($ffrom => $fto),*), "]\n",
                "- **Transliterate:** [", stringify!($($tfrom => $tto),*), "]\n",
                "- **Precomposed to base:** [", stringify!($($sfrom => $sto),*), "]\n",
                "- **Spacing diacritics:** [", stringify!($($d),*), "]\n",
                "- **Needs word segmentation:** ", stringify!($needs_word_segmentation), "\n",
                "- **Segment rules:** [", stringify!($($sr),*), "]\n",
                "- **Requires peek-ahead:** ", stringify!($requires_peek_ahead), "\n",
                "- **Peek pairs:** [", stringify!($(($pa, $pb => $pto)),*), "]\n",
                "- **CJK unigram tokens:** ", stringify!($unigram), "\n",
            )]
            pub const $code: Lang = Lang { code: $code_str, name: $name };
        )*

        $(
            paste! {
                mod [<$code:lower _data>] {
                    use super::*;

                    pub static CODE: &str = $code_str;

                    pub static CASE: &[CaseMap] = &[
                        $(CaseMap { from: $cfrom, to: $cto }),*
                    ];

                    pub static FOLD: &[FoldMap] = &[
                        $(FoldMap { from: $ffrom, to: $fto }),*
                    ];

                    pub static TRANSLITERATE: &[FoldMap] = &[
                        $(FoldMap { from: $tfrom, to: $tto }),*
                    ];

                    pub static PRECOMPOSED_TO_BASE: &[PreComposedToBaseMap] = &[
                        $(PreComposedToBaseMap { from: $sfrom, to: $sto }),*
                    ];

                    pub static SPACING_DIACRITICS: &[char] = &[$($d),*];

                    pub const NEEDS_WORD_SEGMENTATION: bool = $needs_word_segmentation;
                    pub const REQUIRES_PEEK_AHEAD: bool = $requires_peek_ahead;

                    pub static CASE_CHAR_SLICE: &[char] = &[$($cfrom),*];          // ← NEW
                    pub static FOLD_CHAR_SLICE: &[char] = &[$($ffrom),*];
                    pub static TRANSLITERATE_CHAR_SLICE: &[char] = &[$($tfrom),*];
                    pub static PRECOMPOSED_TO_BASE_CHAR_SLICE: &[char] = &[$($sfrom),*];
                    pub static SPACING_DIACRITICS_SLICE: &[char] = &[$($d),*];

                    pub static PEEK_PAIRS: &[PeekPair] = &[
                        $( PeekPair { a: $pa, b: $pb, to: $pto } ),*
                    ];

                    pub static SEGMENT_RULES: &[SegmentRule] = &[$($sr),*];
                    pub const UNIGRAM_CJK: bool = $unigram;

                    // === Precomputed Boolean Flags ===
                    pub const HAS_CASE_MAP: bool = {
                        let arr: &[CaseMap] = &[$(CaseMap { from: $cfrom, to: $cto }),*];
                        !arr.is_empty()
                    };

                    pub const HAS_FOLD_MAP: bool = {
                        let arr: &[FoldMap] = &[$(FoldMap { from: $ffrom, to: $fto }),*];
                        !arr.is_empty()
                    };

                    pub const HAS_TRANSLITERATE_MAP: bool = {
                        let arr: &[FoldMap] = &[$(FoldMap { from: $tfrom, to: $tto }),*];
                        !arr.is_empty()
                    };

                    pub const HAS_PRECOMPOSED_TO_BASE_MAP: bool = {
                        let arr: &[PreComposedToBaseMap] = &[$(PreComposedToBaseMap { from: $sfrom, to: $sto }),*];
                        !arr.is_empty()
                    };

                    pub const HAS_DIACRITICS: bool = {
                        let arr: &[char] = &[$($d),*];
                        !arr.is_empty()
                    };

                    pub const HAS_PEEK_PAIRS: bool = {
                        let arr: &[PeekPair] = &[$( PeekPair { a: $pa, b: $pb, to: $pto } ),*];
                        !arr.is_empty()
                    };

                    pub const HAS_SEGMENT_RULES: bool = {
                        let arr: &[SegmentRule] = &[$($sr),*];
                        !arr.is_empty()
                    };

                    // === Derived Properties ===

                    /// Check if all fold mappings are one-to-one (single char output)
                    pub const HAS_ONE_TO_ONE_FOLDS: bool = {
                        let arr: &[FoldMap] = &[$(FoldMap { from: $ffrom, to: $fto }),*];
                        if arr.is_empty() {
                            true
                        } else {
                            let mut all_one_to_one = true;
                            let mut i = 0;
                            while i < arr.len() {
                                let to_str = arr[i].to;
                                // Count UTF-8 characters in to_str
                                let mut char_count = 0;
                                let bytes = to_str.as_bytes();
                                let mut byte_idx = 0;
                                while byte_idx < bytes.len() {
                                    let b = bytes[byte_idx];
                                    // UTF-8 leading byte detection
                                    if b & 0x80 == 0 {
                                        byte_idx += 1;
                                    } else if b & 0xE0 == 0xC0 {
                                        byte_idx += 2;
                                    } else if b & 0xF0 == 0xE0 {
                                        byte_idx += 3;
                                    } else {
                                        byte_idx += 4;
                                    }
                                    char_count += 1;
                                }
                                if char_count != 1 {
                                    all_one_to_one = false;
                                    break;
                                }
                                i += 1;
                            }
                            all_one_to_one
                        }
                    };

                    /// Check if all transliterate mappings are one-to-one
                    pub const HAS_ONE_TO_ONE_TRANSLITERATE: bool = {
                        let arr: &[FoldMap] = &[$(FoldMap { from: $tfrom, to: $tto }),*];
                        if arr.is_empty() {
                            true
                        } else {
                            let mut all_one_to_one = true;
                            let mut i = 0;
                            while i < arr.len() {
                                let to_str = arr[i].to;
                                let mut char_count = 0;
                                let bytes = to_str.as_bytes();
                                let mut byte_idx = 0;
                                while byte_idx < bytes.len() {
                                    let b = bytes[byte_idx];
                                    if b & 0x80 == 0 {
                                        byte_idx += 1;
                                    } else if b & 0xE0 == 0xC0 {
                                        byte_idx += 2;
                                    } else if b & 0xF0 == 0xE0 {
                                        byte_idx += 3;
                                    } else {
                                        byte_idx += 4;
                                    }
                                    char_count += 1;
                                }
                                if char_count != 1 {
                                    all_one_to_one = false;
                                    break;
                                }
                                i += 1;
                            }
                            all_one_to_one
                        }
                    };
                }
            }
        )*

        paste! {
            pub(crate) static LANG_TABLE: Map<&'static str, LangEntry> = phf_map! {
                $(
                    $code_str => LangEntry {
                        // === Precomputed Flags (Hot Path) ===
                        has_case_map: [<$code:lower _data>]::HAS_CASE_MAP,
                        has_fold_map: [<$code:lower _data>]::HAS_FOLD_MAP,
                        has_transliterate_map: [<$code:lower _data>]::HAS_TRANSLITERATE_MAP,
                        has_pre_composed_to_base_map: [<$code:lower _data>]::HAS_PRECOMPOSED_TO_BASE_MAP,
                        has_spacing_diacritics: [<$code:lower _data>]::HAS_DIACRITICS,
                        has_peek_pairs: [<$code:lower _data>]::HAS_PEEK_PAIRS,
                        has_segment_rules: [<$code:lower _data>]::HAS_SEGMENT_RULES,
                        has_one_to_one_folds: [<$code:lower _data>]::HAS_ONE_TO_ONE_FOLDS,
                        has_one_to_one_transliterate: [<$code:lower _data>]::HAS_ONE_TO_ONE_TRANSLITERATE,
                        needs_segmentation: [<$code:lower _data>]::NEEDS_WORD_SEGMENTATION,
                        requires_peek_ahead: [<$code:lower _data>]::REQUIRES_PEEK_AHEAD,
                        unigram_cjk: [<$code:lower _data>]::UNIGRAM_CJK,

                        // === Data Arrays ===
                        code: [<$code:lower _data>]::CODE,
                        case_map: [<$code:lower _data>]::CASE,
                        fold_map: [<$code:lower _data>]::FOLD,
                        transliterate_map: [<$code:lower _data>]::TRANSLITERATE,
                        pre_composed_to_base_map: [<$code:lower _data>]::PRECOMPOSED_TO_BASE,
                        spacing_diacritics: if [<$code:lower _data>]::SPACING_DIACRITICS.is_empty() {
                            None
                        } else {
                            Some([<$code:lower _data>]::SPACING_DIACRITICS)
                        },
                        case_char_slice: [<$code:lower _data>]::CASE_CHAR_SLICE,
                        fold_char_slice: [<$code:lower _data>]::FOLD_CHAR_SLICE,
                        transliterate_char_slice: [<$code:lower _data>]::TRANSLITERATE_CHAR_SLICE,
                        pre_composed_to_base_char_slice: [<$code:lower _data>]::PRECOMPOSED_TO_BASE_CHAR_SLICE,
                        spacing_diacritics_slice: if [<$code:lower _data>]::SPACING_DIACRITICS_SLICE.is_empty() {
                            None
                        } else {
                            Some([<$code:lower _data>]::SPACING_DIACRITICS_SLICE)
                        },
                        peek_pairs: [<$code:lower _data>]::PEEK_PAIRS,
                        segment_rules: [<$code:lower _data>]::SEGMENT_RULES,
                    }
                ),*
            };
        }

        pub fn from_code(code: &str) -> Option<Lang> {
            let upper = code.to_uppercase();
            match upper.as_str() {
                $(
                    $code_str => Some($code),
                )*
                _ => None,
            }
        }

        /// All supported languages — for testing and introspection
        pub const fn all_langs() -> &'static [Lang] {
            &[
                $(
                    $code
                ),*
            ]
        }
    };
}

define_languages! {
    TUR, "TUR", "Turkish",
        case: [ 'I' => 'ı', 'İ' => 'i' ],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    DEU, "DEU", "German",
        case: [],
        fold: [ 'ß' => "ss", 'ẞ' => "ss" ],
        transliterate: [ 'Ä' => "ae", 'ä' => "ae", 'Ö' => "oe", 'ö' => "oe", 'Ü' => "ue", 'ü' => "ue" ],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    NLD, "NLD", "Dutch",
        case: [],
        fold: [ 'Ĳ' => "ij", 'ĳ' => "ij" ],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: true,
        peek_pairs: [ ('I', 'J' => "ij"), ('I', 'j' => "ij") ],
        segment_rules: [],
        unigram_cjk: false,

    DAN, "DAN", "Danish",
        case: [],
        fold: [],
        transliterate: [ 'Å' => "aa", 'å' => "aa", 'Æ' => "ae", 'æ' => "ae", 'Ø' => "oe", 'ø' => "oe" ],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    NOR, "NOR", "Norwegian",
        case: [],
        fold: [],
        transliterate: [ 'Æ' => "ae", 'æ' => "ae", 'Ø' => "oe", 'ø' => "oe", 'Å' => "aa", 'å' => "aa" ],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    SWE, "SWE", "Swedish",
        case: [],
        fold: [],
        transliterate: [ 'Å' => "aa", 'å' => "aa", 'Ä' => "ae", 'ä' => "ae", 'Ö' => "oe", 'ö' => "oe" ],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    ISL, "ISL", "Icelandic",
        case: [],
        fold: [],
        transliterate: [ 'Þ' => "th", 'þ' => "th", 'Ð' => "d", 'ð' => "d", 'Æ' => "ae", 'æ' => "ae" ],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    ARA, "ARA", "Arabic",
        case: [],
        fold: [],  // NO alif folding - explicitly rejected in validation
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [ // U+0651 SHADDA REMOVED - phonemically significant
            '\u{064B}', '\u{064C}', '\u{064D}', '\u{064E}', '\u{064F}',
            '\u{0650}', '\u{0652}', '\u{0653}', '\u{0654}',
            '\u{0655}', '\u{0656}', '\u{0657}', '\u{0658}', '\u{0670}'
        ],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    HEB, "HEB", "Hebrew",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [
            '\u{05B0}', '\u{05B1}', '\u{05B2}', '\u{05B3}', '\u{05B4}',
            '\u{05B5}', '\u{05B6}', '\u{05B7}', '\u{05B8}', '\u{05B9}',
            '\u{05BA}', '\u{05BB}', '\u{05BC}', '\u{05BD}', '\u{05BF}',
            '\u{05C1}', '\u{05C2}', '\u{05C4}', '\u{05C5}', '\u{05C7}'
        ],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    VIE, "VIE", "Vietnamese",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [
            'À' => 'A', 'à' => 'a', 'Á' => 'A', 'á' => 'a', 'Ả' => 'A', 'ả' => 'a', 'Ã' => 'A', 'ã' => 'a', 'Ạ' => 'A', 'ạ' => 'a',
            'Ă' => 'A', 'ă' => 'a', 'Ằ' => 'A', 'ằ' => 'a', 'Ắ' => 'A', 'ắ' => 'a', 'Ẳ' => 'A', 'ẳ' => 'a', 'Ẵ' => 'A', 'ẵ' => 'a', 'Ặ' => 'A', 'ặ' => 'a',
            'Â' => 'A', 'â' => 'a', 'Ầ' => 'A', 'ầ' => 'a', 'Ấ' => 'A', 'ấ' => 'a', 'Ẩ' => 'A', 'ẩ' => 'a', 'Ẫ' => 'A', 'ẫ' => 'a', 'Ậ' => 'A', 'ậ' => 'a',
            'È' => 'E', 'è' => 'e', 'É' => 'E', 'é' => 'e', 'Ẻ' => 'E', 'ẻ' => 'e', 'Ẽ' => 'E', 'ẽ' => 'e', 'Ẹ' => 'E', 'ẹ' => 'e',
            'Ê' => 'E', 'ê' => 'e', 'Ề' => 'E', 'ề' => 'e', 'Ế' => 'E', 'ế' => 'e', 'Ể' => 'E', 'ể' => 'e', 'Ễ' => 'E', 'ễ' => 'e', 'Ệ' => 'E', 'ệ' => 'e',
            'Ì' => 'I', 'ì' => 'i', 'Í' => 'I', 'í' => 'i', 'Ỉ' => 'I', 'ỉ' => 'i', 'Ĩ' => 'I', 'ĩ' => 'i', 'Ị' => 'I', 'ị' => 'i',
            'Ò' => 'O', 'ò' => 'o', 'Ó' => 'O', 'ó' => 'o', 'Ỏ' => 'O', 'ỏ' => 'o', 'Õ' => 'O', 'õ' => 'o', 'Ọ' => 'O', 'ọ' => 'o',
            'Ô' => 'O', 'ô' => 'o', 'Ồ' => 'O', 'ồ' => 'o', 'Ố' => 'O', 'ố' => 'o', 'Ổ' => 'O', 'ổ' => 'o', 'Ỗ' => 'O', 'ỗ' => 'o', 'Ộ' => 'O', 'ộ' => 'o',
            'Ơ' => 'O', 'ơ' => 'o', 'Ờ' => 'O', 'ờ' => 'o', 'Ớ' => 'O', 'ớ' => 'o', 'Ở' => 'O', 'ở' => 'o', 'Ỡ' => 'O', 'ỡ' => 'o', 'Ợ' => 'O', 'ợ' => 'o',
            'Ù' => 'U', 'ù' => 'u', 'Ú' => 'U', 'ú' => 'u', 'Ủ' => 'U', 'ủ' => 'u', 'Ũ' => 'U', 'ũ' => 'u', 'Ụ' => 'U', 'ụ' => 'u',
            'Ư' => 'U', 'ư' => 'u', 'Ừ' => 'U', 'ừ' => 'u', 'Ứ' => 'U', 'ứ' => 'u', 'Ử' => 'U', 'ử' => 'u', 'Ữ' => 'U', 'ữ' => 'u', 'Ự' => 'U', 'ự' => 'u',
            'Ỳ' => 'Y', 'ỳ' => 'y', 'Ý' => 'Y', 'ý' => 'y', 'Ỷ' => 'Y', 'ỷ' => 'y', 'Ỹ' => 'Y', 'ỹ' => 'y', 'Ỵ' => 'Y', 'ỵ' => 'y',
            'Đ' => 'D', 'đ' => 'd'
        ],
        spacing_diacritics: ['\u{0300}', '\u{0301}', '\u{0303}', '\u{0309}', '\u{0323}'],  // ADDED: Five Vietnamese tone marks. Policy exception: Vietnamese tone marks are stripped despite NFC precomposed forms
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    FRA, "FRA", "French",
        case: [],
        fold: [],
        transliterate: [
            'Œ' => "oe", 'œ' => "oe",
            'Æ' => "ae", 'æ' => "ae",
            'Ç' => "c", 'ç' => "c",
        ],
        precomposed_to_base: [
            'À' => 'A', 'à' => 'a',
            'Â' => 'A', 'â' => 'a',
            'È' => 'E', 'è' => 'e',
            'É' => 'E', 'é' => 'e',
            'Ê' => 'E', 'ê' => 'e',
            'Ë' => 'E', 'ë' => 'e',
            'Î' => 'I', 'î' => 'i',
            'Ï' => 'I', 'ï' => 'i',
            'Ô' => 'O', 'ô' => 'o',
            'Ù' => 'U', 'ù' => 'u',
            'Û' => 'U', 'û' => 'u',
            'Ü' => 'U', 'ü' => 'u',
            'Ÿ' => 'Y', 'ÿ' => 'y',
        ],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    CES, "CES", "Czech",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [
            // ONLY true diacritics — háček (caron) and kroužek (ring)
            'Č' => 'C', 'č' => 'c',
            'Ď' => 'D', 'ď' => 'd',
            'Ě' => 'E', 'ě' => 'e',   // ě is special: háček on e → strip
            'Ň' => 'N', 'ň' => 'n',
            'Ř' => 'R', 'ř' => 'r',
            'Š' => 'S', 'š' => 's',
            'Ť' => 'T', 'ť' => 't',
            'Ž' => 'Z', 'ž' => 'z',
            'Ů' => 'U', 'ů' => 'u',   // ring u → strip (conventional in search)

            // CRITICAL: All acute-accented vowels (Á, É, Í, Ó, Ú, Ý) are EXCLUDED.
            // á é í ó ú ý  — these are native, phonemically distinct graphemes
            // Stripping them would be lossy and break zero-copy on native text
        ],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    SLK, "SLK", "Slovak",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [
            // ONLY true orthographic diacritics — carons (mäkčeň)
            'Č' => 'C', 'č' => 'c',
            'Ď' => 'D', 'ď' => 'd',
            'Ľ' => 'L', 'ľ' => 'l',
            'Ň' => 'N', 'ň' => 'n',
            'Š' => 'S', 'š' => 's',
            'Ť' => 'T', 'ť' => 't',
            'Ž' => 'Z', 'ž' => 'z',

            // Syllabic consonants with acute — these ARE diacritics
            'Ĺ' => 'L', 'ĺ' => 'l',
            'Ŕ' => 'R', 'ŕ' => 'r',

            // ô is a diphthong marker (vokáň) — conventionally stripped
            'Ô' => 'O', 'ô' => 'o',

            // ä is NOT a diacritic — it is a separate vowel phoneme /æ/
            // → MUST NOT be stripped → REMOVE from table
            // 'Ä' => 'A', 'ä' => 'a',   ← DELETE THESE LINES

            // All acute long vowels á é í ó ú ý are phonemically distinct
            // → MUST NOT be stripped → REMOVE from table
            // 'Á' => 'A', 'á' => 'a',   ← DELETE
            // 'É' => 'E', 'é' => 'e',   ← DELETE
            // etc.
        ],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    POL, "POL", "Polish",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [ 'Ą' => 'A', 'ą' => 'a', 'Ć' => 'C', 'ć' => 'c', 'Ę' => 'E', 'ę' => 'e', 'Ł' => 'L', 'ł' => 'l', 'Ń' => 'N', 'ń' => 'n', 'Ó' => 'O', 'ó' => 'o', 'Ś' => 'S', 'ś' => 's', 'Ź' => 'Z', 'ź' => 'z', 'Ż' => 'Z', 'ż' => 'z' ],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    // [('L', '·' => "l·l"),('l', '·' => "l·l")],
    // Catalan l·l → L·L is a case mapping rule, not a fold, The rule says: "Catalan preserves middle dot contextually" → this refers to case mapping, not search folding.
    // This behavior should be handled in a dedicated CaseMap stage, not in fold logic.
    CAT, "CAT", "Catalan",
        case: [],
        fold: [],
        transliterate: [ 'Ç' => "c", 'ç' => "c",],
        precomposed_to_base: [
            'À' => 'A', 'à' => 'a', 'Á' => 'A', 'á' => 'a',
            'È' => 'E', 'è' => 'e', 'É' => 'E', 'é' => 'e',
            'Í' => 'I', 'í' => 'i', 'Ï' => 'I', 'ï' => 'i',
            'Ú' => 'U', 'ú' => 'u', 'Ü' => 'U', 'ü' => 'u',
        ],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    SPA, "SPA", "Spanish",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [ 'Á' => 'A', 'á' => 'a', 'É' => 'E', 'é' => 'e', 'Í' => 'I', 'í' => 'i', 'Ó' => 'O', 'ó' => 'o', 'Ú' => 'U', 'ú' => 'u', 'Ü' => 'U', 'ü' => 'u'],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    POR, "POR", "Portuguese",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [ 'À' => 'A', 'à' => 'a', 'Á' => 'A', 'á' => 'a', 'Â' => 'A', 'â' => 'a', 'Ã' => 'A', 'ã' => 'a', 'Ç' => 'C', 'ç' => 'c', 'É' => 'E', 'é' => 'e', 'Ê' => 'E', 'ê' => 'e', 'Í' => 'I', 'í' => 'i', 'Ó' => 'O', 'ó' => 'o', 'Ô' => 'O', 'ô' => 'o', 'Õ' => 'O', 'õ' => 'o', 'Ú' => 'U', 'ú' => 'u', 'Ü' => 'U', 'ü' => 'u' ],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    ITA, "ITA", "Italian",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [ 'À' => 'A', 'à' => 'a', 'È' => 'E', 'è' => 'e', 'É' => 'E', 'é' => 'e', 'Ì' => 'I', 'ì' => 'i', 'Ò' => 'O', 'ò' => 'o', 'Ù' => 'U', 'ù' => 'u' ],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    HRV, "HRV", "Croatian",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [ 'Č' => 'C', 'č' => 'c', 'Ć' => 'C', 'ć' => 'c', 'Đ' => 'D', 'đ' => 'd', 'Š' => 'S', 'š' => 's', 'Ž' => 'Z', 'ž' => 'z', 'ǈ' => 'L', 'ǉ' => 'l', 'ǋ' => 'N', 'ǌ' => 'n' ],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    SRP, "SRP", "Serbian",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [ 'Ђ' => 'D', 'ђ' => 'd', 'Ј' => 'J', 'ј' => 'j', 'Љ' => 'L', 'љ' => 'l', 'Њ' => 'N', 'њ' => 'n', 'Ћ' => 'C', 'ћ' => 'c', 'Џ' => 'D', 'џ' => 'd', 'Ž' => 'Z', 'ž' => 'z' ],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    LIT, "LIT", "Lithuanian",
        case: [ 'Ė' => 'ė', 'Į' => 'į', 'Ų' => 'ų' ],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    // Greek (ELL) Addition: Added Greek to requires_peek_ahead to mandate
    // the necessary lookahead for handling the contextual final sigma (σ/ς).[1]
    // Final sigma: Lowercase Σ → ς if no next char (word-end); else σ
    // In map impl: if current=='Σ' && next.is_none() { Some("ς") } else { Some('σ') }
    // But for PeekPair: Use a sentinel for EOF, or extend LangEntry to handle None explicitly.
    // Quick fix: Add pair for 'Σ' with b='\0' (null char) as EOF sentinel → "ς"
    // Better: In lang.rs PeekPair, add variant for EOF, but to keep current design:
    // Handle in case_fold.rs: If requires_peek_ahead && next.is_none() && current=='Σ' { return Some('ς') }
    // For now, populate minimally to trigger logic:
    //    ('Σ', '\0' => "ς"),  // \0 as EOF sentinel; filter in impl
    ELL, "ELL", "Greek",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [ '\u{0301}', '\u{0308}', '\u{0342}', '\u{0313}', '\u{0314}', '\u{0345}' ],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    HIN, "HIN", "Hindi",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [ '\u{093C}', '\u{0901}', '\u{0902}', '\u{0903}', '\u{094D}' ],
        needs_word_segmentation: true,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    BEN, "BEN", "Bengali",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [ '\u{09BC}', '\u{0981}', '\u{0982}', '\u{0983}', '\u{09CD}' ],
        needs_word_segmentation: true,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    TAM, "TAM", "Tamil",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [ '\u{0BCD}' ],  // Virama/puḷḷi mark
        needs_word_segmentation: true,  // NEW: Agglutinative language requires morphological segmentation
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    RUS, "RUS", "Russian",
        case: [],
        fold: [],
        transliterate: [
            'А' => "A", 'а' => "a",
            'Б' => "B", 'б' => "b",
            'В' => "V", 'в' => "v",
            'Г' => "G", 'г' => "g",
            'Д' => "D", 'д' => "d",
            'Е' => "E", 'е' => "e",
            'Ё' => "Ë", 'ё' => "ë",
            'Ж' => "Ž", 'ж' => "ž",
            'З' => "Z", 'з' => "z",
            'И' => "I", 'и' => "i",
            'Й' => "J", 'й' => "j",
            'К' => "K", 'к' => "k",
            'Л' => "L", 'л' => "l",
            'М' => "M", 'м' => "m",
            'Н' => "N", 'н' => "n",
            'О' => "O", 'о' => "o",
            'П' => "P", 'п' => "p",
            'Р' => "R", 'р' => "r",
            'С' => "S", 'с' => "s",
            'Т' => "T", 'т' => "t",
            'У' => "U", 'у' => "u",
            'Ф' => "F", 'ф' => "f",
            'Х' => "H", 'х' => "h",
            'Ц' => "C", 'ц' => "c",
            'Ч' => "Č", 'ч' => "č",
            'Ш' => "Š", 'ш' => "š",
            'Щ' => "Šč", 'щ' => "šč",  // ISO/R 9:1968 specific
            'Ъ' => "ʺ", 'ъ' => "ʺ",    // U+02BA Modifier Letter Double Prime
            'Ы' => "Y", 'ы' => "y",
            'Ь' => "ʹ", 'ь' => "ʹ",    // U+02B9 Modifier Letter Prime
            'Э' => "È", 'э' => "è",
            'Ю' => "Ju", 'ю' => "ju",  // ISO/R 9:1968 specific
            'Я' => "Ja", 'я' => "ja",  // ISO/R 9:1968 specific
        ],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    JPN, "JPN", "Japanese",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: true,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    ZHO, "ZHO", "Chinese (Simplified)",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: true,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::CJKIdeographUnigram,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: true,

    KOR, "KOR", "Korean",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: true,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    THA, "THA", "Thai",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [ '\u{0E31}', '\u{0E34}', '\u{0E35}', '\u{0E36}', '\u{0E37}', '\u{0E38}', '\u{0E39}', '\u{0E3A}', '\u{0E47}', '\u{0E48}', '\u{0E49}', '\u{0E4A}', '\u{0E4B}', '\u{0E4C}', '\u{0E4D}', '\u{0E4E}' ],
        needs_word_segmentation: true,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    LAO, "LAO", "Lao",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [ '\u{0EB1}', '\u{0EB4}', '\u{0EB5}', '\u{0EB6}', '\u{0EB7}', '\u{0EB8}', '\u{0EB9}', '\u{0EBB}', '\u{0EBC}', '\u{0EC8}', '\u{0EC9}', '\u{0ECA}', '\u{0ECB}', '\u{0ECC}', '\u{0ECD}' ],
        needs_word_segmentation: true,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    MYA, "MYA", "Myanmar",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [ '\u{102B}', '\u{102C}', '\u{102D}', '\u{102E}', '\u{102F}', '\u{1030}', '\u{1031}', '\u{1032}', '\u{1036}', '\u{1037}', '\u{1038}', '\u{1039}', '\u{103A}', '\u{103B}', '\u{103C}', '\u{103D}', '\u{103E}' ],
        needs_word_segmentation: true,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    KHM, "KHM", "Khmer",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [ '\u{17B6}', '\u{17B7}', '\u{17B8}', '\u{17B9}', '\u{17BA}', '\u{17BB}', '\u{17BC}', '\u{17BD}', '\u{17BE}', '\u{17BF}', '\u{17C0}', '\u{17C1}', '\u{17C2}', '\u{17C3}', '\u{17C4}', '\u{17C5}', '\u{17C6}', '\u{17C7}', '\u{17C8}', '\u{17C9}', '\u{17CA}', '\u{17CB}', '\u{17CC}', '\u{17CD}', '\u{17CE}', '\u{17CF}', '\u{17D0}', '\u{17D1}', '\u{17D2}', '\u{17D3}', '\u{17DD}' ],
        needs_word_segmentation: true,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    ENG, "ENG", "English",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false
}
