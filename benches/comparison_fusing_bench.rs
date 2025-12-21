use std::{borrow::Cow, hint::black_box, time::Duration};

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{
    ARA, COLLAPSE_WHITESPACE_UNICODE, CaseFold, DEU, ENG, JPN, LowerCase, NFC,
    NormalizePunctuation, Normy, RemoveDiacritics, StripControlChars, StripHtml, StripMarkdown,
    TUR, UnifyWidth, VIE, fused_process::ProcessFused, lang::Lang, process::Process,
    static_fused_process::StaticFusedProcess,
};

// Samples with diverse content
const SAMPLES: &[(&str, Lang)] = &[
    // English with HTML, accents, needs NFC normalization
    (
        concat!(
            "<html><head><title>Test</title></head><body>",
            "<h1>Hello Naïve World!</h1>",
            "<p>This is a longer paragraph with <b>bold</b>, <i>italic</i>, and <a href=\"#\">links</a>. ",
            "It includes multiple sentences. Café in French. Résumé with accents. ",
            "Decomposed accents: cafe\u{0301} ",
            "Control chars:\t\r\n\u{200B}\u{200C}. ",
            "Repeated content: Hello world hello world hello world hello world hello world. </p>",
            "<ul><li>Item 1</li><li>Item 2 with emoji 🇺🇸</li></ul>",
            "</body></html>"
        ),
        ENG,
    ),
    // Turkish with dotted letters
    (
        concat!(
            "<div>İSTANBUL'da büyük bir ŞEHİR. İĞNE iğde Iı. ",
            "Longer Turkish text with cafe\u{0301} decomposed. ",
            "İİİİİİİİİİ ıııııııııı. ",
            "HTML tags: <p>Paragraf</p> <b>Kalın</b>. ",
            "Repeated: İstanbul İstanbul İstanbul İstanbul İstanbul.</div>"
        ),
        TUR,
    ),
    // German with eszett
    (
        concat!(
            "GRÜNE STRAßE mit ẞ und ß. Dies ist ein längerer Text mit Maßstab für die Größe. ",
            "Decomposed: Gru\u{0308}ne ",
            "ẞßẞßẞßẞßẞß. ",
            "ÄÖÜäöü in Wörtern. "
        ),
        DEU,
    ),
    // Japanese with half/full width
    (
        concat!(
            "ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟーーこんにちは世界。これは長いテキストで、半角と全角が混在しています。",
            "ﾊﾟﾊﾟﾊﾟﾊﾟﾊﾟﾊﾟﾊﾟﾊﾟ。 ",
            "Repeated: 世界世界世界世界世界世界世界。"
        ),
        JPN,
    ),
    // Arabic with diacritics
    (
        concat!(
            "ٱلْكِتَابُ مُحَمَّدٌ ـــــ هذا نص أطول مع حركات وتطويل. ",
            "Repeated lam-alef: ٱٱٱٱٱٱٱ. ",
            "More: الكتاب محمد الكتاب محمد."
        ),
        ARA,
    ),
    // Vietnamese with stacked diacritics
    (
        concat!(
            "<p>Việt Nam with stacked: Phỏ̉ Tiếng Việt. ",
            "Longer: Đây là một đoạn văn dài hơn với nhiều dấu. ",
            "Decomposed: Vie\u{0302}\u{0323}t ",
            "Repeated: Việt Việt Việt Việt. </p>"
        ),
        VIE,
    ),
];

// Pipeline builders
fn build_single_fusable(lang: Lang) -> Normy<impl Process + ProcessFused + StaticFusedProcess> {
    Normy::builder().lang(lang).add_stage(LowerCase).build()
}

fn build_multi_fusable(lang: Lang) -> Normy<impl Process + ProcessFused + StaticFusedProcess> {
    Normy::builder()
        .lang(lang)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .build()
}

fn build_nfc_fusable(lang: Lang) -> Normy<impl Process + ProcessFused + StaticFusedProcess> {
    Normy::builder()
        .lang(lang)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(RemoveDiacritics)
        .build()
}

