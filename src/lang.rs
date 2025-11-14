//! # Normy Language Layer — FULL, ZERO-COST, MACRO-DRIVEN (2025)
//! * **No enum, no match, no duplication**
//! * **All behavior in `define_lang!` — including `needs_segmentation`**
//! * **phf perfect-hash** — O(1), compile-time
//! * **Single source of truth** — change only in define_lang!

#![allow(dead_code)]

use phf::{Map, phf_map};

/// ---------------------------------------------------------------------------
/// 1. Static Language Identifier
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

/// Default: English
pub const DEFAULT_LANG: Lang = Lang {
    code: "ENG",
    name: "English",
};

/// ---------------------------------------------------------------------------
/// 2. Data Types
/// ---------------------------------------------------------------------------
#[derive(Clone, Copy)]
pub struct CaseMap {
    pub from: char,
    pub to: char,
}

#[derive(Clone, Copy)]
pub struct FoldMap {
    pub from: char,
    pub to: &'static str,
}

pub type DiacriticSet = &'static [char];

/// ---------------------------------------------------------------------------
/// 3. Internal Entry
/// ---------------------------------------------------------------------------
#[derive(Clone, Copy)]
pub struct LangEntry {
    pub id: Lang,
    pub case_map: &'static [CaseMap],
    pub fold_map: &'static [FoldMap],
    pub diacritics: Option<DiacriticSet>,
    pub needs_segmentation: bool,
}

