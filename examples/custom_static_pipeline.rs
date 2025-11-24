use normy::{
    CaseFold, CjkUnigram, DEU, ENG, FRA, NFKC, NLD, NormalizeWhitespace, NormyBuilder,
    RemoveDiacritics, StripFormatControls, StripHtml, StripMarkdown, TUR, ZHO,
    profile::ProfileBuilder,
};
use std::borrow::Cow;

fn main() {
    let profile_builder = ProfileBuilder::new("search_profile");
    let profile = profile_builder
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
        .add_stage(NormalizeWhitespace::default())
        .add_stage(CjkUnigram) // ← only active for ZHO, JPN, etc.
        .build();

    let cases = &[
        // The killer test case — only Normy gets this right
        ("café résumé naïve", TUR, "café résumé naïve"), // Turkish: preserve foreign accents
        ("café résumé naïve", FRA, "cafe resume naive"), // French: strip them
        ("İSTANBUL café", TUR, "istanbul café"),
        ("Weißwurststraße", DEU, "weisswurststrasse"),
        ("Ĳsselmeer", NLD, "ijsselmeer"),
        ("hello <b>world</b>", ENG, "hello world"),
        ("**café** ½-price", ENG, "café 1⁄2-price"),
        ("family\u{200D}family", ENG, "familyfamily"),
        ("　一二三　", ZHO, "一 二 三"),
        ("clean ascii text", ENG, "clean ascii text"), // ZERO ALLOCATION
    ];

    for (input, lang, expected) in cases {
        let normalizer = NormyBuilder::default().lang(*lang).build();
        let result: Cow<str> = normalizer
            .normalize_with_profile(&profile, input)
            .expect("norm failed");

        let allocated = if result.as_ptr() == input.as_ptr() {
            "NO"
        } else {
            "yes"
        };

        println!("{:<30} → {:<30} | alloc: {}", input, result, allocated);
        assert_eq!(result.as_ref(), *expected);
    }

    let normalizer = NormyBuilder::default().lang(ENG).build();
    let clean = "already perfectly normalized text";
    let out = normalizer.normalize(clean).unwrap();
    assert_eq!(out.as_ptr(), clean.as_ptr());
    println!("\nZero-copy guarantee: clean input → borrowed output (no allocation)");
}