fn build_mixed_pipeline(lang: Lang) -> Normy<impl Process + ProcessFused + StaticFusedProcess> {
    Normy::builder()
        .lang(lang)
        .add_stage(StripHtml)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(RemoveDiacritics)
        .build()
}

fn build_complex_pipeline(lang: Lang) -> Normy<impl Process + ProcessFused + StaticFusedProcess> {
    Normy::builder()
        .lang(lang)
        .add_stage(StripHtml)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .add_stage(StripMarkdown)
        .add_stage(UnifyWidth)
        .add_stage(NormalizePunctuation)
        .add_stage(StripControlChars)
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .build()
}

fn build_non_fusable(lang: Lang) -> Normy<impl Process> {
    Normy::builder()
        .lang(lang)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .build()
}

// Helper to check zero-copy on a sample
fn check_zero_copy<'a>(input: &'a str, result: Cow<'a, str>) -> bool {
    matches!(result, Cow::Borrowed(s) if s.as_ptr() == input.as_ptr() && s.len() == input.len())
}

// Correctness verification
fn verify_outputs_match<P: Process + ProcessFused + StaticFusedProcess>(
    pipeline: &Normy<P>,
    text: &str,
) -> Result<(), String> {
    let normal_result = pipeline
        .normalize(text)
        .map_err(|e| format!("normalize failed: {}", e))?;
    let fused_result = pipeline
        .normalize_fused(text)
        .map_err(|e| format!("normalize_fused failed: {}", e))?;
    let static_fused_result = pipeline
        .normalize_static_fused(text)
        .map_err(|e| format!("normalize_static_fused failed: {}", e))?;

    if normal_result != fused_result {
        return Err(format!(
            "Output mismatch (Dynamic Fused)!\n  normalize:       '{}'\n  normalize_fused: '{}'",
            normal_result, fused_result
        ));
    }

    if normal_result != static_fused_result {
        return Err(format!(
            "Output mismatch (Static Fused)!\n  normalize:              '{}'\n  normalize_static_fused: '{}'",
            normal_result, static_fused_result
        ));
    }

    Ok(())
}

// Macro for fused cases
macro_rules! bench_fused_case {
    ($group:expr, $case:expr, $build:expr, $samples:expr) => {
        for &(raw_text, lang) in $samples {
            let pipeline = ($build)(lang);

            // Verify correctness before benchmarking
            if let Err(e) = verify_outputs_match(&pipeline, raw_text) {
                panic!(
                    "❌ {} CORRECTNESS FAILURE for {}/{}: {}",
                    $case,
                    lang.code(),
                    raw_text.len(),
                    e
                );
            }

            let normalized_output = pipeline.normalize(raw_text).unwrap();
            let normalized = normalized_output.into_owned();

            if let Err(e) = verify_outputs_match(&pipeline, &normalized) {
                panic!(
                    "❌ {} CORRECTNESS FAILURE (normalized) for {}/{}: {}",
                    $case,
                    lang.code(),
                    normalized.len(),
                    e
                );
            }

            // Check zero-copy behavior (Static fusion should also be zero-copy where possible)
            let zc_raw_normal = check_zero_copy(raw_text, pipeline.normalize(raw_text).unwrap());
            let zc_raw_static =
                check_zero_copy(raw_text, pipeline.normalize_static_fused(raw_text).unwrap());
            let zc_norm_static = check_zero_copy(
                &normalized,
                pipeline.normalize_static_fused(&normalized).unwrap(),
            );

            let id = format!("{}/{}/{}b", $case, lang.code(), raw_text.len());

            println!(
                "  {} - Zero-Copy [Raw: Normal={} Static={}] [Norm: Static={}]",
                id,
                if zc_raw_normal { "✓" } else { "✗" },
                if zc_raw_static { "✓" } else { "✗" },
                if zc_norm_static { "✓" } else { "✗" },
            );

            // --- RAW INPUT BENCHMARKS ---

            $group.bench_with_input(
                BenchmarkId::new("normalize/raw", &id),
                &raw_text,
                |b, &text| {
                    b.iter(|| black_box(pipeline.normalize(black_box(text)).unwrap()));
                },
            );

            $group.bench_with_input(
                BenchmarkId::new("normalize_fused/raw", &id),
                &raw_text,
                |b, &text| {
                    b.iter(|| black_box(pipeline.normalize_fused(black_box(text)).unwrap()));
                },
            );

            $group.bench_with_input(
                BenchmarkId::new("normalize_static_fused/raw", &id),
                &raw_text,
                |b, &text| {
                    b.iter(|| black_box(pipeline.normalize_static_fused(black_box(text)).unwrap()));
                },
            );

            // --- NORMALIZED INPUT BENCHMARKS (Zero-copy path) ---

            $group.bench_with_input(
                BenchmarkId::new("normalize/normalized", &id),
                &normalized,
                |b, text| {
                    b.iter(|| black_box(pipeline.normalize(black_box(text.as_str())).unwrap()));
                },
            );

            $group.bench_with_input(
                BenchmarkId::new("normalize_fused/normalized", &id),
                &normalized,
                |b, text| {
                    b.iter(|| {
                        black_box(pipeline.normalize_fused(black_box(text.as_str())).unwrap())
                    });
                },
            );

            $group.bench_with_input(
                BenchmarkId::new("normalize_static_fused/normalized", &id),
                &normalized,
                |b, text| {
                    b.iter(|| {
                        black_box(
                            pipeline
                                .normalize_static_fused(black_box(text.as_str()))
                                .unwrap(),
                        )
                    });
                },
            );
        }
    };
}

