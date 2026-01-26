#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
//#![deny(missing_docs, clippy::all)]

pub mod context;
pub mod lang;
pub mod process;
pub mod stage;
pub mod testing;

pub use lang::data::*;
pub use normy::{DynamicNormyBuilder, Normy, NormyBuilder, NormyError};

pub use stage::case_fold::CaseFold;
pub use stage::lower_case::LowerCase;
pub use stage::normalization::{NFC, NFD, NFKC, NFKD};
pub use stage::normalize_punctuation::NormalizePunctuation;
pub use stage::normalize_whitespace::{
    COLLAPSE_WHITESPACE, COLLAPSE_WHITESPACE_UNICODE, NORMALIZE_WHITESPACE_FULL, TRIM_WHITESPACE,
    TRIM_WHITESPACE_UNICODE,
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
    include!("tests/integration.rs");
    include!("tests/proptest.rs");
}
