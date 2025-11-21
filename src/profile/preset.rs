use crate::{
    FoldCase, LowerCase, NFKC, NormalizeWhitespace, RemoveControlChars, RemoveDiacritics,
    RemoveFormatControls, ReplaceFullwidth, StripHtml, StripMarkdown, UnigramCJK, process::Process,
    profile::Profile, stage::normalize_punctuation::NormalizePunctuation,
};

/// Ultra-fast path for clean or ASCII-only text
pub fn ascii_fast() -> Profile<impl Process> {
    Profile::builder("ascii_fast")
        .add_stage(NormalizeWhitespace::default())
        .build()
}

/// Light normalization preserving semantic markers.
pub fn machine_translation() -> Profile<impl Process> {
    Profile::builder("machine_translation")
        .add_stage(NFKC)
        .add_stage(RemoveControlChars)
        .add_stage(RemoveFormatControls)
        .add_stage(NormalizePunctuation)
        .add_stage(NormalizeWhitespace::default())
        .build()
}

/// Great for static-site generators, documentation tools.
pub fn markdown_processing() -> Profile<impl Process> {
    Profile::builder("markdown_processing")
        .add_stage(NFKC)
        .add_stage(StripMarkdown)
        .add_stage(RemoveControlChars)
        .add_stage(RemoveFormatControls)
        .add_stage(NormalizeWhitespace::default())
        .build()
}

/// Extract clean text from web sources while preserving case/diacritics.
pub fn web_scraping() -> Profile<impl Process> {
    Profile::builder("web_scraping")
        .add_stage(NFKC)
        .add_stage(StripHtml)
        .add_stage(RemoveControlChars)
        .add_stage(RemoveFormatControls)
        .add_stage(ReplaceFullwidth)
        .add_stage(NormalizeWhitespace::default())
        .build()
}

/// The gold standard — used by Meilisearch, Tantivy, Typesense
pub fn search() -> Profile<impl Process> {
    Profile::builder("search")
        .add_stage(NFKC)
        .add_stage(LowerCase)
        .add_stage(FoldCase)
        .add_stage(RemoveDiacritics)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(RemoveFormatControls)
        .add_stage(NormalizeWhitespace::default())
        .add_stage(UnigramCJK)
        .build()
}

/// CJK-optimized with fullwidth handling and unigram segmentation.
pub fn cjk_search() -> Profile<impl Process> {
    Profile::builder("cjk_search")
        .add_stage(NFKC)
        .add_stage(ReplaceFullwidth)
        .add_stage(RemoveFormatControls)
        .add_stage(RemoveControlChars)
        .add_stage(NormalizeWhitespace::default())
        .add_stage(UnigramCJK) // Critical for CJK tokenization
        .build()
}

/// Preserve maximum information for NER, grammar checking, code analysis.
pub fn minimum() -> Profile<impl Process> {
    Profile::builder("minimal")
        .add_stage(NFKC)
        .add_stage(RemoveControlChars)
        .add_stage(NormalizeWhitespace::collapse_only())
        .build()
}

/// Maximum cleaning — social media, user input, logs
pub fn maximum() -> Profile<impl Process> {
    Profile::builder("aggressive")
        .add_stage(NFKC)
        .add_stage(RemoveDiacritics)
        .add_stage(FoldCase)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(RemoveFormatControls)
        .add_stage(RemoveControlChars)
        .add_stage(ReplaceFullwidth)
        .add_stage(NormalizeWhitespace::default())
        .add_stage(UnigramCJK)
        .build()
}

/// Heavy normalization for noisy user-generated content.
pub fn social_media() -> Profile<impl Process> {
    Profile::builder("social_media")
        .add_stage(NFKC)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(LowerCase)
        .add_stage(FoldCase)
        .add_stage(RemoveDiacritics)
        .add_stage(ReplaceFullwidth) // Handle Asian social media text
        .add_stage(RemoveControlChars)
        .add_stage(RemoveFormatControls)
        .add_stage(NormalizePunctuation)
        .add_stage(NormalizeWhitespace::default())
        .build()
}
