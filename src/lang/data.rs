use crate::lang::{CaseMap, FoldMap, Lang, LangEntry, PeekPair, SegmentRule, StripMap};

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
        strip: [ $($sfrom:expr => $sto:expr),* $(,)? ],
        diac: [ $($d:expr),* $(,)? ],
        segment: $segment:expr,
        peek_ahead: $peek:expr,
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
                "- **Strip precomposed:** [", stringify!($($sfrom => $sto),*), "]\n",
                "- **Spacing diacritics:** [", stringify!($($d),*), "]\n",
                "- **Peek pairs:** [", stringify!($( ($pa, $pb => $pto) ),*), "]\n",
                "- **Segment rules:** [", stringify!($($sr),*), "]\n",
                "- **Peek ahead:** `", stringify!($peek), "`\n",
                "- **CJK unigram:** `", stringify!($unigram), "`\n",
            )]
            pub const $code: Lang = Lang { code: $code_str, name: $name };
        )*
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

                    pub static TRANSLITERATE: &[FoldMap] = &[
                        $(FoldMap { from: $tfrom, to: $tto }),*
                    ];

                    pub static STRIP: &[StripMap] = &[
                        $(StripMap { from: $sfrom, to: $sto }),*
                    ];

                    pub static DIAC: &[char] = &[$($d),*];

                    pub const NEEDS_SEGMENTATION: bool = $segment;
                    pub const REQUIRES_PEEK_AHEAD: bool = $peek;

                    pub static FOLD_CHAR_SLICE: &[char] = &[$($ffrom),*];
                    pub static TRANSLITERATE_CHAR_SLICE: &[char] = &[$($tfrom),*];
                    pub static STRIP_CHAR_SLICE: &[char] = &[$($sfrom),*];
                    pub static DIACRITIC_SLICE: &[char] = &[$($d),*];

                    pub static PEEK_PAIRS: &[PeekPair] = &[
                        $( PeekPair { a: $pa, b: $pb, to: $pto } ),*
                    ];

                    pub static SEGMENT_RULES: &[SegmentRule] = &[$($sr),*];
                    pub const UNIGRAM_CJK: bool = $unigram;
                }
            }
        )*
        paste! {
            pub(crate) static LANG_TABLE: Map<&'static str, LangEntry> = phf_map! {
                $(
                    $code_str => LangEntry {
                        case_map: [<$code:lower _data>]::CASE,
                        fold_map: [<$code:lower _data>]::FOLD,
                        transliterate_map: [<$code:lower _data>]::TRANSLITERATE,
                        strip_map: [<$code:lower _data>]::STRIP,
                        diacritics: if [<$code:lower _data>]::DIAC.is_empty() {
                            None
                        } else {
                            Some([<$code:lower _data>]::DIAC)
                        },
                        needs_segmentation: [<$code:lower _data>]::NEEDS_SEGMENTATION,
                        requires_peek_ahead: [<$code:lower _data>]::REQUIRES_PEEK_AHEAD,
                        fold_char_slice: [<$code:lower _data>]::FOLD_CHAR_SLICE,
                        transliterate_char_slice: [<$code:lower _data>]::TRANSLITERATE_CHAR_SLICE,
                        strip_char_slice: [<$code:lower _data>]::STRIP_CHAR_SLICE,
                        diacritic_slice: if [<$code:lower _data>]::DIACRITIC_SLICE.is_empty() {
                            None
                        } else {
                            Some([<$code:lower _data>]::DIACRITIC_SLICE)
                        },
                        peek_pairs: [<$code:lower _data>]::PEEK_PAIRS,
                        segment_rules: [<$code:lower _data>]::SEGMENT_RULES,
                        unigram_cjk: [<$code:lower _data>]::UNIGRAM_CJK,
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
    };
}

define_languages! {
    TUR, "TUR", "Turkish",
        case: [ 'I' => 'ı', 'İ' => 'i' ],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    DEU, "DEU", "German",
        case: [],
        fold: [ 'ß' => "ss", 'ẞ' => "ss" ],
        transliterate: [ 'Ä' => "ae", 'ä' => "ae", 'Ö' => "oe", 'ö' => "oe", 'Ü' => "ue", 'ü' => "ue" ],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    NLD, "NLD", "Dutch",
        case: [],
        fold: [ 'Ĳ' => "ij", 'ĳ' => "ij" ],
        transliterate: [],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: true,
        peek_pairs: [ ('I', 'J' => "ij"), ('I', 'j' => "ij") ],
        segment_rules: [],
        unigram_cjk: false,

    DAN, "DAN", "Danish",
        case: [],
        fold: [],
        transliterate: [ 'Å' => "aa", 'å' => "aa", 'Æ' => "ae", 'æ' => "ae", 'Ø' => "oe", 'ø' => "oe" ],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    NOR, "NOR", "Norwegian",
        case: [],
        fold: [],
        transliterate: [ 'Æ' => "ae", 'æ' => "ae", 'Ø' => "oe", 'ø' => "oe", 'Å' => "aa", 'å' => "aa" ],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    SWE, "SWE", "Swedish",
        case: [],
        fold: [],
        transliterate: [ 'Å' => "aa", 'å' => "aa", 'Ä' => "ae", 'ä' => "ae", 'Ö' => "oe", 'ö' => "oe" ],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    FIN, "FIN", "Finnish",
        case: [],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    ISL, "ISL", "Icelandic",
        case: [],
        fold: [],
        transliterate: [ 'Þ' => "th", 'þ' => "th", 'Ð' => "d", 'ð' => "d", 'Æ' => "ae", 'æ' => "ae" ],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    ARA, "ARA", "Arabic",
        case: [],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [
            '\u{064B}', '\u{064C}', '\u{064D}', '\u{064E}', '\u{064F}',
            '\u{0650}', '\u{0651}', '\u{0652}', '\u{0653}', '\u{0654}',
            '\u{0655}', '\u{0656}', '\u{0657}', '\u{0658}', '\u{0670}'
        ],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    HEB, "HEB", "Hebrew",
        case: [],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [
            '\u{05B0}', '\u{05B1}', '\u{05B2}', '\u{05B3}', '\u{05B4}',
            '\u{05B5}', '\u{05B6}', '\u{05B7}', '\u{05B8}', '\u{05B9}',
            '\u{05BA}', '\u{05BB}', '\u{05BC}', '\u{05BD}', '\u{05BF}',
            '\u{05C1}', '\u{05C2}', '\u{05C4}', '\u{05C5}', '\u{05C7}'
        ],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    VIE, "VIE", "Vietnamese",
        case: [],
        fold: [],
        transliterate: [],
        strip: [
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
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    FRA, "FRA", "French",
        case: [
            'Œ' => 'œ',
            'Æ' => 'æ',
        ],
        fold: [],
        transliterate: [
            'Œ' => "oe", 'œ' => "oe",
            'Æ' => "ae", 'æ' => "ae",
            'Ç' => "c", 'ç' => "c",  // Historical ASCII (postal/telegraph) — Rule 3
        ],
        strip: [
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
            // Excluded: Ç/ç (real letter), Œ/œ, Æ/æ (ligatures) — Rule 4
        ],
        diac: [],  // No true spacing marks — Rule 5
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    CES, "CES", "Czech",
        case: [],
        fold: [],
        transliterate: [],
        strip: [ 'Á' => 'A', 'á' => 'a', 'Č' => 'C', 'č' => 'c', 'Ď' => 'D', 'ď' => 'd', 'É' => 'E', 'é' => 'e', 'Ě' => 'E', 'ě' => 'e', 'Í' => 'I', 'í' => 'i', 'Ň' => 'N', 'ň' => 'n', 'Ó' => 'O', 'ó' => 'o', 'Ř' => 'R', 'ř' => 'r', 'Š' => 'S', 'š' => 's', 'Ť' => 'T', 'ť' => 't', 'Ú' => 'U', 'ú' => 'u', 'Ů' => 'U', 'ů' => 'u', 'Ý' => 'Y', 'ý' => 'y', 'Ž' => 'Z', 'ž' => 'z' ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    SLK, "SLK", "Slovak",
        case: [],
        fold: [],
        transliterate: [],
        strip: [ 'Á' => 'A', 'á' => 'a', 'Ä' => 'A', 'ä' => 'a', 'Č' => 'C', 'č' => 'c', 'Ď' => 'D', 'ď' => 'd', 'É' => 'E', 'é' => 'e', 'Í' => 'I', 'í' => 'i', 'Ĺ' => 'L', 'ĺ' => 'l', 'Ľ' => 'L', 'ľ' => 'l', 'Ň' => 'N', 'ň' => 'n', 'Ó' => 'O', 'ó' => 'o', 'Ô' => 'O', 'ô' => 'o', 'Ŕ' => 'R', 'ŕ' => 'r', 'Š' => 'S', 'š' => 's', 'Ť' => 'T', 'ť' => 't', 'Ú' => 'U', 'ú' => 'u', 'Ý' => 'Y', 'ý' => 'y', 'Ž' => 'Z', 'ž' => 'z' ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    POL, "POL", "Polish",
        case: [],
        fold: [],
        transliterate: [],
        strip: [ 'Ą' => 'A', 'ą' => 'a', 'Ć' => 'C', 'ć' => 'c', 'Ę' => 'E', 'ę' => 'e', 'Ł' => 'L', 'ł' => 'l', 'Ń' => 'N', 'ń' => 'n', 'Ó' => 'O', 'ó' => 'o', 'Ś' => 'S', 'ś' => 's', 'Ź' => 'Z', 'ź' => 'z', 'Ż' => 'Z', 'ż' => 'z' ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    // [('L', '·' => "l·l"),('l', '·' => "l·l")],
    // Catalan l·l → L·L is a case mapping rule, not a fold, The rule says: "Catalan preserves middle dot contextually" → this refers to case mapping, not search folding.
    // This behavior should be handled in a dedicated CaseMap stage, not in fold logic.
    CAT, "CAT", "Catalan",
        case: ['Ç' => 'ç', 'Ï' => 'ï'],
        fold: [],
        transliterate: [ 'Ç' => "c", 'ç' => "c",],
        strip: [
            'À' => 'A', 'à' => 'a', 'Á' => 'A', 'á' => 'a',
            'È' => 'E', 'è' => 'e', 'É' => 'E', 'é' => 'e',
            'Í' => 'I', 'í' => 'i', 'Ï' => 'I', 'ï' => 'i',
            // 'Ò' => 'O', 'ò' => 'o', 'Ó' => 'O', 'ó' => 'o',
            'Ú' => 'U', 'ú' => 'u', 'Ü' => 'U', 'ü' => 'u',
        ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    SPA, "SPA", "Spanish",
        case: [],
        fold: [],
        transliterate: [],
        strip: [ 'Á' => 'A', 'á' => 'a', 'É' => 'E', 'é' => 'e', 'Í' => 'I', 'í' => 'i', 'Ó' => 'O', 'ó' => 'o', 'Ú' => 'U', 'ú' => 'u', 'Ü' => 'U', 'ü' => 'u'],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    POR, "POR", "Portuguese",
        case: [],
        fold: [],
        transliterate: [],
        strip: [ 'À' => 'A', 'à' => 'a', 'Á' => 'A', 'á' => 'a', 'Â' => 'A', 'â' => 'a', 'Ã' => 'A', 'ã' => 'a', 'Ç' => 'C', 'ç' => 'c', 'É' => 'E', 'é' => 'e', 'Ê' => 'E', 'ê' => 'e', 'Í' => 'I', 'í' => 'i', 'Ó' => 'O', 'ó' => 'o', 'Ô' => 'O', 'ô' => 'o', 'Õ' => 'O', 'õ' => 'o', 'Ú' => 'U', 'ú' => 'u', 'Ü' => 'U', 'ü' => 'u' ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    ITA, "ITA", "Italian",
        case: [],
        fold: [],
        transliterate: [],
        strip: [ 'À' => 'A', 'à' => 'a', 'È' => 'E', 'è' => 'e', 'É' => 'E', 'é' => 'e', 'Ì' => 'I', 'ì' => 'i', 'Ò' => 'O', 'ò' => 'o', 'Ù' => 'U', 'ù' => 'u' ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    RON, "RON", "Romanian",
        case: [],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    HUN, "HUN", "Hungarian",
        case: [],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    HRV, "HRV", "Croatian",
        case: [],
        fold: [],
        transliterate: [],
        strip: [ 'Č' => 'C', 'č' => 'c', 'Ć' => 'C', 'ć' => 'c', 'Đ' => 'D', 'đ' => 'd', 'Š' => 'S', 'š' => 's', 'Ž' => 'Z', 'ž' => 'z', 'ǈ' => 'L', 'ǉ' => 'l', 'ǋ' => 'N', 'ǌ' => 'n' ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    SRP, "SRP", "Serbian",
        case: [],
        fold: [],
        transliterate: [],
        strip: [ 'Ђ' => 'D', 'ђ' => 'd', 'Ј' => 'J', 'ј' => 'j', 'Љ' => 'L', 'љ' => 'l', 'Њ' => 'N', 'њ' => 'n', 'Ћ' => 'C', 'ћ' => 'c', 'Џ' => 'D', 'џ' => 'd', 'Ž' => 'Z', 'ž' => 'z' ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    UKR, "UKR", "Ukrainian",
        case: [],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    BUL, "BUL", "Bulgarian",
        case: [],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    LIT, "LIT", "Lithuanian",
        case: [ 'Ė' => 'ė', 'Į' => 'į', 'Ų' => 'ų' ],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    LAV, "LAV", "Latvian",
        case: [],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    EST, "EST", "Estonian",
        case: [],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    ELL, "ELL", "Greek",
        case: [],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [ '\u{0301}', '\u{0308}', '\u{0342}', '\u{0313}', '\u{0314}', '\u{0345}' ],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

    HIN, "HIN", "Hindi",
        case: [],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [ '\u{093C}', '\u{0901}', '\u{0902}', '\u{0903}', '\u{094D}' ],
        segment: true,
        peek_ahead: false,
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
        strip: [],
        diac: [ '\u{09BC}', '\u{0981}', '\u{0982}', '\u{0983}', '\u{09CD}' ],
        segment: true,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [
            SegmentRule::WesternToScript,
            SegmentRule::ScriptToWestern,
        ],
        unigram_cjk: false,

    JPN, "JPN", "Japanese",
        case: [],
        fold: [],
        transliterate: [],
        strip: [],
        diac: [],
        segment: true,
        peek_ahead: false,
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
        strip: [],
        diac: [],
        segment: true,
        peek_ahead: false,
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
        strip: [],
        diac: [],
        segment: true,
        peek_ahead: false,
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
        strip: [],
        diac: [ '\u{0E31}', '\u{0E34}', '\u{0E35}', '\u{0E36}', '\u{0E37}', '\u{0E38}', '\u{0E39}', '\u{0E3A}', '\u{0E47}', '\u{0E48}', '\u{0E49}', '\u{0E4A}', '\u{0E4B}', '\u{0E4C}', '\u{0E4D}', '\u{0E4E}' ],
        segment: true,
        peek_ahead: false,
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
        strip: [],
        diac: [ '\u{0EB1}', '\u{0EB4}', '\u{0EB5}', '\u{0EB6}', '\u{0EB7}', '\u{0EB8}', '\u{0EB9}', '\u{0EBB}', '\u{0EBC}', '\u{0EC8}', '\u{0EC9}', '\u{0ECA}', '\u{0ECB}', '\u{0ECC}', '\u{0ECD}' ],
        segment: true,
        peek_ahead: false,
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
        strip: [],
        diac: [ '\u{102B}', '\u{102C}', '\u{102D}', '\u{102E}', '\u{102F}', '\u{1030}', '\u{1031}', '\u{1032}', '\u{1036}', '\u{1037}', '\u{1038}', '\u{1039}', '\u{103A}', '\u{103B}', '\u{103C}', '\u{103D}', '\u{103E}' ],
        segment: true,
        peek_ahead: false,
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
        strip: [],
        diac: [ '\u{17B6}', '\u{17B7}', '\u{17B8}', '\u{17B9}', '\u{17BA}', '\u{17BB}', '\u{17BC}', '\u{17BD}', '\u{17BE}', '\u{17BF}', '\u{17C0}', '\u{17C1}', '\u{17C2}', '\u{17C3}', '\u{17C4}', '\u{17C5}', '\u{17C6}', '\u{17C7}', '\u{17C8}', '\u{17C9}', '\u{17CA}', '\u{17CB}', '\u{17CC}', '\u{17CD}', '\u{17CE}', '\u{17CF}', '\u{17D0}', '\u{17D1}', '\u{17D2}', '\u{17D3}', '\u{17DD}' ],
        segment: true,
        peek_ahead: false,
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
        strip: [],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false
}

#[cfg(test)]
mod tests {
    use crate::{
        LANG_TABLE,
        lang::{
            LangEntry,
            data::{
                ARA, BUL, CAT, CES, DAN, DEU, ENG, FRA, HEB, HRV, HUN, JPN, KHM, KOR, MYA, NLD,
                NOR, POL, SLK, SRP, SWE, THA, TUR, UKR, VIE, ZHO, from_code,
            },
        },
    };

    fn get_from_table(code: &str) -> &'static LangEntry {
        LANG_TABLE.get(code).unwrap()
    }

    #[test]
    fn test_turkish_metadata() {
        let entry = get_from_table("TUR");
        assert!(entry.has_one_to_one_folds());
        assert!(!entry.requires_peek_ahead());
        assert!(entry.needs_case_fold('İ'));
        assert!(entry.needs_case_fold('I'));
        assert_eq!(entry.fold_char('İ'), Some('i'));
        assert_eq!(entry.fold_char('I'), Some('ı'));
    }

    #[test]
    fn test_german_metadata() {
        let entry = get_from_table("DEU");
        assert!(!entry.has_one_to_one_folds());
        assert!(!entry.requires_peek_ahead());
        assert!(entry.needs_case_fold('ß'));
    }

    #[test]
    fn test_dutch_metadata() {
        let entry = get_from_table("NLD");
        assert!(!entry.has_one_to_one_folds());
        assert!(entry.requires_peek_ahead());

        // Only uppercase triggers peek-ahead
        assert_eq!(entry.peek_ahead_fold('I', Some('J')), Some("ij"));
        assert_eq!(entry.peek_ahead_fold('i', Some('j')), None); // ← FIXED
        assert_eq!(entry.peek_ahead_fold('I', Some('K')), None);
        assert_eq!(entry.peek_ahead_fold('I', None), None);
    }

    #[test]
    fn test_english_metadata() {
        let entry = get_from_table("ENG");
        assert!(entry.has_one_to_one_folds());
        assert!(!entry.requires_peek_ahead());
        assert!(entry.needs_case_fold('A'));
        assert_eq!(entry.fold_char('A'), Some('a'));
    }

    #[test]
    fn test_arabic_diacritics() {
        let entry = get_from_table("ARA");
        assert!(entry.has_diacritics());
        assert!(entry.is_diacritic('َ'));
        assert!(!entry.is_diacritic('ا'));
        // assert!(entry.contains_diacritics("مَرْحَبًا"));
        // assert!(!entry.contains_diacritics("مرحبا"));
    }

    #[test]
    fn test_from_code() {
        assert_eq!(from_code("TUR"), Some(TUR));
        assert_eq!(from_code("tur"), Some(TUR));
        assert_eq!(from_code("ENG"), Some(ENG));
        assert_eq!(from_code("XXX"), None);
    }

    // #[test]
    // fn test_needs_trim() {
    //     assert!(entry.needs_trim(" hello"));
    //     assert!(entry.needs_trim("hello "));
    //     assert!(entry.needs_trim(" hello "));
    //     assert!(!entry.needs_trim("hello"));
    // }

    // #[test]
    // fn test_count_foldable_chars() {
    //     assert_eq!(entry.count_foldable_chars("HELLO"), 5);
    //     assert_eq!(entry.count_foldable_chars("hello"), 0);
    //     assert_eq!(entry.count_foldable_chars("HeLLo"), 3);
    //     assert_eq!(get_from_table("TUR").count_foldable_chars("İSTANBUL"), 8);
    // }

    // #[test]
    // fn test_count_diacritics() {
    //     assert_eq!(entry.count_diacritics("مَرْحَبًا"), 4);
    //     assert_eq!(entry.count_diacritics("مرحبا"), 0);
    //     assert_eq!(entry.count_diacritics("hello"), 0);
    // }

    #[test]
    fn test_fold_char_preserves_grapheme_count_in_one_to_one_cases() {
        let cases = [
            ("ABCabc", "ENG"), // ASCII: byte == char
            ("éÉèÈ", "FRA"),   // Latin-1: 2-byte chars, but 1:1 mapping
            ("İIıi", "TUR"),   // Turkish: should preserve grapheme count
        ];

        for (text, code) in cases {
            let entry = get_from_table(code);
            let folded: String = text.chars().filter_map(|c| entry.fold_char(c)).collect();

            assert_eq!(
                text.chars().count(),
                folded.chars().count(),
                "Grapheme count changed for {} in lang {}",
                text,
                code
            );
        }
    }

    #[test]
    fn test_all_languages_have_valid_metadata() {
        let langs = [
            TUR, DEU, NLD, DAN, NOR, SWE, ARA, HEB, VIE, JPN, ZHO, KOR, THA, MYA, KHM, FRA, CAT,
            HUN, POL, CES, SLK, HRV, SRP, UKR, BUL, ENG,
        ];

        for lang in langs {
            let entry = get_from_table(lang.code());

            if entry.requires_peek_ahead {
                assert!(
                    !entry.peek_pairs.is_empty(),
                    "peek_ahead=true but peek_pairs empty for {:?}",
                    lang.name
                );
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
                assert!(entry.has_diacritics());
            }
        }
    }

    #[test]
    fn test_segmentation_languages() {
        assert!(get_from_table("JPN").needs_segmentation());
        assert!(get_from_table("ZHO").needs_segmentation());
        assert!(get_from_table("KOR").needs_segmentation());
        assert!(get_from_table("THA").needs_segmentation());
        assert!(!get_from_table("ENG").needs_segmentation());
        assert!(!get_from_table("TUR").needs_segmentation());
    }

    #[test]
    fn test_case_map_only_turkish() {
        assert!(!get_from_table("TUR").case_map().is_empty());
        assert!(get_from_table("ENG").case_map().is_empty());
        assert!(get_from_table("DEU").case_map().is_empty());
        assert!(get_from_table("ARA").case_map().is_empty());
    }

    #[test]
    fn test_idempotency_metadata() {
        let langs = [
            TUR, DEU, NLD, DAN, NOR, SWE, FRA, HUN, POL, CES, SLK, HRV, SRP, UKR, BUL,
        ];

        for lang in langs {
            for fold in get_from_table(lang.code()).fold_map() {
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
        assert_eq!(
            get_from_table("NLD").peek_ahead_fold('I', Some('J')),
            Some("ij")
        );
        assert_eq!(get_from_table("NLD").peek_ahead_fold('i', Some('j')), None); // ← FIXED
        assert_eq!(get_from_table("NLD").peek_ahead_fold('I', Some('K')), None);
        assert_eq!(get_from_table("NLD").peek_ahead_fold('I', None), None);
        assert_eq!(get_from_table("ENG").peek_ahead_fold('I', Some('J')), None);
        assert_eq!(get_from_table("TUR").peek_ahead_fold('I', Some('J')), None);
    }

    #[test]
    fn test_peek_ahead_fold_is_generalized() {
        assert_eq!(
            get_from_table("NLD").peek_ahead_fold('I', Some('J')),
            Some("ij")
        );
        assert_eq!(get_from_table("NLD").peek_ahead_fold('A', Some('B')), None);
    }

    #[test]
    fn test_performance_o1_lookup() {
        let text = "AAAAAAAAAA";
        let count = text
            .chars()
            .filter(|&c| get_from_table("ENG").needs_case_fold(c))
            .count();
        assert_eq!(count, 10);

        let turkish_text = "İİİİİİİİİİ";
        let count = turkish_text
            .chars()
            .filter(|&c| get_from_table("TUR").needs_case_fold(c))
            .count();
        assert_eq!(count, 10);
    }

    #[test]
    fn test_fold_char_rejects_multi_char() {
        // German: multi-char folds should return None
        assert_eq!(
            get_from_table("DEU").fold_char('ß'),
            None,
            "ß→ss is multi-char"
        );
        assert_eq!(
            get_from_table("DEU").fold_char('ẞ'),
            None,
            "ẞ→ss is multi-char"
        );

        // Dutch: multi-char folds (ligatures) should return None
        assert_eq!(
            get_from_table("NLD").fold_char('Ĳ'),
            None,
            "Ĳ→ij is multi-char"
        );
        assert_eq!(
            get_from_table("NLD").fold_char('ĳ'),
            None,
            "ĳ→ij is multi-char"
        );

        // But regular chars work
        assert_eq!(get_from_table("DEU").fold_char('A'), Some('a'));
        assert_eq!(get_from_table("NLD").fold_char('A'), Some('a'));
    }

    #[test]
    fn test_fold_char_accepts_one_to_one() {
        // Turkish: 1→1 folds should work
        assert_eq!(get_from_table("TUR").fold_char('İ'), Some('i'));
        assert_eq!(get_from_table("TUR").fold_char('I'), Some('ı'));

        // English: Unicode lowercase
        assert_eq!(get_from_table("ENG").fold_char('A'), Some('a'));
        assert_eq!(get_from_table("ENG").fold_char('Z'), Some('z'));
    }

    #[test]
    fn test_lowercase_char_always_one_to_one() {
        // German: lowercase is always 1→1 (ẞ→ß, not →"ss")
        assert_eq!(get_from_table("DEU").lowercase_char('ẞ'), 'ß');
        assert_eq!(get_from_table("DEU").lowercase_char('ß'), 'ß');

        // Turkish
        assert_eq!(get_from_table("TUR").lowercase_char('İ'), 'i');
        assert_eq!(get_from_table("TUR").lowercase_char('I'), 'ı');

        // English
        assert_eq!(get_from_table("ENG").lowercase_char('A'), 'a');
    }

    #[test]
    fn test_fold_vs_lowercase_difference() {
        // German ẞ (capital eszett)
        assert_eq!(
            get_from_table("DEU").lowercase_char('ẞ'),
            'ß',
            "Lowercase: ẞ→ß"
        );
        assert_eq!(
            get_from_table("DEU").fold_char('ẞ'),
            None,
            "Fold: ẞ→ss (multi-char, rejected)"
        );

        // German ß (lowercase eszett)
        assert_eq!(
            get_from_table("DEU").lowercase_char('ß'),
            'ß',
            "Already lowercase"
        );
        assert_eq!(
            get_from_table("DEU").fold_char('ß'),
            None,
            "Fold: ß→ss (multi-char, rejected)"
        );

        // This is why German can use CharMapper for Lowercase but not FoldCase
        assert!(!get_from_table("DEU").has_one_to_one_folds());
    }

    #[test]
    fn lowercase_char_is_infallible() {
        assert_eq!(get_from_table("TUR").lowercase_char('İ'), 'i');
        assert_eq!(get_from_table("TUR").lowercase_char('I'), 'ı');
        assert_eq!(get_from_table("ENG").lowercase_char('A'), 'a');
        assert_eq!(get_from_table("DEU").lowercase_char('ẞ'), 'ß');
        assert_eq!(get_from_table("ARA").lowercase_char('ا'), 'ا');
    }
}
