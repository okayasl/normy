use crate::lang::{CaseMap, FoldMap, Lang, LangEntry, PeekPair, SegmentRule};

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
        diac: [ $($d:expr),* $(,)? ],
        segment: $segment:expr,
        peek_ahead: $peek:expr,
        peek_pairs: [ $( ($pa:expr, $pb:expr => $pto:expr) ),* $(,)? ],
        segment_rules: [ $($sr:expr),* $(,)? ],
        unigram_cjk: $unigram:expr
    ),* $(,)?) => {
        // Public `Lang` constants
        $(
            pub const $code: Lang = Lang { code: $code_str, name: $name };
        )*

        //Per-language static data modules
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

        // Global lookup table (public)
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
        // Helper: `Lang::from_code`
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
//    Language definitions (single source of truth)
//    Note: peek_pairs provided only where needed (Dutch IJ as example)
// ---------------------------------------------------------------------------
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
define_languages! {
    TUR,  "TUR", "Turkish",
        case: [ 'I' => 'ı', 'İ' => 'i' ],
        fold: [ 'I' => "ı", 'İ' => "i" ],
        diac: [],
        segment: false,
        peek_ahead: false,
        peek_pairs: [],
        segment_rules: [],
        unigram_cjk: false,

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

#[cfg(test)]
mod tests {
    use crate::lang::{
        LangEntry,
        data::{
            ARA, BUL, CAT, CES, DAN, DEU, ENG, FRA, HEB, HRV, HUN, JPN, KHM, KOR, LANG_TABLE, MYA,
            NLD, NOR, POL, SLK, SRP, SWE, THA, TUR, UKR, VIE, ZHO, from_code,
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
    fn test_byte_vs_char_length() {
        let entry = get_from_table("TUR");
        let mapping = entry.fold_map();
        let i_mapping = mapping.iter().find(|m| m.from == 'I').unwrap();
        assert_eq!(i_mapping.to.chars().count(), 1);
        assert!(entry.has_one_to_one_folds());
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