// Macro for non-fused case
macro_rules! bench_non_fused_case {
    ($group:expr, $case:expr, $build:expr, $samples:expr) => {
        for &(raw_text, lang) in $samples {
            let pipeline = ($build)(lang);
            let normalized_output = pipeline.normalize(raw_text).unwrap();
            let normalized = normalized_output.into_owned();

            let zc_raw = check_zero_copy(raw_text, pipeline.normalize(raw_text).unwrap());
            let zc_norm = check_zero_copy(&normalized, pipeline.normalize(&normalized).unwrap());

            let id = format!("{}/{}/{}b", $case, lang.code(), raw_text.len());
            println!(
                "  {} - raw: {}, normalized: {}",
                id,
                if zc_raw { "✓" } else { "✗" },
                if zc_norm { "✓" } else { "✗" },
            );

            if !zc_norm {
                eprintln!("  ⚠️  WARNING: Low zero-copy on normalized input!");
            }

            // normalize/raw
            $group.bench_with_input(
                BenchmarkId::new("normalize/raw", &id),
                &raw_text,
                |b, &text| {
                    b.iter_batched(
                        || text,
                        |t| black_box(pipeline.normalize(black_box(t)).unwrap()),
                        BatchSize::SmallInput,
                    );
                },
            );

            // normalize/normalized
            $group.bench_with_input(
                BenchmarkId::new("normalize/normalized", &id),
                &normalized,
                |b, text| {
                    b.iter_batched(
                        || text.as_str(),
                        |t| black_box(pipeline.normalize(black_box(t)).unwrap()),
                        BatchSize::SmallInput,
                    );
                },
            );
        }
    };
}

fn fused_vs_normal_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("normalize_vs_normalize_fused");

    println!("\n=== Single Fusable Stage ===");
    bench_fused_case!(group, "single_fusable", build_single_fusable, SAMPLES);

    println!("\n=== Multi Fusable Stages ===");
    bench_fused_case!(group, "multi_fusable", build_multi_fusable, SAMPLES);

    println!("\n=== NFC Fusable Stage ===");
    bench_fused_case!(group, "nfc_fusable", build_nfc_fusable, SAMPLES);

    println!("\n=== Mixed Pipeline ===");
    bench_fused_case!(group, "mixed_pipeline", build_mixed_pipeline, SAMPLES);

    println!("\n=== Complex Pipeline ===");
    bench_fused_case!(group, "complex_pipeline", build_complex_pipeline, SAMPLES);

    println!("\n=== Non-Fusable Only ===");
    bench_non_fused_case!(group, "non_fusable", build_non_fusable, SAMPLES);

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(2))
        .warm_up_time(Duration::from_secs(2))
        .sample_size(500)
        .noise_threshold(0.015)
        .significance_level(0.05);
    targets = fused_vs_normal_benches
);
criterion_main!(benches);

