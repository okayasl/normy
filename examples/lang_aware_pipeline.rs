use normy::{
    CaseFold, DEU, ENG, FRA, NFKC, NLD, NORMALIZE_WHITESPACE_FULL, Normy, NormyBuilder,
    RemoveDiacritics, SegmentWords, StripFormatControls, StripHtml, StripMarkdown, TUR, ZHO,
    process::FusablePipeline,
};
use std::borrow::Cow;

// ————————————————————————————————
// REUSABLE SEARCH PIPELINE FUNCTION
// ————————————————————————————————
fn search_pipeline() -> NormyBuilder<impl FusablePipeline> {
    Normy::builder()
        // No modify_lang() needed — your static tables already do it perfectly:
        // • TUR has 'I'→'ı', 'İ'→'i'
        // • DEU has 'ß'→"ss"
        // • NLD has 'Ĳ'→"ij" via peek_pairs + requires_peek_ahead = true
        // • ZHO has unigram_cjk = true
        .add_stage(NFKC)
        .add_stage(RemoveDiacritics)
        .add_stage(CaseFold) // ← uses your real fold_map + peek_ahead_fold
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(StripFormatControls)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .add_stage(SegmentWords) // ← only active for ZHO, JPN, etc.
}

fn main() {
    // Build language-specific search normalizers upfront
    let search_tur = search_pipeline().lang(TUR).build();
    let search_fra = search_pipeline().lang(FRA).build();
    let search_deu = search_pipeline().lang(DEU).build();
    let search_nld = search_pipeline().lang(NLD).build();
    let search_eng = search_pipeline().lang(ENG).build();
    let search_zho = search_pipeline().lang(ZHO).build();

    let cases = &[
        // The killer test case — only Normy gets this right
        ("café résumé naïve", &search_tur, "café résumé naïve", "TUR"), // Turkish: preserve foreign accents
        ("café résumé naïve", &search_fra, "cafe resume naive", "FRA"), // French: strip them
        ("İSTANBUL café", &search_tur, "istanbul café", "TUR"),
        ("Weißwurststraße", &search_deu, "weisswurststrasse", "DEU"),
        ("Ĳsselmeer", &search_nld, "ijsselmeer", "NLD"),
        ("hello <b>world</b>", &search_eng, "hello world", "ENG"),
        ("**café** ½-price", &search_eng, "café 1⁄2-price", "ENG"),
        ("family\u{200D}family", &search_eng, "familyfamily", "ENG"),
        ("　一二三　", &search_zho, "一 二 三", "ZHO"),
        ("clean ascii text", &search_eng, "clean ascii text", "ENG"), // ZERO ALLOCATION
    ];

    for (input, normalizer, expected, lang) in cases {
        let result: Cow<str> = normalizer.normalize(input).expect("norm failed");

        let allocated = if result.as_ptr() == input.as_ptr() {
            "NO"
        } else {
            "yes"
        };

        println!(
            "{:<30} → {:<30} | lang: {} | alloc: {}",
            input, result, lang, allocated
        );
        assert_eq!(result.as_ref(), *expected);
    }

    let normalizer = NormyBuilder::default().lang(ENG).build();
    let clean = "already perfectly normalized text";
    let out = normalizer.normalize(clean).unwrap();
    assert_eq!(out.as_ptr(), clean.as_ptr());

    println!("\nZero-copy guarantee: clean input → borrowed output (no allocation)");
}
