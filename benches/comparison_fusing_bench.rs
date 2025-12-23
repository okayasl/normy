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
    use super::*; // Gain access to SAMPLES and build_* functions from the outer scope
    use normy::{NFD, NFKC, NFKD, Transliterate};
    use std::borrow::Cow;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    // ============================================================================
    // Additional Edge Cases (Specific to Testing)
    // ============================================================================
    const EDGE_CASES: &[(&str, Lang)] = &[
        ("", ENG),
        ("hello", ENG),
        ("a", ENG),
        ("\n\t  \r\n", ENG),
        ("café café café", ENG),
        ("cafe\u{0301} cafe\u{0301} cafe\u{0301}", ENG),
        ("混合ASCII日本語text", JPN),
        ("🎉🎊🎈", ENG),
        ("<>", ENG),
        ("a\u{0301}\u{0302}\u{0303}", ENG),
        ("İiIı", TUR),
        ("ẞßSs", DEU),
        ("\u{200B}\u{200C}\u{200D}", ENG),
        ("café naïve résumé", ENG),
        ("ＡＢＣＤ１２３４", JPN),
        ("ﬁﬀﬂﬃﬄ", ENG),
        ("½⅓¼①②③", ENG),
        ("الكتاب", ARA),
        ("<b>test</b> **markdown**", ENG),
        ("test\r\ntest\r\ntest", ENG),
    ];

    // ============================================================================
    // Test Pipelines (Extending the benchmark set for full coverage)
    // ============================================================================

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
    // Core Verification Logic
    // ============================================================================

    fn assert_normalize_equals_fused<P: Process + ProcessFused + StaticFusedProcess>(
        pipeline: &Normy<P>,
        text: &str,
        context: &str,
    ) -> TestResult {
        // We reuse the existing outer verify_outputs_match function
        verify_outputs_match(pipeline, text)
            .map_err(|e| format!("Mismatch in {}: {}", context, e).into())
    }

    macro_rules! test_pipeline_variant {
        ($name:ident, $builder:ident, $description:expr) => {
            #[test]
            fn $name() -> TestResult {
                // Test against benchmark SAMPLES
                for (i, &(text, lang)) in SAMPLES.iter().enumerate() {
                    let pipeline = $builder(lang);
                    let context = format!("{} - sample {} ({})", $description, i, lang.code());
                    assert_normalize_equals_fused(&pipeline, text, &context)?;

                    // Test idempotency
                    let normalized = pipeline.normalize(text)?.into_owned();
                    assert_normalize_equals_fused(
                        &pipeline,
                        &normalized,
                        &format!("{}_idempotency", context),
                    )?;
                }

                // Test against EDGE_CASES
                for (i, &(text, lang)) in EDGE_CASES.iter().enumerate() {
                    let pipeline = $builder(lang);
                    let context = format!("{} - edge case {} ({})", $description, i, lang.code());
                    assert_normalize_equals_fused(&pipeline, text, &context)?;
                }
                Ok(())
            }
        };
    }

    // ============================================================================
    // Generated Tests (Using Exact Benchmark Pipelines)
    // ============================================================================

    test_pipeline_variant!(test_single_fusable, build_single_fusable, "Single Fusable");
    test_pipeline_variant!(test_multi_fusable, build_multi_fusable, "Multi Fusable");
    test_pipeline_variant!(test_nfc_fusable, build_nfc_fusable, "NFC Fusable");
    test_pipeline_variant!(test_mixed_pipeline, build_mixed_pipeline, "Mixed Pipeline");
    test_pipeline_variant!(
        test_complex_pipeline,
        build_complex_pipeline,
        "Complex Pipeline"
    );

    // Testing specialized test-only pipelines
    test_pipeline_variant!(
        test_all_norms,
        build_all_normalizations,
        "All Normalizations"
    );
    test_pipeline_variant!(
        test_transliterate,
        build_transliterate_pipeline,
        "Transliterate"
    );

    #[test]
    fn test_zero_copy_on_normalized_input() -> TestResult {
        for &(text, lang) in SAMPLES.iter().chain(EDGE_CASES.iter()) {
            let pipeline = build_nfc_fusable(lang);
            let normalized = pipeline.normalize(text)?.into_owned();
            let result = pipeline.normalize_static_fused(&normalized)?;

            if let Cow::Borrowed(s) = result {
                assert_eq!(
                    s.as_ptr(),
                    normalized.as_ptr(),
                    "Pointer mismatch for: {}",
                    text
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod static_fusion_debug {
    use super::*;
    use crate::{
        COLLAPSE_WHITESPACE_UNICODE, CaseFold, ENG, LowerCase, NFC, NormalizePunctuation,
        RemoveDiacritics, StripControlChars, StripHtml, StripMarkdown, UnifyWidth,
    };

    const COMPLEX_INPUT: &str = concat!(
        "<html><head><title>Test</title></head><body>",
        "<h1>Hello Naïve World!</h1>",
        "<p>This is a longer paragraph with <b>bold</b>, <i>italic</i>, and <a href=\"#\">links</a>. ",
        "It includes multiple sentences. Café in French. Résumé with accents. ",
        "Decomposed accents: cafe\u{0301} ",
        "Control chars:\t\r\n\u{200B}\u{200C}. ",
        "Repeated content: Hello world hello world hello world hello world hello world. </p>",
        "<ul><li>Item 1</li><li>Item 2 with emoji 🇺🇸</li></ul>",
        "</body></html>"
    );

    #[test]
    fn test_complex_pipeline_correctness() {
        let pipeline = Normy::builder()
            .lang(ENG)
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
            .build();

        let result_normal = pipeline.normalize(COMPLEX_INPUT).unwrap();
        let result_static = pipeline.normalize_static_fused(COMPLEX_INPUT).unwrap();

        assert_eq!(
            result_normal, result_static,
            "Static fusion produces different output than normal path!"
        );

        println!("✓ Correctness verified");
        println!("  Input length: {} bytes", COMPLEX_INPUT.len());
        println!("  Output length: {} bytes", result_normal.len());
    }

    #[test]
    fn test_single_stage_vs_multi_stage() {
        let input = "Hello Café Résumé cafe\u{0301}";

        let single_pipeline = Normy::builder().lang(ENG).add_stage(LowerCase).build();

        let multi_pipeline = Normy::builder()
            .lang(ENG)
            .add_stage(LowerCase)
            .add_stage(CaseFold)
            .add_stage(RemoveDiacritics)
            .build();

        let single_result = single_pipeline.normalize(input).unwrap();
        let multi_result = multi_pipeline.normalize(input).unwrap();

        println!("Single stage output: {}", single_result);
        println!("Multi stage output: {}", multi_result);

        assert!(!single_result.is_empty());
        assert!(!multi_result.is_empty());
    }

    #[test]
    fn test_allocation_pattern() {
        let pipeline = Normy::builder()
            .lang(ENG)
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
            .build();

        // Test 1: Normalized input (should be zero-copy)
        let normalized = "hello world cafe resume";
        let result = pipeline.normalize_static_fused(normalized).unwrap();

        let is_borrowed = matches!(result, Cow::Borrowed(_));
        println!("Normalized input zero-copy: {}", is_borrowed);
        assert!(
            is_borrowed,
            "Static fusion should return borrowed for normalized input!"
        );

        // Test 2: Raw input (should allocate)
        let result = pipeline.normalize_static_fused(COMPLEX_INPUT).unwrap();
        assert!(matches!(result, Cow::Owned(_)));
        println!("Raw input allocated: {} bytes", result.len());
    }

    #[test]
    fn test_deep_island_overhead() {
        println!("\n=== Island Depth Performance ===");
        let input = "HELLO WORLD ".repeat(50); // 600 chars

        // Depth 2
        {
            let pipeline = Normy::builder()
                .lang(ENG)
                .add_stage(LowerCase)
                .add_stage(CaseFold)
                .build();

            let start = std::time::Instant::now();
            let _result = pipeline.normalize_static_fused(&input).unwrap();
            let duration = start.elapsed();
            println!("Depth 2: {:?}", duration);
        }

        // Depth 4
        {
            let pipeline = Normy::builder()
                .lang(ENG)
                .add_stage(LowerCase)
                .add_stage(CaseFold)
                .add_stage(RemoveDiacritics)
                .add_stage(NFC)
                .build();

            let start = std::time::Instant::now();
            let _result = pipeline.normalize_static_fused(&input).unwrap();
            let duration = start.elapsed();
            println!("Depth 4: {:?}", duration);
        }

        // Depth 6
        {
            let pipeline = Normy::builder()
                .lang(ENG)
                .add_stage(LowerCase)
                .add_stage(CaseFold)
                .add_stage(RemoveDiacritics)
                .add_stage(NFC)
                .add_stage(UnifyWidth)
                .add_stage(NormalizePunctuation)
                .build();

            let start = std::time::Instant::now();
            let _result = pipeline.normalize_static_fused(&input).unwrap();
            let duration = start.elapsed();
            println!("Depth 6: {:?}", duration);
        }

        // Depth 8
        {
            let pipeline = Normy::builder()
                .lang(ENG)
                .add_stage(LowerCase)
                .add_stage(CaseFold)
                .add_stage(RemoveDiacritics)
                .add_stage(NFC)
                .add_stage(UnifyWidth)
                .add_stage(NormalizePunctuation)
                .add_stage(StripControlChars)
                .add_stage(COLLAPSE_WHITESPACE_UNICODE)
                .build();

            let start = std::time::Instant::now();
            let _result = pipeline.normalize_static_fused(&input).unwrap();
            let duration = start.elapsed();
            println!("Depth 8: {:?}", duration);
        }
    }

    #[test]
    fn test_character_by_character_overhead() {
        let pipeline = Normy::builder()
            .lang(ENG)
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
            .build();

        println!("\n=== Character-by-Character Overhead ===");
        for size in [10, 50, 100, 500, 1000] {
            let input = format!("<p>{}</p>", "A".repeat(size));

            let start = std::time::Instant::now();
            let _result = pipeline.normalize_static_fused(&input).unwrap();
            let duration = start.elapsed();

            let ns_per_char = duration.as_nanos() / size as u128;
            println!(
                "Size {:4}: {:?} ({:4} ns/char)",
                size, duration, ns_per_char
            );
        }
    }

    #[test]
    fn test_compare_paths_detailed() {
        let pipeline = Normy::builder()
            .lang(ENG)
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
            .build();

        let input = COMPLEX_INPUT;

        // Warm up (important for accurate timing!)
        for _ in 0..10 {
            let _ = pipeline.normalize(input).unwrap();
            let _ = pipeline.normalize_static_fused(input).unwrap();
        }

        // Measure normal path (multiple iterations for accuracy)
        let mut total_normal = std::time::Duration::ZERO;
        for _ in 0..100 {
            let start = std::time::Instant::now();
            let _ = pipeline.normalize(input).unwrap();
            total_normal += start.elapsed();
        }
        let time_normal = total_normal / 100;

        // Measure static fused path
        let mut total_static = std::time::Duration::ZERO;
        for _ in 0..100 {
            let start = std::time::Instant::now();
            let _ = pipeline.normalize_static_fused(input).unwrap();
            total_static += start.elapsed();
        }
        let time_static = total_static / 100;

        // Verify correctness
        let result_normal = pipeline.normalize(input).unwrap();
        let result_static = pipeline.normalize_static_fused(input).unwrap();
        assert_eq!(result_normal, result_static);

        println!("\n=== Performance Comparison (avg of 100 runs) ===");
        println!("Input:  {} bytes", input.len());
        println!("Output: {} bytes", result_normal.len());
        println!("Normal path:       {:?}", time_normal);
        println!("Static fused path: {:?}", time_static);

        if time_normal.as_nanos() > 0 {
            let overhead =
                (time_static.as_nanos() as f64 / time_normal.as_nanos() as f64 - 1.0) * 100.0;
            println!("Overhead: {:.1}%", overhead);

            if overhead > 100.0 {
                println!("\n⚠️  WARNING: Static fusion is more than 2x slower!");
                println!("   This indicates a fundamental implementation issue.");
                panic!("Static fusion performance regression detected!");
            } else if overhead > 50.0 {
                println!("\n⚠️  WARNING: Static fusion is significantly slower (>50%)");
            } else if overhead > 20.0 {
                println!("\n⚠️  Static fusion is moderately slower (>20%)");
            } else if overhead < -10.0 {
                println!("\n✅ Static fusion is FASTER! Good job!");
            } else {
                println!("\n✅ Performance is comparable (within 20%)");
            }
        }
    }
}

#[cfg(test)]
mod perf_regression_tests {
    use std::time::Instant;

    use normy::{context::Context, stage::Stage};

    use super::*;
    use crate::{
        COLLAPSE_WHITESPACE_UNICODE, CaseFold, ENG, LowerCase, NFC, NormalizePunctuation,
        RemoveDiacritics, StripControlChars, StripHtml, StripMarkdown, TUR, UnifyWidth,
    };

    // ========================================================================
    // WORST CASE #1: Turkish with Complex Pipeline (+81.9% overhead)
    // ========================================================================

    const TURKISH_INPUT: &str = concat!(
        "<div>İSTANBUL'da büyük bir ŞEHİR. İĞNE iğde Iı. ",
        "Longer Turkish text with cafe\u{0301} decomposed. ",
        "İİİİİİİİİİ ıııııııııı. ",
        "HTML tags: <p>Paragraf</p> <b>Kalın</b>. ",
        "Repeated: İstanbul İstanbul İstanbul İstanbul İstanbul.</div>"
    );

    #[test]
    fn test_turkish_complex_pipeline_regression() {
        let pipeline = Normy::builder()
            .lang(TUR) // ⚠️ Turkish has special case mapping!
            .add_stage(StripHtml)
            .add_stage(NFC)
            .add_stage(LowerCase) // ❌ This is doing linear search per char!
            .add_stage(CaseFold)
            .add_stage(RemoveDiacritics)
            .add_stage(StripMarkdown)
            .add_stage(UnifyWidth)
            .add_stage(NormalizePunctuation)
            .add_stage(StripControlChars)
            .add_stage(COLLAPSE_WHITESPACE_UNICODE)
            .build();

        // Warm up
        for _ in 0..10 {
            let _ = pipeline.normalize(TURKISH_INPUT).unwrap();
            let _ = pipeline.normalize_static_fused(TURKISH_INPUT).unwrap();
        }

        // Measure
        let mut total_normal = std::time::Duration::ZERO;
        let mut total_static = std::time::Duration::ZERO;

        for _ in 0..100 {
            let start_normal = Instant::now();
            let _ = pipeline.normalize(TURKISH_INPUT).unwrap();
            total_normal += start_normal.elapsed();

            let start_static = Instant::now();
            let _ = pipeline.normalize_static_fused(TURKISH_INPUT).unwrap();
            total_static += start_static.elapsed();
        }

        let avg_normal = total_normal / 100;
        let avg_static = total_static / 100;

        println!("\n=== TURKISH COMPLEX PIPELINE (Worst Case #1) ===");
        println!(
            "Input: {} bytes, {} chars",
            TURKISH_INPUT.len(),
            TURKISH_INPUT.chars().count()
        );
        println!("Normal:        {:?}", avg_normal);
        println!("Static fused:  {:?}", avg_static);

        if avg_normal.as_nanos() > 0 {
            let overhead =
                (avg_static.as_nanos() as f64 / avg_normal.as_nanos() as f64 - 1.0) * 100.0;
            println!("Overhead:      {:.1}%", overhead);

            if overhead > 80.0 {
                println!("🔥 CRITICAL: Fusing overhead is more thant 80 percent");
            }
        }

        // Verify correctness
        assert_eq!(
            pipeline.normalize(TURKISH_INPUT).unwrap(),
            pipeline.normalize_static_fused(TURKISH_INPUT).unwrap()
        );
    }

    #[test]
    fn test_which_stages_modify_turkish_text() {
        println!("\n=== Stage-by-Stage Transformation Analysis ===");
        println!("Input: {}", TURKISH_INPUT);
        println!(
            "Length: {} bytes, {} chars\n",
            TURKISH_INPUT.len(),
            TURKISH_INPUT.chars().count()
        );

        let ctx = Context::new(TUR);

        // ========================================================================
        // VERIFICATION: Build actual pipeline and compare
        // ========================================================================
        let actual_pipeline = Normy::builder()
            .lang(TUR)
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
            .build();

        let actual_result = actual_pipeline.normalize(TURKISH_INPUT).unwrap();
        let actual_static_result = actual_pipeline
            .normalize_static_fused(TURKISH_INPUT)
            .unwrap();
        // Verify correctness
        assert_eq!(
            actual_result,
            actual_static_result
        );

        println!("🔍 VERIFICATION: Actual pipeline result:");
        println!("   {}\n", actual_result);

        // ========================================================================
        // PART 1: Manual Sequential Transformation
        // ========================================================================
        println!("=== PART 1: Manual Sequential Transformation ===");
        println!("(Each stage receives the OUTPUT of the previous stage)\n");

        let mut current_text = Cow::Borrowed(TURKISH_INPUT);
        let mut stage_num = 0;

        macro_rules! apply_stage {
            ($stage:expr, $name:expr) => {{
                stage_num += 1;
                let stage = $stage;

                let before = current_text.to_string();
                let before_chars = before.chars().count();

                let needs = stage.needs_apply(&current_text, &ctx).unwrap();
                current_text = stage.apply(current_text, &ctx).unwrap();

                let after = current_text.to_string();
                let after_chars = after.chars().count();
                let modified = before != after;

                println!("Stage {}: {}", stage_num, $name);
                println!("  Input (from previous):    {} chars", before_chars);
                println!("  needs_apply (on input):   {}", needs);
                println!("  actually modified:        {}", modified);
                println!("  Output:                   {} chars", after_chars);

                if modified {
                    println!("  ✓ DID WORK");
                    let preview_before = if before.len() > 60 {
                        format!("{}...", &before[..60])
                    } else {
                        before.clone()
                    };
                    let preview_after = if after.len() > 60 {
                        format!("{}...", &after[..60])
                    } else {
                        after.clone()
                    };
                    if preview_before != preview_after {
                        println!("  Before: {}", preview_before);
                        println!("  After:  {}", preview_after);
                    }
                } else {
                    println!("  ✗ NO-OP (text unchanged)");
                }
                println!();
            }};
        }

        apply_stage!(StripHtml, "StripHtml");

        let island1_start = current_text.to_string();
        println!("──────────────────────────────────────────────────────");
        println!("🏝️  ISLAND 1 STARTS (4 fusable stages)");
        println!("──────────────────────────────────────────────────────\n");

        apply_stage!(NFC, "NFC");
        apply_stage!(LowerCase, "LowerCase");
        apply_stage!(CaseFold, "CaseFold");
        apply_stage!(RemoveDiacritics, "RemoveDiacritics");

        println!("──────────────────────────────────────────────────────");
        println!("🏝️  ISLAND 1 ENDS");
        println!("──────────────────────────────────────────────────────\n");

        apply_stage!(StripMarkdown, "StripMarkdown");

        let island2_start = current_text.to_string();
        println!("──────────────────────────────────────────────────────");
        println!("🏝️  ISLAND 2 STARTS (4 fusable stages)");
        println!("──────────────────────────────────────────────────────\n");

        apply_stage!(UnifyWidth, "UnifyWidth");
        apply_stage!(NormalizePunctuation, "NormalizePunctuation");
        apply_stage!(StripControlChars, "StripControlChars");
        apply_stage!(COLLAPSE_WHITESPACE_UNICODE, "COLLAPSE_WHITESPACE_UNICODE");

        println!("──────────────────────────────────────────────────────");
        println!("🏝️  ISLAND 2 ENDS");
        println!("──────────────────────────────────────────────────────\n");

        // ========================================================================
        // VERIFICATION: Compare with actual pipeline
        // ========================================================================
        let manual_result = current_text.to_string();

        println!("=== VERIFICATION RESULT ===");
        println!("Manual stage-by-stage: {}", manual_result);
        println!("Actual pipeline:       {}", actual_result);
        println!();

        if manual_result == actual_result.as_ref() {
            println!("✅ VERIFIED: Manual application matches actual pipeline!");
        } else {
            println!("❌ ERROR: Manual application DIFFERS from actual pipeline!");
            println!("   This test is WRONG!");
            panic!("Test verification failed!");
        }

        // ========================================================================
        // PART 2: Static Fusion Analysis
        // ========================================================================
        println!("\n=== PART 2: What Static Fusion Sees ===");
        println!("(When fusing an island, all stages check needs_apply on the SAME input)\n");

        println!("🏝️  ISLAND 1 Analysis");
        println!("All 4 stages will be checked against the ISLAND START:");
        println!(
            "  Input: {}...",
            &island1_start[..80.min(island1_start.len())]
        );
        println!();

        // Check what each stage reports on the ORIGINAL island input
        let nfc_needs = NFC.needs_apply(&island1_start, &ctx).unwrap();
        let lower_needs = LowerCase.needs_apply(&island1_start, &ctx).unwrap();
        let fold_needs = CaseFold.needs_apply(&island1_start, &ctx).unwrap();
        let diacritics_needs = RemoveDiacritics.needs_apply(&island1_start, &ctx).unwrap();

        println!("needs_apply checks on ISLAND START (not sequential outputs):");
        println!(
            "  NFC:              {} {}",
            nfc_needs,
            if nfc_needs {
                "→ FUSED"
            } else {
                "→ skipped"
            }
        );
        println!(
            "  LowerCase:        {} {}",
            lower_needs,
            if lower_needs {
                "→ FUSED"
            } else {
                "→ skipped"
            }
        );
        println!(
            "  CaseFold:         {} {}",
            fold_needs,
            if fold_needs {
                "→ FUSED"
            } else {
                "→ skipped"
            }
        );
        println!(
            "  RemoveDiacritics: {} {}",
            diacritics_needs,
            if diacritics_needs {
                "→ FUSED"
            } else {
                "→ skipped"
            }
        );

        // Now show what sequential checks would be (FIX: use cloning to avoid move)
        println!("\nFor comparison, if we checked SEQUENTIALLY:");
        let after_nfc = NFC.apply(Cow::Borrowed(&island1_start), &ctx).unwrap();
        let after_nfc_str = after_nfc.as_ref();

        let after_lower = LowerCase.apply(Cow::Borrowed(after_nfc_str), &ctx).unwrap();
        let after_lower_str = after_lower.as_ref();

        let after_fold = CaseFold
            .apply(Cow::Borrowed(after_lower_str), &ctx)
            .unwrap();
        let after_fold_str = after_fold.as_ref();

        println!("  NFC needs on island start:    {}", nfc_needs);
        println!(
            "  Lower needs on NFC output:    {}",
            LowerCase.needs_apply(after_nfc_str, &ctx).unwrap()
        );
        println!(
            "  Fold needs on Lower output:   {}",
            CaseFold.needs_apply(after_lower_str, &ctx).unwrap()
        );
        println!(
            "  Diacritics needs on Fold out: {}",
            RemoveDiacritics.needs_apply(after_fold_str, &ctx).unwrap()
        );

        let island1_fused = [nfc_needs, lower_needs, fold_needs, diacritics_needs]
            .iter()
            .filter(|&&x| x)
            .count();

        println!("\n📊 Island 1 Static Fusion:");
        println!("  Stages in iterator chain: 4");
        println!("  Stages with needs_apply=true: {}", island1_fused);
        println!("  No-op stages in chain: {}", 4 - island1_fused);

        if island1_fused < 4 {
            let wasted = island1_start.chars().count() * (4 - island1_fused);
            println!(
                "  ⚠️  {} × {} chars = {} wasted Iterator::next() calls!",
                4 - island1_fused,
                island1_start.chars().count(),
                wasted
            );
        }

        // Same for Island 2
        println!("\n🏝️  ISLAND 2 Analysis");
        println!("All 4 stages will be checked against the ISLAND START:");
        println!(
            "  Input: {}...",
            &island2_start[..80.min(island2_start.len())]
        );
        println!();

        let width_needs = UnifyWidth.needs_apply(&island2_start, &ctx).unwrap();
        let punct_needs = NormalizePunctuation
            .needs_apply(&island2_start, &ctx)
            .unwrap();
        let control_needs = StripControlChars.needs_apply(&island2_start, &ctx).unwrap();
        let collapse_needs = COLLAPSE_WHITESPACE_UNICODE
            .needs_apply(&island2_start, &ctx)
            .unwrap();

        println!("needs_apply checks on ISLAND START:");
        println!(
            "  UnifyWidth:         {} {}",
            width_needs,
            if width_needs {
                "→ FUSED"
            } else {
                "→ skipped"
            }
        );
        println!(
            "  NormalizePunct:     {} {}",
            punct_needs,
            if punct_needs {
                "→ FUSED"
            } else {
                "→ skipped"
            }
        );
        println!(
            "  StripControlChars:  {} {}",
            control_needs,
            if control_needs {
                "→ FUSED"
            } else {
                "→ skipped"
            }
        );
        println!(
            "  CollapseWhitespace: {} {}",
            collapse_needs,
            if collapse_needs {
                "→ FUSED"
            } else {
                "→ skipped"
            }
        );

        let island2_fused = [width_needs, punct_needs, control_needs, collapse_needs]
            .iter()
            .filter(|&&x| x)
            .count();

        println!("\n📊 Island 2 Static Fusion:");
        println!("  Stages in iterator chain: 4");
        println!("  Stages with needs_apply=true: {}", island2_fused);
        println!("  No-op stages in chain: {}", 4 - island2_fused);

        if island2_fused < 4 {
            let wasted = island2_start.chars().count() * (4 - island2_fused);
            println!(
                "  ⚠️  {} × {} chars = {} wasted Iterator::next() calls!",
                4 - island2_fused,
                island2_start.chars().count(),
                wasted
            );
        }

        println!("\n=== THE KEY INSIGHT ===");
        println!("Normal pipeline (apply): Each stage checks needs_apply on ITS OWN input");
        println!("Static fusion:           All island stages check needs_apply on SAME input");
        println!();
        println!("Result: Stages that would return false on transformed input");
        println!("        return true on original input → get included in chain → waste cycles!");
    }
    // ========================================================================
    // WORST CASE #2: English with Complex Pipeline (+72.4% overhead)
    // ========================================================================

    const ENGLISH_INPUT: &str = concat!(
        "<html><head><title>Test</title></head><body>",
        "<h1>Hello Naïve World!</h1>",
        "<p>This is a longer paragraph with <b>bold</b>, <i>italic</i>, and <a href=\"#\">links</a>. ",
        "It includes multiple sentences. Café in French. Résumé with accents. ",
        "Decomposed accents: cafe\u{0301} ",
        "Control chars:\t\r\n\u{200B}\u{200C}. ",
        "Repeated content: Hello world hello world hello world hello world hello world. </p>",
        "<ul><li>Item 1</li><li>Item 2 with emoji 🇺🇸</li></ul>",
        "</body></html>"
    );

    #[test]
    fn test_english_complex_pipeline_regression() {
        let pipeline = Normy::builder()
            .lang(ENG)
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
            .build();

        // Warm up
        for _ in 0..10 {
            let _ = pipeline.normalize(ENGLISH_INPUT).unwrap();
            let _ = pipeline.normalize_static_fused(ENGLISH_INPUT).unwrap();
        }

        let mut total_normal = std::time::Duration::ZERO;
        let mut total_static = std::time::Duration::ZERO;

        for _ in 0..100 {
            let start = std::time::Instant::now();
            let _ = pipeline.normalize(ENGLISH_INPUT).unwrap();
            total_normal += start.elapsed();

            let start = std::time::Instant::now();
            let _ = pipeline.normalize_static_fused(ENGLISH_INPUT).unwrap();
            total_static += start.elapsed();
        }

        let avg_normal = total_normal / 100;
        let avg_static = total_static / 100;

        println!("\n=== ENGLISH COMPLEX PIPELINE (Worst Case #2) ===");
        println!(
            "Input: {} bytes, {} chars",
            ENGLISH_INPUT.len(),
            ENGLISH_INPUT.chars().count()
        );
        println!("Normal:        {:?}", avg_normal);
        println!("Static fused:  {:?}", avg_static);

        if avg_normal.as_nanos() > 0 {
            let overhead =
                (avg_static.as_nanos() as f64 / avg_normal.as_nanos() as f64 - 1.0) * 100.0;
            println!("Overhead:      {:.1}%", overhead);
        }

        assert_eq!(
            pipeline.normalize(ENGLISH_INPUT).unwrap(),
            pipeline.normalize_static_fused(ENGLISH_INPUT).unwrap()
        );
    }

    // ========================================================================
    // PROFILING TEST: Isolate the apply_lowercase bottleneck
    // ========================================================================

    #[test]
    fn test_apply_lowercase_bottleneck() {
        println!("\n=== apply_lowercase Performance Profile ===");

        // Test Turkish (worst case - lots of special mappings)
        {
            let ctx = Context::new(TUR);
            let input = "İSTANBUL istanbul İĞNE iğne Iı İİİİİİİİİİ ıııııııııı";

            let start = std::time::Instant::now();
            for _ in 0..10000 {
                for c in input.chars() {
                    let _ = ctx.lang_entry.apply_lowercase(c);
                }
            }
            let duration = start.elapsed();

            let total_calls = 10000 * input.chars().count();
            let ns_per_call = duration.as_nanos() / total_calls as u128;

            println!("Turkish (TUR):");
            println!("  Calls: {}", total_calls);
            println!("  Total: {:?}", duration);
            println!("  Per call: {} ns", ns_per_call);

            if ns_per_call > 50 {
                println!("  🔥 SLOW! Expected <20ns, got {}ns", ns_per_call);
                println!("     This confirms linear search is the bottleneck!");
            }
        }

        // Test English (control)
        {
            let ctx = Context::new(ENG);
            let input = "HELLO WORLD hello world ABCDEFGHIJKLMNOPQRSTUVWXYZ";

            let start = std::time::Instant::now();
            for _ in 0..10000 {
                for c in input.chars() {
                    let _ = ctx.lang_entry.apply_lowercase(c);
                }
            }
            let duration = start.elapsed();

            let total_calls = 10000 * input.chars().count();
            let ns_per_call = duration.as_nanos() / total_calls as u128;

            println!("\nEnglish (ENG):");
            println!("  Calls: {}", total_calls);
            println!("  Total: {:?}", duration);
            println!("  Per call: {} ns", ns_per_call);
        }
    }

    // ========================================================================
    // STRESS TEST: Just LowerCase stage in isolation
    // ========================================================================

    #[test]
    fn test_lowercase_stage_isolation() {
        let turkish_pipeline = Normy::builder().lang(TUR).add_stage(LowerCase).build();

        let english_pipeline = Normy::builder().lang(ENG).add_stage(LowerCase).build();

        let turkish_input = "İSTANBUL İĞNE ".repeat(50); // 700 chars
        let english_input = "HELLO WORLD ".repeat(50); // 600 chars

        println!("\n=== LowerCase Stage Isolation ===");

        // Turkish
        {
            let mut total_normal = std::time::Duration::ZERO;
            let mut total_static = std::time::Duration::ZERO;

            for _ in 0..100 {
                let start = std::time::Instant::now();
                let _ = turkish_pipeline.normalize(&turkish_input).unwrap();
                total_normal += start.elapsed();

                let start = std::time::Instant::now();
                let _ = turkish_pipeline
                    .normalize_static_fused(&turkish_input)
                    .unwrap();
                total_static += start.elapsed();
            }

            println!("Turkish ({} chars):", turkish_input.chars().count());
            println!("  Normal:       {:?}", total_normal / 100);
            println!("  Static fused: {:?}", total_static / 100);
            println!(
                "  Per-char (static): {} ns",
                total_static.as_nanos() / 100 / turkish_input.chars().count() as u128
            );
        }

        // English
        {
            let mut total_normal = std::time::Duration::ZERO;
            let mut total_static = std::time::Duration::ZERO;

            for _ in 0..100 {
                let start = std::time::Instant::now();
                let _ = english_pipeline.normalize(&english_input).unwrap();
                total_normal += start.elapsed();

                let start = std::time::Instant::now();
                let _ = english_pipeline
                    .normalize_static_fused(&english_input)
                    .unwrap();
                total_static += start.elapsed();
            }

            println!("\nEnglish ({} chars):", english_input.chars().count());
            println!("  Normal:       {:?}", total_normal / 100);
            println!("  Static fused: {:?}", total_static / 100);
            println!(
                "  Per-char (static): {} ns",
                total_static.as_nanos() / 100 / english_input.chars().count() as u128
            );
        }
    }
}