#[cfg(test)]
mod fusion_correctness_tests {
    use std::borrow::Cow;

    use normy::{
        ARA, COLLAPSE_WHITESPACE_UNICODE, CaseFold, DEU, ENG, JPN, LowerCase, NFC, NFD, NFKC, NFKD,
        NormalizePunctuation, Normy, RemoveDiacritics, StripControlChars, StripHtml, StripMarkdown,
        TUR, Transliterate, UnifyWidth, VIE, fused_process::ProcessFused, lang::Lang,
        process::Process, static_fused_process::StaticFusedProcess,
    };

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    // ============================================================================
    // Test Data - Same as benchmark
    // ============================================================================

    const TEST_SAMPLES: &[(&str, Lang)] = &[
        // English with HTML, accents, decomposed chars needing NFC
        (
            concat!(
                "<html><head><title>Test</title></head><body>",
                "<h1>Hello Naïve World!</h1>",
                "<p>This is a longer paragraph with <b>bold</b>, <i>italic</i>, and <a href=\"#\">links</a>. ",
                "It includes multiple sentences. Café in French. Résumé with accents. ",
                "Decomposed accents: cafe\u{0301} re\u{0301}sume\u{0301} ",
                "Control chars:\t\r\n\u{200B}\u{200C}. ",
                "Repeated content: Hello world hello world hello world. </p>",
                "<ul><li>Item 1</li><li>Item 2 with emoji 🇺🇸</li></ul>",
                "</body></html>"
            ),
            ENG,
        ),
        // Turkish with dotted letters
        (
            concat!(
                "<div>İSTANBUL'da büyük bir ŞEHİR. İĞNE iğde Iı. ",
                "Longer Turkish text with cafe\u{0301} decomposed. ",
                "İİİİİİİİİİ ıııııııııı. ",
                "HTML tags: <p>Paragraf</p> <b>Kalın</b>. ",
                "Repeated: İstanbul İstanbul İstanbul.</div>"
            ),
            TUR,
        ),
        // German with eszett and decomposed umlauts
        (
            concat!(
                "GRÜNE STRAßE mit ẞ und ß. Dies ist ein längerer Text mit Maßstab. ",
                "Decomposed: Gru\u{0308}ne Munchner Buro\u{0308} ",
                "ẞßẞßẞß. ",
                "ÄÖÜäöü in Wörtern. "
            ),
            DEU,
        ),
        // Japanese with half/full width
        (
            concat!(
                "ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟーーこんにちは世界。これは長いテキストで、半角と全角が混在。",
                "ﾊﾟﾊﾟﾊﾟﾊﾟﾊﾟﾊﾟﾊﾟﾊﾟ。 ",
                "Full-width: ＡＢＣＤ１２３４ ",
                "Repeated: 世界世界世界。"
            ),
            JPN,
        ),
        // Arabic with diacritics and tatweel
        (
            concat!(
                "ٱلْكِتَابُ مُحَمَّدٌ ـــــ هذا نص مع حركات. ",
                "Repeated lam-alef: ٱٱٱٱٱٱٱ. ",
                "More: الكتاب محمد الكتاب."
            ),
            ARA,
        ),
        // Vietnamese with stacked diacritics
        (
            concat!(
                "<p>Việt Nam with stacked: Phỏ̉ Tiếng Việt. ",
                "Longer: Đây là một đoạn văn dài hơn. ",
                "Decomposed: Vie\u{0302}\u{0323}t Pha\u{0309} ",
                "Repeated: Việt Việt Việt. </p>"
            ),
            VIE,
        ),
    ];

