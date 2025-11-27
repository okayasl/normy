#![forbid(unsafe_code)]
// #![deny(missing_docs, clippy::all)]
// #![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod context;
pub mod lang;
pub mod process;
pub mod profile;
pub mod stage;
pub mod testing;

pub use lang::data::*;
pub use normy::{DynamicNormyBuilder, Normy, NormyBuilder}; // All languages auto-exported

// All stages — flat, zero nesting
pub use stage::case_fold::CaseFold;
pub use stage::cjk_unigram::CjkUnigram;
pub use stage::lower_case::LowerCase;
pub use stage::normalization::{NFC, NFD, NFKC, NFKD};
pub use stage::normalize_punctuation::NormalizePunctuation;
pub use stage::normalize_whitespace::{
    COLLAPSE_WHITESPACE_ONLY, NORMALIZE_WHITESPACE_FULL, TRIM_WHITESPACE_ONLY,
};
pub use stage::remove_diacritics::RemoveDiacritics;
pub use stage::segment_words::SegmentWords;
pub use stage::strip_control_chars::StripControlChars;
pub use stage::strip_format_controls::StripFormatControls;
pub use stage::strip_html::StripHtml;
pub use stage::strip_markdown::StripMarkdown;
pub use stage::transliterate::Transliterate;
pub use stage::unify_width::UnifyWidth;

// Internal only
mod normy;
mod unicode;

#[cfg(test)]
mod tests {
    include!("tests/unit.rs");
    include!("tests/integration.rs");
    include!("tests/proptest.rs");
}