/// ---------------------------------------------------------------------------
/// 4. Macro that generates EVERYTHING
/// ---------------------------------------------------------------------------
macro_rules! define_languages {
    ($(
        $code:ident, $code_str:literal, $name:literal,
        case: [ $($cfrom:expr => $cto:expr),* $(,)? ],
        fold: [ $($ffrom:expr => $fto:expr),* $(,)? ],
        diac: [ $($d:expr),* $(,)? ],
        segment: $segment:expr
    );* $(;)?) => {
        // Generate constants for each language
        $(
            pub const $code: Lang = Lang {
                code: $code_str,
                name: $name,
            };
        )*

        // Generate data modules for each language
        $(
            paste::paste! {
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
                }
            }
        )*

        // Generate the lookup table
        paste::paste! {
            pub static LANG_TABLE: Map<&'static str, LangEntry> = phf_map! {
                $(
                    $code_str => LangEntry {
                        id: $code,
                        case_map: [<$code:lower _data>]::CASE,
                        fold_map: [<$code:lower _data>]::FOLD,
                        diacritics: if [<$code:lower _data>]::DIAC.is_empty() {
                            None
                        } else {
                            Some([<$code:lower _data>]::DIAC)
                        },
                        needs_segmentation: [<$code:lower _data>]::NEEDS_SEGMENTATION,
                    },
                )*
            };
        }

        // Helper function to get language by code string
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
    // Turkic languages
    TUR, "TUR", "Turkish",
        case: [ 'I' => 'ı', 'İ' => 'i' ],
        fold: [],
        diac: [],
        segment: false;

    AZE, "AZE", "Azerbaijani",
        case: [ 'I' => 'ı', 'İ' => 'i' ],
        fold: [],
        diac: [],
        segment: false;

    KAZ, "KAZ", "Kazakh",
        case: [ 'I' => 'ı', 'İ' => 'i' ],
        fold: [],
        diac: [],
        segment: false;

    KIR, "KIR", "Kyrgyz",
        case: [ 'I' => 'ı', 'İ' => 'i' ],
        fold: [],
        diac: [],
        segment: false;

    // Germanic
    DEU, "DEU", "German",
        case: [],
        fold: [ 'ß' => "SS" ],
        diac: [],
        segment: false;

    NLD, "NLD", "Dutch",
        case: [],
        fold: [ 'Ĳ' => "IJ", 'ĳ' => "ij" ],
        diac: [],
        segment: false;

    DAN, "DAN", "Danish",
        case: [],
        fold: [ 'Å' => "AA", 'å' => "aa" ],
        diac: [],
        segment: false;

    NOR, "NOR", "Norwegian",
        case: [],
        fold: [ 'Æ' => "AE", 'æ' => "ae", 'Ø' => "OE", 'ø' => "oe" ],
        diac: [],
        segment: false;

    SWE, "SWE", "Swedish",
        case: [],
        fold: [ 'Å' => "AA", 'å' => "aa", 'Ä' => "AE", 'ä' => "ae", 'Ö' => "OE", 'ö' => "oe" ],
        diac: [],
        segment: false;

    // Greek
    ELL, "ELL", "Greek",
        case: [ 'Σ' => 'ς' ],
        fold: [],
        diac: [],
        segment: false;

    // Baltic
    LIT, "LIT", "Lithuanian",
        case: [ 'Į' => 'į', 'Ė' => 'ė', 'Ų' => 'ų' ],
        fold: [],
        diac: [],
        segment: false;

    LAV, "LAV", "Latvian",
        case: [ 'Ģ' => 'ģ', 'Ķ' => 'ķ', 'Ļ' => 'ļ', 'Ņ' => 'ņ' ],
        fold: [],
        diac: [],
        segment: false;

    // Arabic & Semitic
    ARA, "ARA", "Arabic",
        case: [],
        fold: [],
        diac: [ 'َ', 'ِ', 'ُ', 'ً', 'ٌ', 'ٍ', 'ْ', 'ّ', 'ٓ', 'ٔ', 'ٕ' ],
        segment: false;

    HEB, "HEB", "Hebrew",
        case: [],
        fold: [],
        diac: [ 'ּ', 'ְ', 'ֱ', 'ֲ', 'ֳ', 'ִ', 'ֵ', 'ֶ', 'ַ', 'ָ', 'ֹ', 'ֻ', 'ֿ', 'ׁ', 'ׂ' ],
        segment: false;

    // Vietnamese
    VIE, "VIE", "Vietnamese",
        case: [],
        fold: [],
        diac: [ '̉', '̃', '́', '̀', '̣', '̂', '̄', '̆', '̛' ],
        segment: false;

    // East Asian (Segmentation Required)
    JPN, "JPN", "Japanese",
        case: [],
        fold: [],
        diac: [],
        segment: true;

    ZHO, "ZHO", "Chinese",
        case: [],
        fold: [],
        diac: [],
        segment: true;

    KOR, "KOR", "Korean",
        case: [],
        fold: [],
        diac: [],
        segment: true;

    THA, "THA", "Thai",
        case: [],
        fold: [],
        diac: [],
        segment: true;

    MYA, "MYA", "Myanmar",
        case: [],
        fold: [],
        diac: [],
        segment: true;

    KHM, "KHM", "Khmer",
        case: [],
        fold: [],
        diac: [],
        segment: true;

    // Other Special Cases
    FRA, "FRA", "French",
        case: [],
        fold: [ 'Œ' => "OE", 'œ' => "oe" ],
        diac: [ '́', '̀', '̂', '̈', '̧', '̌' ],
        segment: false;

    CAT, "CAT", "Catalan",
        case: [],
        fold: [],
        diac: [],
        segment: false;

    HUN, "HUN", "Hungarian",
        case: [],
        fold: [ 'Ő' => "OE", 'ő' => "oe", 'Ű' => "UE", 'ű' => "ue" ],
        diac: [],
        segment: false;

    POL, "POL", "Polish",
        case: [],
        fold: [ 'Ł' => "L", 'ł' => "l" ],
        diac: [],
        segment: false;

    CES, "CES", "Czech",
        case: [],
        fold: [ 'Ď' => "D", 'ď' => "d", 'Ť' => "T", 'ť' => "t" ],
        diac: [],
        segment: false;

    SLK, "SLK", "Slovak",
        case: [],
        fold: [ 'Ľ' => "L", 'ľ' => "l", 'Ĺ' => "L", 'ĺ' => "l" ],
        diac: [],
        segment: false;

    HRV, "HRV", "Croatian",
        case: [],
        fold: [ 'ǈ' => "LJ", 'ǉ' => "lj", 'ǋ' => "NJ", 'ǌ' => "nj" ],
        diac: [],
        segment: false;

    SRP, "SRP", "Serbian",
        case: [],
        fold: [ 'Љ' => "LJ", 'љ' => "lj", 'Њ' => "NJ", 'њ' => "nj", 'Џ' => "DZ", 'џ' => "dz" ],
        diac: [],
        segment: false;

    UKR, "UKR", "Ukrainian",
        case: [],
        fold: [ 'Ґ' => "G", 'ґ' => "g" ],
        diac: [],
        segment: false;

    BUL, "BUL", "Bulgarian",
        case: [],
        fold: [ 'Щ' => "SHT", 'щ' => "sht" ],
        diac: [],
        segment: false;

    // Default (English)
    ENG, "ENG", "English",
        case: [],
        fold: [],
        diac: [],
        segment: false;
}

/// ---------------------------------------------------------------------------
/// 6. LocaleBehavior Trait — **ZERO RUNTIME BRANCH**
/// ---------------------------------------------------------------------------
pub trait LocaleBehavior {
    fn id(&self) -> Lang;
    fn case_map(&self) -> &'static [CaseMap];
    fn fold_map(&self) -> &'static [FoldMap];
    fn diacritics(&self) -> Option<DiacriticSet>;
    fn needs_segmentation(&self) -> bool;
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