    // Additional edge cases
    const EDGE_CASES: &[(&str, Lang)] = &[
        ("", ENG),                                       // Empty string
        ("hello", ENG),                                  // Pure ASCII
        ("a", ENG),                                      // Single char
        ("\n\t  \r\n", ENG),                             // Only whitespace/control
        ("café café café", ENG),                         // Already composed
        ("cafe\u{0301} cafe\u{0301} cafe\u{0301}", ENG), // All decomposed
        ("混合ASCII日本語text", JPN),                    // Mixed scripts
        ("🎉🎊🎈", ENG),                                 // Emoji only
        ("<>", ENG),                                     // Empty HTML tags
        ("a\u{0301}\u{0302}\u{0303}", ENG),              // Multiple combining marks
        ("İiIı", TUR),                                   // Turkish i variants
        ("ẞßSs", DEU),                                   // German eszett variants
        ("\u{200B}\u{200C}\u{200D}", ENG),               // Zero-width chars
        ("café naïve résumé", ENG),                      // Multiple accents
        ("ＡＢＣＤ１２３４", JPN),                       // Full-width ASCII
        ("ﬁﬀﬂﬃﬄ", ENG),                                  // Ligatures
        ("½⅓¼①②③", ENG),                                 // Fractions and circled
        ("الكتاب", ARA),                                 // RTL text
        ("<b>test</b> **markdown**", ENG),               // Mixed HTML and Markdown
        ("test\r\ntest\r\ntest", ENG),                   // CRLF line endings
    ];

    // ============================================================================
    // Pipeline Builders - Same as benchmark
    // ============================================================================

    fn build_single_fusable(lang: Lang) -> Normy<impl Process + ProcessFused + StaticFusedProcess> {
        Normy::builder().lang(lang).add_stage(NFC).build()
    }

    fn build_multi_fusable(lang: Lang) -> Normy<impl Process + ProcessFused + StaticFusedProcess> {
        Normy::builder()
            .lang(lang)
            .add_stage(LowerCase)
            .add_stage(CaseFold)
            .add_stage(RemoveDiacritics)
            .build()
    }

    fn build_nfc_fusable(lang: Lang) -> Normy<impl Process + ProcessFused + StaticFusedProcess> {
        Normy::builder()
            .lang(lang)
            .add_stage(NFC)
            .add_stage(LowerCase)
            .add_stage(RemoveDiacritics)
            .build()
    }

    fn build_mixed_pipeline(lang: Lang) -> Normy<impl Process + ProcessFused + StaticFusedProcess> {
        Normy::builder()
            .lang(lang)
            .add_stage(StripHtml) // Non-fusable
            .add_stage(NFC) // Fusable
            .add_stage(LowerCase) // Fusable
            .add_stage(RemoveDiacritics) // Fusable
            .build()
    }

    fn build_complex_pipeline(
        lang: Lang,
    ) -> Normy<impl Process + ProcessFused + StaticFusedProcess> {
        Normy::builder()
            .lang(lang)
            .add_stage(StripHtml) // Non-fusable
            .add_stage(NFC) // Fusable
            .add_stage(LowerCase) // Fusable
            .add_stage(CaseFold) // Fusable
            .add_stage(RemoveDiacritics) // Fusable
            .add_stage(StripMarkdown) // Non-fusable
            .add_stage(UnifyWidth) // Fusable
            .add_stage(NormalizePunctuation) // Fusable
            .add_stage(StripControlChars) // Fusable
            .add_stage(COLLAPSE_WHITESPACE_UNICODE) // Fusable
            .build()
    }

    fn build_all_normalizations(
        lang: Lang,
    ) -> Normy<impl Process + ProcessFused + StaticFusedProcess> {
        Normy::builder()
            .lang(lang)
            .add_stage(NFC)
            .add_stage(NFD)
            .add_stage(NFKC)
            .add_stage(NFKD)
            .build()
    }

    fn build_transliterate_pipeline(
        lang: Lang,
    ) -> Normy<impl Process + ProcessFused + StaticFusedProcess> {
        Normy::builder()
            .lang(lang)
            .add_stage(NFC)
            .add_stage(Transliterate)
            .add_stage(LowerCase)
            .build()
    }

