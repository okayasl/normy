pub mod context;
pub mod lang;
pub mod normy;
pub mod process;
pub mod profile;
pub mod stage;

pub use lang::Lang;
pub use lang::{
    ARA, AZE, BUL, CAT, CES, DAN, DEU, ELL, ENG, FRA, HEB, HRV, HUN, JPN, KAZ, KHM, KIR, KOR, LAV,
    LIT, MYA, NLD, NOR, POL, SLK, SRP, SWE, THA, TUR, UKR, VIE, ZHO,
};
pub use normy::Normy;
pub use stage::Utf8Validate;
pub use stage::case_fold::CaseFold;
pub use stage::lower_case::Lowercase;
pub use stage::trim_whitespace::TrimWhitespace;

#[cfg(test)]
mod tests {
    include!("tests/unit.rs");
    include!("tests/integration.rs");
    include!("tests/proptest.rs");
}
