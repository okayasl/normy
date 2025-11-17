pub mod context;
pub mod lang;
pub mod normy;
pub mod process;
pub mod profile;
pub mod stage;
pub mod unicode;

pub use lang::Lang;
pub use lang::{
    // Middle Eastern
    ARA,
    // European – existing
    BUL,
    CAT,
    CES,
    DAN,
    DEU,
    ENG,
    FRA,
    HEB,

    HRV,
    HUN,
    ITA, // Italian

    // Asian
    JPN,
    KHM,
    KOR,
    MYA,
    NLD,
    NOR,
    POL,
    POR, // Portuguese (already present)
    SLK,
    // Newly added Tier-1 major Latin languages
    SPA, // Spanish
    SRP,
    SWE,
    THA,
    TUR,
    UKR,

    // Vietnamese stays
    VIE,
    ZHO,
};
pub use normy::Normy;
pub use stage::fold_case::FoldCase;
pub use stage::lower_case::LowerCase;
pub use stage::normalization::NFC;
pub use stage::normalization::NFD;
pub use stage::normalization::NFKC;
pub use stage::normalization::NFKD;
pub use stage::normalize_whitespace::NormalizeWhitespace;
pub use stage::remove_control_chars::RemoveControlChars;
pub use stage::remove_diacritics::RemoveDiacritics;
pub use stage::remove_format_controls::RemoveFormatControls;
pub use stage::replace_fullwidth::ReplaceFullwidth;

#[cfg(test)]
mod tests {
    include!("tests/unit.rs");
    include!("tests/integration.rs");
    include!("tests/proptest.rs");
}