    // ============================================================================
    // Helper Function
    // ============================================================================

    fn assert_normalize_equals_fused<P: Process + ProcessFused + StaticFusedProcess>(
        pipeline: &Normy<P>,
        text: &str,
        context: &str,
    ) -> TestResult {
        let normal_result = pipeline.normalize(text)?;
        let fused_result = pipeline.normalize_fused(text)?;
        let static_fused_result = pipeline.normalize_static_fused(text)?;

        assert_eq!(
            normal_result.as_ref(),
            fused_result.as_ref(),
            "\n❌ MISMATCH in {}\n  Input: {:?}\n  normalize:       {:?}\n  normalize_fused: {:?}",
            context,
            text,
            normal_result.as_ref(),
            fused_result.as_ref()
        );

        assert_eq!(
            normal_result.as_ref(),
            static_fused_result.as_ref(),
            "\n❌ MISMATCH in {}\n  Input: {:?}\n  normalize:       {:?}\n  normalize_static_fused: {:?}",
            context,
            text,
            normal_result.as_ref(),
            static_fused_result.as_ref()
        );

        Ok(())
    }

    // ============================================================================
    // Test Macros for DRY
    // ============================================================================

    macro_rules! test_pipeline_variant {
        ($name:ident, $builder:expr, $description:expr) => {
            #[test]
            fn $name() -> TestResult {
                println!("\n=== Testing {} ===", $description);

                // Test all main samples
                for (i, &(text, lang)) in TEST_SAMPLES.iter().enumerate() {
                    let pipeline = ($builder)(lang);
                    let context = format!("{} - sample {} ({})", $description, i, lang.code());

                    // Test raw input
                    assert_normalize_equals_fused(&pipeline, text, &context)?;

                    // Test normalized input (should be idempotent)
                    let normalized = pipeline.normalize(text)?.into_owned();
                    let context_norm = format!(
                        "{} - normalized sample {} ({})",
                        $description,
                        i,
                        lang.code()
                    );
                    assert_normalize_equals_fused(&pipeline, &normalized, &context_norm)?;
                }

                // Test edge cases
                for (i, &(text, lang)) in EDGE_CASES.iter().enumerate() {
                    let pipeline = ($builder)(lang);
                    let context = format!("{} - edge case {} ({})", $description, i, lang.code());
                    assert_normalize_equals_fused(&pipeline, text, &context)?;
                }

                Ok(())
            }
        };
    }

    // ============================================================================
    // Test Cases - One for each pipeline variant
    // ============================================================================

    test_pipeline_variant!(
        test_single_fusable_correctness,
        build_single_fusable,
        "Single Fusable Stage (NFC)"
    );

    test_pipeline_variant!(
        test_multi_fusable_correctness,
        build_multi_fusable,
        "Multiple Fusable Stages"
    );

    test_pipeline_variant!(
        test_nfc_fusable_correctness,
        build_nfc_fusable,
        "NFC + Fusable Pipeline"
    );

    test_pipeline_variant!(
        test_mixed_pipeline_correctness,
        build_mixed_pipeline,
        "Mixed Fusable/Non-fusable Pipeline"
    );

    test_pipeline_variant!(
        test_complex_pipeline_correctness,
        build_complex_pipeline,
        "Complex Pipeline"
    );

    test_pipeline_variant!(
        test_all_normalizations_correctness,
        build_all_normalizations,
        "All Normalization Forms"
    );

    test_pipeline_variant!(
        test_transliterate_correctness,
        build_transliterate_pipeline,
        "Transliterate Pipeline"
    );

    // ============================================================================
    // Special Test: Verify Zero-Copy Behavior
    // ============================================================================

