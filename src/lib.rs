// #![forbid(unsafe_code)]
// #![deny(missing_docs, clippy::all)]
// #![cfg_attr(docsrs, feature(doc_auto_cfg))]

/// Zero-allocation, locale-correct, format-aware text normalization for Rust.
///
/// See the crate-level example for the full power demonstration.
pub mod lang;
pub mod profile;
pub mod stage;

/// High-performance normalization pipeline
pub use normy::Normy;

/// Language identifier and per-language static rules
pub use lang::Lang;

/// Re-export all language constants (optional but convenient)
pub use lang::data::{
    ARA, BUL, CAT, CES, DAN, DEU, ENG, FRA, HEB, HRV, HUN, ITA, JPN, KHM, KOR, MYA, NLD, NOR, POL,
    POR, SLK, SPA, SRP, SWE, THA, TUR, UKR, VIE, ZHO,
};

/// The only two functions users ever need to call
pub mod builder {
    pub use crate::normy::{DynNormyBuilder, NormyBuilder};
    pub use crate::process::EmptyProcess;

    /// Create a new zero-allocation normalization pipeline (recommended).
    ///
    /// All white-paper guarantees are active by default.
    #[inline(always)]
    pub fn builder() -> NormyBuilder<EmptyProcess> {
        NormyBuilder::default()
    }

    /// Create a plugin-compatible dynamic pipeline (uses `Arc<dyn Stage>`).
    #[inline(always)]
    pub fn plugin_builder() -> DynNormyBuilder {
        DynNormyBuilder::new()
    }
}
pub use builder::{builder, plugin_builder};

/// All built-in stages — flat, no nesting, zero cognitive load
pub use stage::fold_case::FoldCase;
pub use stage::lower_case::LowerCase;
pub use stage::normalization::{NFC, NFD, NFKC, NFKD};
pub use stage::normalize_whitespace::NormalizeWhitespace;
pub use stage::remove_control_chars::RemoveControlChars;
pub use stage::remove_diacritics::RemoveDiacritics;
pub use stage::remove_format_controls::RemoveFormatControls;
pub use stage::replace_fullwidth::ReplaceFullwidth;
pub use stage::segment_word::SegmentWord; // future
pub use stage::strip_html::StripHtml;
pub use stage::strip_markdown::StripMarkdown;
pub use stage::unigram_cjk::UnigramCJK; // future

// ──────────────────────────────────────────────────────────────
// Everything below this line is INTERNAL — never exposed to users
// ──────────────────────────────────────────────────────────────
mod context;
mod normy;
mod process;
mod unicode;

#[cfg(test)]
mod tests {
    include!("tests/unit.rs");
    include!("tests/integration.rs");
    include!("tests/proptest.rs");
}
