use crate::lang::{Lang, LangEntry, SegmentRule};

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
                "Language **", stringify!($code), "** – ", $name, "\n\n",
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

                    pub static CASE: &[(char, char)] = &[
                        $(($cfrom, $cto)),*
                    ];

                    pub static FOLD: &[(char, &'static str)] = &[
                        $(($ffrom, $fto)),*
                    ];

                    pub static TRANSLITERATE: &[(char, &'static str)] = &[
                        $(($tfrom, $tto)),*
                    ];

                    pub static PRECOMPOSED_TO_BASE: &[(char, char)] = &[
                        $(($sfrom, $sto)),*
                    ];

                    pub static SPACING_DIACRITICS: &[char] = &[$($d),*];

                    pub const NEEDS_WORD_SEGMENTATION: bool = $needs_word_segmentation;
                    pub const REQUIRES_PEEK_AHEAD: bool = $requires_peek_ahead;

                    pub static TRANSLITERATE_CHAR_SLICE: &[char] = &[$($tfrom),*];
                    pub static PRECOMPOSED_TO_BASE_CHAR_SLICE: &[char] = &[$($sfrom),*];

                    pub static PEEK_PAIRS: &[(char, char, &'static str)] = &[
                        $( ($pa, $pb, $pto) ),*
                    ];

                    pub static SEGMENT_RULES: &[SegmentRule] = &[$($sr),*];
                    pub const UNIGRAM_CJK: bool = $unigram;

                    // === Precomputed Boolean Flags ===
                    pub const HAS_CASE_MAP: bool = {
                        let arr: &[(char, char)] = &[$(($cfrom, $cto)),*];
                        !arr.is_empty()
                    };

                    pub const HAS_FOLD_MAP: bool = {
                        let arr: &[(char, &'static str)] = &[$(($ffrom, $fto)),*];
                        !arr.is_empty()
                    };

                    pub const HAS_TRANSLITERATE_MAP: bool = {
                        let arr: &[(char, &'static str)] = &[$(($tfrom, $tto)),*];
                        !arr.is_empty()
                    };

                    pub const HAS_PRECOMPOSED_TO_BASE_MAP: bool = {
                        let arr: &[(char, char)] = &[$(($sfrom, $sto)),*];
                        !arr.is_empty()
                    };

                    pub const HAS_DIACRITICS: bool = {
                        let arr: &[char] = &[$($d),*];
                        !arr.is_empty()
                    };

                    pub const HAS_PEEK_PAIRS: bool = {
                        let arr: &[(char, char, &'static str)] = &[$( ($pa, $pb, $pto) ),*];
                        !arr.is_empty()
                    };

                    pub const HAS_SEGMENT_RULES: bool = {
                        let arr: &[SegmentRule] = &[$($sr),*];
                        !arr.is_empty()
                    };

                    // === Derived Properties ===

                    /// Check if all fold mappings are one-to-one (single char output)
                    pub const HAS_ONE_TO_ONE_FOLDS: bool = {
                        let arr: &[(char, &'static str)] = &[$(($ffrom, $fto)),*];
                        if arr.is_empty() {
                            true
                        } else {
                            let mut all_one_to_one = true;
                            let mut i = 0;
                            while i < arr.len() {
                                let to_str = arr[i].1;
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
                        let arr: &[(char, &'static str)] = &[$(($tfrom, $tto)),*];
                        if arr.is_empty() {
                            true
                        } else {
                            let mut all_one_to_one = true;
                            let mut i = 0;
                            while i < arr.len() {
                                let to_str = arr[i].1;
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
                        spacing_diacritics: [<$code:lower _data>]::SPACING_DIACRITICS,
                        transliterate_char_slice: [<$code:lower _data>]::TRANSLITERATE_CHAR_SLICE,
                        pre_composed_to_base_char_slice: [<$code:lower _data>]::PRECOMPOSED_TO_BASE_CHAR_SLICE,
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

        /// All supported languages – for testing and introspection
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
        case: [ 'İ' => 'i','I' => 'ı' ],
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
        requires_peek_ahead: false,
        peek_pairs: [],
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

    // ⚡ OPTIMIZED: Frequency-ordered (Fatha most common)
    ARA, "ARA", "Arabic",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [
            '\u{064E}', // Fatha (~35%)
            '\u{0650}', // Kasra (~25%)
            '\u{064F}', // Damma (~20%)
            '\u{0652}', // Sukun (~10%)
            '\u{0651}', // Shadda (~5%) — added for full tashkil removal
            '\u{064B}', // Tanwin Fath (~3%)
            '\u{064D}', // Tanwin Kasr (~2%)
            '\u{064C}', // Tanwin Damm (~2%)
            '\u{0653}', // Maddah
            '\u{0654}', // Hamza above
            '\u{0655}', // Hamza below
            '\u{0656}', // Subscript Alef
            '\u{0657}', // Inverted Damma
            '\u{0658}', // Mark Noon Ghunna
            '\u{0670}'  // Superscript Alef
        ],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    // ⚡ OPTIMIZED: Frequency-ordered (Shva most common)
    HEB, "HEB", "Hebrew",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [
            '\u{05B0}', // Shva (~25% frequency)
            '\u{05B7}', // Patah (~18%)
            '\u{05B8}', // Kamatz (~15%)
            '\u{05B4}', // Hiriq (~12%)
            '\u{05B6}', // Segol (~10%)
            '\u{05B5}', // Tzere (~8%)
            '\u{05B9}', // Holam (~6%)
            '\u{05BB}', // Kubutz (~4%)
            '\u{05BC}', // Dagesh (~2%)
            '\u{05C1}', // Shin dot
            '\u{05C2}', // Sin dot
            '\u{05BD}', // Meteg
            '\u{05BF}', // Rafe
            '\u{05B1}', // Hataf Segol
            '\u{05B2}', // Hataf Patah
            '\u{05B3}', // Hataf Kamatz
            '\u{05BA}', // Holam Haser
            '\u{05C4}', // Upper dot
            '\u{05C5}', // Lower dot
            '\u{05C7}'  // Kamatz Katan
        ],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    // ⚡ OPTIMIZED: Frequency-ordered by vowel family (a > o > e > u > i > y)
    // Lowercase first (95%+ of text), uppercase last
    VIE, "VIE", "Vietnamese",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [
            // A family (most frequent vowel, ~30%)
            'a' => 'a', 'à' => 'a', 'á' => 'a', 'ả' => 'a', 'ã' => 'a', 'ạ' => 'a',
            'ă' => 'a', 'ằ' => 'a', 'ắ' => 'a', 'ẳ' => 'a', 'ẵ' => 'a', 'ặ' => 'a',
            'â' => 'a', 'ầ' => 'a', 'ấ' => 'a', 'ẩ' => 'a', 'ẫ' => 'a', 'ậ' => 'a',

            // O family (~20%)
            'o' => 'o', 'ò' => 'o', 'ó' => 'o', 'ỏ' => 'o', 'õ' => 'o', 'ọ' => 'o',
            'ô' => 'o', 'ồ' => 'o', 'ố' => 'o', 'ổ' => 'o', 'ỗ' => 'o', 'ộ' => 'o',
            'ơ' => 'o', 'ờ' => 'o', 'ớ' => 'o', 'ở' => 'o', 'ỡ' => 'o', 'ợ' => 'o',

            // E family (~15%)
            'e' => 'e', 'è' => 'e', 'é' => 'e', 'ẻ' => 'e', 'ẽ' => 'e', 'ẹ' => 'e',
            'ê' => 'e', 'ề' => 'e', 'ế' => 'e', 'ể' => 'e', 'ễ' => 'e', 'ệ' => 'e',

            // U family (~12%)
            'u' => 'u', 'ù' => 'u', 'ú' => 'u', 'ủ' => 'u', 'ũ' => 'u', 'ụ' => 'u',
            'ư' => 'u', 'ừ' => 'u', 'ứ' => 'u', 'ử' => 'u', 'ữ' => 'u', 'ự' => 'u',

            // I family (~10%)
            'i' => 'i', 'ì' => 'i', 'í' => 'i', 'ỉ' => 'i', 'ĩ' => 'i', 'ị' => 'i',

            // Y family (~8%)
            'y' => 'y', 'ỳ' => 'y', 'ý' => 'y', 'ỷ' => 'y', 'ỹ' => 'y', 'ỵ' => 'y',

            // Đ (~5%)
            'đ' => 'd',

            // UPPERCASE (< 5% combined)
            'A' => 'A', 'À' => 'A', 'Á' => 'A', 'Ả' => 'A', 'Ã' => 'A', 'Ạ' => 'A',
            'Ă' => 'A', 'Ằ' => 'A', 'Ắ' => 'A', 'Ẳ' => 'A', 'Ẵ' => 'A', 'Ặ' => 'A',
            'Â' => 'A', 'Ầ' => 'A', 'Ấ' => 'A', 'Ẩ' => 'A', 'Ẫ' => 'A', 'Ậ' => 'A',
            'O' => 'O', 'Ò' => 'O', 'Ó' => 'O', 'Ỏ' => 'O', 'Õ' => 'O', 'Ọ' => 'O',
            'Ô' => 'O', 'Ồ' => 'O', 'Ố' => 'O', 'Ổ' => 'O', 'Ỗ' => 'O', 'Ộ' => 'O',
            'Ơ' => 'O', 'Ờ' => 'O', 'Ớ' => 'O', 'Ở' => 'O', 'Ỡ' => 'O', 'Ợ' => 'O',
            'E' => 'E', 'È' => 'E', 'É' => 'E', 'Ẻ' => 'E', 'Ẽ' => 'E', 'Ẹ' => 'E',
            'Ê' => 'E', 'Ề' => 'E', 'Ế' => 'E', 'Ể' => 'E', 'Ễ' => 'E', 'Ệ' => 'E',
            'U' => 'U', 'Ù' => 'U', 'Ú' => 'U', 'Ủ' => 'U', 'Ũ' => 'U', 'Ụ' => 'U',
            'Ư' => 'U', 'Ừ' => 'U', 'Ứ' => 'U', 'Ử' => 'U', 'Ữ' => 'U', 'Ự' => 'U',
            'I' => 'I', 'Ì' => 'I', 'Í' => 'I', 'Ỉ' => 'I', 'Ĩ' => 'I', 'Ị' => 'I',
            'Y' => 'Y', 'Ỳ' => 'Y', 'Ý' => 'Y', 'Ỷ' => 'Y', 'Ỹ' => 'Y', 'Ỵ' => 'Y',
            'Đ' => 'D'
        ],
        spacing_diacritics: ['\u{0300}', '\u{0301}', '\u{0303}', '\u{0309}', '\u{0323}'],
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
            // ONLY true diacritics – háček (caron) and kroužek (ring)
            'Č' => 'C', 'č' => 'c',
            'Ď' => 'D', 'ď' => 'd',
            'Ě' => 'E', 'ě' => 'e',
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
            'Ĺ' => 'L', 'ĺ' => 'l',
            'Ŕ' => 'R', 'ŕ' => 'r',
            'Ô' => 'O', 'ô' => 'o',
        ],
        spacing_diacritics: [],
        needs_word_segmentation: false,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    // ⚡ OPTIMIZED: Frequency-ordered (ą/Ą most common)
    POL, "POL", "Polish",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [
            'ą' => 'a', 'Ą' => 'A', // Most frequent
            'ę' => 'e', 'Ę' => 'E',
            'ó' => 'o', 'Ó' => 'O',
            'ż' => 'z', 'Ż' => 'Z',
            'ź' => 'z', 'Ź' => 'Z',
            'ś' => 's', 'Ś' => 'S',
            'ć' => 'c', 'Ć' => 'C',
            'ń' => 'n', 'Ń' => 'N',
            'ł' => 'l', 'Ł' => 'L'  // Least frequent
        ],
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

    // No case_map needed: Unicode lowercase correctly handles final sigma (Σ → ς word-finally)
    // No peek-ahead required: pure 1:1 mapping via char::to_lowercase()
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
        spacing_diacritics: [ '\u{0BCD}' ],
        needs_word_segmentation: true,
        requires_peek_ahead: false,
        peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    // ⚡ OPTIMIZED: Frequency-ordered (о most common at 10.97%)
    // Lowercase first (90%+ of text), uppercase paired immediately after
    RUS, "RUS", "Russian",
        case: [],
        fold: [],
        transliterate: [
            'о' => "o", 'О' => "O",  // 10.97%
            'е' => "e", 'Е' => "E",  // 8.48%
            'а' => "a", 'А' => "A",  // 7.99%
            'и' => "i", 'И' => "I",  // 7.36%
            'н' => "n", 'Н' => "N",  // 6.70%
            'т' => "t", 'Т' => "T",  // 6.32%
            'с' => "s", 'С' => "S",  // 5.47%
            'р' => "r", 'Р' => "R",  // 4.73%
            'в' => "v", 'В' => "V",  // 4.54%
            'л' => "l", 'Л' => "L",  // 4.34%
            'к' => "k", 'К' => "K",  // 3.49%
            'м' => "m", 'М' => "M",  // 3.21%
            'д' => "d", 'Д' => "D",  // 2.98%
            'п' => "p", 'П' => "P",  // 2.81%
            'у' => "u", 'У' => "U",  // 2.62%
            'я' => "ja", 'Я' => "Ja", // 2.01%
            'ы' => "y", 'Ы' => "Y",  // 1.90%
            'ь' => "ʹ", 'Ь' => "ʹ",  // 1.74%
            'г' => "g", 'Г' => "G",  // 1.70%
            'з' => "z", 'З' => "Z",  // 1.65%
            'б' => "b", 'Б' => "B",  // 1.59%
            'ч' => "č", 'Ч' => "Č",  // 1.44%
            'й' => "j", 'Й' => "J",  // 1.21%
            'х' => "h", 'Х' => "H",  // 0.97%
            'ж' => "ž", 'Ж' => "Ž",  // 0.94%
            'ш' => "š", 'Ш' => "Š",  // 0.73%
            'ю' => "ju", 'Ю' => "Ju", // 0.64%
            'ц' => "c", 'Ц' => "C",  // 0.48%
            'щ' => "šč", 'Щ' => "Šč", // 0.36%
            'э' => "è", 'Э' => "È",  // 0.32%
            'ф' => "f", 'Ф' => "F",  // 0.26%
            'ъ' => "ʺ", 'Ъ' => "ʺ",  // 0.04%
            'ё' => "ë", 'Ё' => "Ë"   // 0.04%
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

    // ⚡ OPTIMIZED: Frequency-ordered (vowel signs first, tone marks last)
    THA, "THA", "Thai",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [
            '\u{0E34}', '\u{0E35}', // I, II (most common)
            '\u{0E38}', '\u{0E39}', // U, UU
            '\u{0E31}', // Mai Han-Akat
            '\u{0E36}', '\u{0E37}', // UE, UEE
            '\u{0E48}', '\u{0E49}', // Tone marks (mai ek, tho)
            '\u{0E4A}', '\u{0E4B}', // Tone marks (tri, chattawa)
            '\u{0E47}', '\u{0E4C}', // Mai Taikhu, Thanthakhat
            '\u{0E3A}', '\u{0E4D}', '\u{0E4E}' // Rare marks
        ],
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

    // ⚡ OPTIMIZED: Frequency-ordered (vowel signs most common)
    KHM, "KHM", "Khmer",
        case: [],
        fold: [],
        transliterate: [],
        precomposed_to_base: [],
        spacing_diacritics: [
            '\u{17B6}', // AA (most common vowel sign)
            '\u{17C1}', // E
            '\u{17B8}', // II
            '\u{17BB}', '\u{17BC}', // U, UU
            '\u{17C4}', // OO
            '\u{17B7}', '\u{17B9}', '\u{17BA}', // Other vowels
            '\u{17BD}', '\u{17BE}', '\u{17BF}', '\u{17C0}',
            '\u{17C2}', '\u{17C3}', '\u{17C5}',
            '\u{17D2}', // Coeng (medium frequency)
            '\u{17C6}', // Nikahit
            '\u{17C7}', '\u{17C8}', '\u{17C9}', '\u{17CA}', // Tone marks (less common)
            '\u{17CB}', '\u{17CC}', '\u{17CD}', '\u{17CE}',
            '\u{17CF}', '\u{17D0}', '\u{17D1}', '\u{17D3}',
            '\u{17DD}' // Rare
        ],
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