    #[test]
    fn test_zero_copy_on_normalized_input() -> TestResult {
        println!("\n=== Testing Zero-Copy on Normalized Input ===");

        for &(text, lang) in TEST_SAMPLES.iter().chain(EDGE_CASES.iter()) {
            let pipeline = build_nfc_fusable(lang);

            // Normalize once
            let normalized = pipeline.normalize(text)?.into_owned();

            // Re-normalize should be zero-copy
            let result = pipeline.normalize_fused(&normalized)?;

            if let Cow::Borrowed(s) = result {
                // Verify it's actually the same memory location
                assert_eq!(
                    s.as_ptr(),
                    normalized.as_ptr(),
                    "Zero-copy failed: pointers don't match for {:?}",
                    &normalized[..normalized.len().min(50)]
                );
                assert_eq!(
                    s.len(),
                    normalized.len(),
                    "Zero-copy failed: lengths don't match"
                );
            } else {
                // Some cases might legitimately need to allocate
                // (e.g., if the text still needs transformation)
                // But for truly normalized text, we should get Borrowed
                eprintln!(
                    "⚠️  Warning: Got Owned for normalized input: {:?}",
                    &normalized[..normalized.len().min(50)]
                );
            }
        }

        Ok(())
    }

    // ============================================================================
    // Stress Test: Many Iterations
    // ============================================================================

    #[test]
    fn test_fusion_stability_over_iterations() -> TestResult {
        println!("\n=== Testing Fusion Stability Over Multiple Iterations ===");

        let text = "Café naïve ﬁﬀ <b>test</b> cafe\u{0301}";
        let pipeline = build_complex_pipeline(ENG);

        let first_normal = pipeline.normalize(text)?;
        let first_fused = pipeline.normalize_fused(text)?;

        assert_eq!(first_normal, first_fused, "First iteration mismatch");

        // Run many times to catch any state-related bugs
        for i in 0..100 {
            let normal = pipeline.normalize(text)?;
            let fused = pipeline.normalize_fused(text)?;

            assert_eq!(
                normal, fused,
                "Mismatch at iteration {}: normal={:?}, fused={:?}",
                i, normal, fused
            );

            // Should also match first iteration
            assert_eq!(
                normal, first_normal,
                "Result changed at iteration {}: first={:?}, now={:?}",
                i, first_normal, normal
            );
        }

        Ok(())
    }

    // ============================================================================
    // Property Test: Idempotency
    // ============================================================================

    #[test]
    fn test_fusion_idempotency() -> TestResult {
        println!("\n=== Testing Fusion Idempotency ===");

        for &(text, lang) in TEST_SAMPLES.iter().chain(EDGE_CASES.iter()) {
            let pipeline = build_complex_pipeline(lang);

            // normalize_fused should be idempotent
            let once = pipeline.normalize_fused(text)?;
            let twice = pipeline.normalize_fused(once.as_ref())?;
            let thrice = pipeline.normalize_fused(twice.as_ref())?;

            assert_eq!(
                once,
                twice,
                "Not idempotent (1st vs 2nd): input={:?}",
                &text[..text.len().min(50)]
            );
            assert_eq!(
                twice,
                thrice,
                "Not idempotent (2nd vs 3rd): input={:?}",
                &text[..text.len().min(50)]
            );

            // Should also match normalize
            let normal_once = pipeline.normalize(text)?;
            let normal_twice = pipeline.normalize(normal_once.as_ref())?;

            assert_eq!(
                once,
                normal_once,
                "Fused vs normal (1st): input={:?}",
                &text[..text.len().min(50)]
            );
            assert_eq!(
                twice,
                normal_twice,
                "Fused vs normal (2nd): input={:?}",
                &text[..text.len().min(50)]
            );
        }

        Ok(())
    }

    // ============================================================================
    // Test: All Language Codes
    // ============================================================================

    #[test]
    fn test_all_language_codes() -> TestResult {
        println!("\n=== Testing All Language Codes ===");

        let test_text = "Café naïve test café";
        let all_langs = [ENG, DEU, TUR, JPN, ARA, VIE];

        for lang in all_langs {
            let pipeline = build_nfc_fusable(lang);
            assert_normalize_equals_fused(
                &pipeline,
                test_text,
                &format!("Language: {}", lang.code()),
            )?;
        }

        Ok(())
    }

    // ============================================================================
    // Test: Chaining Multiple Normalizations
    // ============================================================================

