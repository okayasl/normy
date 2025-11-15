pub mod context;
pub mod lang;
pub mod normy;
pub mod process;
pub mod profile;
pub mod stage;

pub use lang::Lang;
pub use lang::{
    ARA, BUL, CAT, CES, DAN, DEU, ENG, FRA, HEB, HRV, HUN, JPN, KHM, KOR, LAV, LIT, MYA, NLD, NOR,
    POL, SLK, SRP, SWE, THA, TUR, UKR, VIE, ZHO,
};
pub use normy::Normy;
pub use stage::case_fold::CaseFold;
pub use stage::lower_case::Lowercase;
pub use stage::remove_diacritics::RemoveDiacritics;
pub use stage::trim::Trim;

#[cfg(test)]
mod tests {
    include!("tests/unit.rs");
    include!("tests/integration.rs");
    include!("tests/proptest.rs");
}
