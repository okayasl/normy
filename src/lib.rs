// #![forbid(unsafe_code)]
// #![deny(missing_docs, clippy::all)]
// #![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod context;
pub mod lang;
pub mod process;
pub mod profile;
pub mod stage;

// Public API — zero duplication, zero indirection
pub use lang::data::*;
pub use normy::{DynNormyBuilder, Normy, NormyBuilder}; // All languages auto-exported

// All stages — flat, zero nesting
pub use stage::fold_case::FoldCase;
pub use stage::lower_case::LowerCase;
pub use stage::normalization::{NFC, NFD, NFKC, NFKD};
pub use stage::normalize_punctuation::NormalizePunctuation;
pub use stage::normalize_whitespace::NormalizeWhitespace;
pub use stage::remove_control_chars::RemoveControlChars;
pub use stage::remove_diacritics::RemoveDiacritics;
pub use stage::remove_format_controls::RemoveFormatControls;
pub use stage::replace_fullwidth::ReplaceFullwidth;
pub use stage::segment_word::SegmentWord;
pub use stage::strip_html::StripHtml;
pub use stage::strip_markdown::StripMarkdown;
pub use stage::unigram_cjk::UnigramCJK;

// Internal only
mod normy;
mod unicode;

#[cfg(test)]
mod tests {
    include!("tests/unit.rs");
    include!("tests/integration.rs");
    include!("tests/proptest.rs");
}