    #[test]
    fn test_multiple_normalization_forms_chained() -> TestResult {
        println!("\n=== Testing Multiple Normalization Forms Chained ===");

        let texts = [
            "cafe\u{0301}", // Decomposed
            "café",         // Composed
            "ﬁﬀ",           // Ligatures
            "½①",           // Compatibility chars
        ];

        for text in texts {
            let pipeline = build_all_normalizations(ENG);
            let context = format!("Chained normalizations - {:?}", text);
            assert_normalize_equals_fused(&pipeline, text, &context)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod perf_tests {
    use super::*;
    use std::time::Instant;

    // Direct processing - monomorphized, no virtual dispatch
    fn direct_lowercase(text: &str) -> String {
        text.chars().flat_map(|c| c.to_lowercase()).collect()
    }

    // Virtual dispatch - through Box<dyn Iterator>
    fn boxed_lowercase(text: &str) -> String {
        let iter: Box<dyn Iterator<Item = char>> = Box::new(text.chars());
        let mapped: Box<dyn Iterator<Item = char>> = Box::new(iter.flat_map(|c| c.to_lowercase()));
        mapped.collect()
    }

    #[test]
    fn measure_virtual_dispatch_overhead() {
        let text = "Hello World! This is a test. CAFÉ naïve résumé.".repeat(10);
        let iterations = 10_000;

        // Warmup
        for _ in 0..100 {
            black_box(direct_lowercase(black_box(&text)));
            black_box(boxed_lowercase(black_box(&text)));
        }

        // Measure direct
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(direct_lowercase(black_box(&text)));
        }
        let direct_time = start.elapsed();

        // Measure boxed
        let start = Instant::now();
        for _ in 0..iterations {
            black_box(boxed_lowercase(black_box(&text)));
        }
        let boxed_time = start.elapsed();

        println!("\nVirtual Dispatch Overhead Test:");
        println!("Text length: {} chars", text.len());
        println!(
            "Direct:  {:?} ({:.2}ns per call)",
            direct_time,
            direct_time.as_nanos() as f64 / iterations as f64
        );
        println!(
            "Boxed:   {:?} ({:.2}ns per call)",
            boxed_time,
            boxed_time.as_nanos() as f64 / iterations as f64
        );
        println!(
            "Overhead: {:.1}x slower",
            boxed_time.as_secs_f64() / direct_time.as_secs_f64()
        );
    }

    #[test]
    fn measure_chained_overhead() {
        let text = "HELLO WORLD CAFÉ";

        // Direct chaining (monomorphized)
        let direct_chain = || -> String {
            text.chars()
                .flat_map(|c| c.to_lowercase())
                .map(|c| if c.is_ascii() { c } else { 'x' })
                .flat_map(|c| c.to_uppercase())
                .collect()
        };

        // Boxed chaining (dynamic dispatch)
        let boxed_chain = || -> String {
            let mut iter: Box<dyn Iterator<Item = char>> = Box::new(text.chars());
            iter = Box::new(iter.flat_map(|c| c.to_lowercase()));
            iter = Box::new(iter.map(|c| if c.is_ascii() { c } else { 'x' }));
            iter = Box::new(iter.flat_map(|c| c.to_uppercase()));
            iter.collect()
        };

        let iterations = 100_000;

        let start = Instant::now();
        for _ in 0..iterations {
            black_box(direct_chain());
        }
        let direct = start.elapsed();

        let start = Instant::now();
        for _ in 0..iterations {
            black_box(boxed_chain());
        }
        let boxed = start.elapsed();

        println!("\nChained Processing Test:");
        println!(
            "Direct: {:?} ({:.2}ns per call)",
            direct,
            direct.as_nanos() as f64 / iterations as f64
        );
        println!(
            "Boxed:  {:?} ({:.2}ns per call)",
            boxed,
            boxed.as_nanos() as f64 / iterations as f64
        );
        println!(
            "Overhead: {:.1}x slower",
            boxed.as_secs_f64() / direct.as_secs_f64()
        );
    }
}
