use crate::{
    COLLAPSE_WHITESPACE_ONLY, CaseFold, LowerCase, NFKC, NORMALIZE_WHITESPACE_FULL,
    RemoveDiacritics, SegmentWords, StripControlChars, StripFormatControls, StripHtml,
    StripMarkdown, UnifyWidth, process::Process, profile::Profile,
    stage::normalize_punctuation::NormalizePunctuation,
};

/// Ultra-fast path for clean or ASCII-only text
pub fn ascii_fast() -> Profile<impl Process> {
    Profile::builder("ascii_fast")
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .build()
}

/// Light normalization preserving semantic markers.
pub fn machine_translation() -> Profile<impl Process> {
    Profile::builder("machine_translation")
        .add_stage(NFKC)
        .add_stage(StripControlChars)
        .add_stage(StripFormatControls)
        .add_stage(NormalizePunctuation)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .build()
}

/// Great for static-site generators, documentation tools.
pub fn markdown_processing() -> Profile<impl Process> {
    Profile::builder("markdown_processing")
        .add_stage(NFKC)
        .add_stage(StripMarkdown)
        .add_stage(StripControlChars)
        .add_stage(StripFormatControls)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .build()
}

/// Extract clean text from web sources while preserving case/diacritics.
pub fn web_scraping() -> Profile<impl Process> {
    Profile::builder("web_scraping")
        .add_stage(NFKC)
        .add_stage(StripHtml)
        .add_stage(StripControlChars)
        .add_stage(StripFormatControls)
        .add_stage(UnifyWidth)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .build()
}

/// The gold standard — used by Meilisearch, Tantivy, Typesense
pub fn search() -> Profile<impl Process> {
    Profile::builder("search")
        .add_stage(NFKC)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(StripFormatControls)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .add_stage(SegmentWords)
        .build()
}

/// CJK-optimized with fullwidth handling and unigram segmentation.
pub fn cjk_search() -> Profile<impl Process> {
    Profile::builder("cjk_search")
        .add_stage(NFKC)
        .add_stage(UnifyWidth)
        .add_stage(StripFormatControls)
        .add_stage(StripControlChars)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .add_stage(SegmentWords) // Critical for CJK tokenization
        .build()
}

/// Preserve maximum information for NER, grammar checking, code analysis.
pub fn minimum() -> Profile<impl Process> {
    Profile::builder("minimum")
        .add_stage(NFKC)
        .add_stage(StripControlChars)
        .add_stage(COLLAPSE_WHITESPACE_ONLY)
        .build()
}

/// Maximum cleaning — social media, user input, logs
pub fn maximum() -> Profile<impl Process> {
    Profile::builder("maximum")
        .add_stage(NFKC)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(StripFormatControls)
        .add_stage(StripControlChars)
        .add_stage(UnifyWidth)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .add_stage(SegmentWords)
        .build()
}

/// Heavy normalization for noisy user-generated content.
pub fn social_media() -> Profile<impl Process> {
    Profile::builder("social_media")
        .add_stage(NFKC)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .add_stage(UnifyWidth)
        .add_stage(StripControlChars)
        .add_stage(StripFormatControls)
        .add_stage(NormalizePunctuation)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .build()
}
