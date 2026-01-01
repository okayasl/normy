use std::{borrow::Cow, hint::black_box, time::Duration};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{
    ARA, COLLAPSE_WHITESPACE_UNICODE, CaseFold, DEU, ENG, JPN, LowerCase, NFC,
    NormalizePunctuation, Normy, RemoveDiacritics, StripControlChars, StripHtml, StripMarkdown,
    TUR, UnifyWidth, VIE,
    lang::Lang,
    process::{FusablePipeline, Process},
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
fn build_single_fusable(lang: Lang) -> Normy<impl FusablePipeline> {
    Normy::builder().lang(lang).add_stage(LowerCase).build()
}

fn build_multi_fusable(lang: Lang) -> Normy<impl FusablePipeline> {
    Normy::builder()
        .lang(lang)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .build()
}

fn build_nfc_fusable(lang: Lang) -> Normy<impl FusablePipeline> {
    Normy::builder()
        .lang(lang)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(RemoveDiacritics)
        .build()
}

fn build_mixed_pipeline(lang: Lang) -> Normy<impl FusablePipeline> {
    Normy::builder()
        .lang(lang)
        .add_stage(StripHtml)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(RemoveDiacritics)
        .build()
}

fn build_complex_pipeline(lang: Lang) -> Normy<impl FusablePipeline> {
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

fn build_non_fusable(lang: Lang) -> Normy<impl FusablePipeline> {
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
fn verify_outputs_match<P: Process + FusablePipeline>(
    pipeline: &Normy<P>,
    text: &str,
) -> Result<(), String> {
    let normal_result = pipeline
        .normalize(text)
        .map_err(|e| format!("normalize failed: {}", e))?;
    let apply_only_result = pipeline
        .normalize_apply_only(text)
        .map_err(|e| format!("normalize_apply_only failed: {}", e))?;

    if normal_result != apply_only_result {
        return Err(format!(
            "Output mismatch!\n  normalize:            '{}'\n  normalize_apply_only: '{}'",
            normal_result, apply_only_result
        ));
    }

    Ok(())
}

// Macro for fusable cases (BuildIter pipelines)
macro_rules! bench_fusable_case {
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

            // Check zero-copy behavior
            let zc_raw_smart = check_zero_copy(raw_text, pipeline.normalize(raw_text).unwrap());
            let zc_raw_apply =
                check_zero_copy(raw_text, pipeline.normalize_apply_only(raw_text).unwrap());
            let zc_norm_smart =
                check_zero_copy(&normalized, pipeline.normalize(&normalized).unwrap());
            let zc_norm_apply = check_zero_copy(
                &normalized,
                pipeline.normalize_apply_only(&normalized).unwrap(),
            );

            let id = format!("{}/{}/{}b", $case, lang.code(), raw_text.len());
            let fusion_status = if pipeline.uses_fusion() {
                "FUSION"
            } else {
                "APPLY"
            };

            println!(
                "  {} [{}] - Zero-Copy: Raw[Smart={} Apply={}] Norm[Smart={} Apply={}]",
                id,
                fusion_status,
                if zc_raw_smart { "✓" } else { "✗" },
                if zc_raw_apply { "✓" } else { "✗" },
                if zc_norm_smart { "✓" } else { "✗" },
                if zc_norm_apply { "✓" } else { "✗" },
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
                BenchmarkId::new("normalize_apply_only/raw", &id),
                &raw_text,
                |b, &text| {
                    b.iter(|| black_box(pipeline.normalize_apply_only(black_box(text)).unwrap()));
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
                BenchmarkId::new("normalize_apply_only/normalized", &id),
                &normalized,
                |b, text| {
                    b.iter(|| {
                        black_box(
                            pipeline
                                .normalize_apply_only(black_box(text.as_str()))
                                .unwrap(),
                        )
                    });
                },
            );
        }
    };
}

// Macro for non-fusable case
macro_rules! bench_non_fusable_case {
    ($group:expr, $case:expr, $build:expr, $samples:expr) => {
        for &(raw_text, lang) in $samples {
            let pipeline = ($build)(lang);
            let normalized_output = pipeline.normalize(raw_text).unwrap();
            let normalized = normalized_output.into_owned();

            // Check zero-copy for both methods
            let zc_raw_norm = check_zero_copy(raw_text, pipeline.normalize(raw_text).unwrap());
            let zc_raw_apply =
                check_zero_copy(raw_text, pipeline.normalize_apply_only(raw_text).unwrap());
            let zc_norm_norm =
                check_zero_copy(&normalized, pipeline.normalize(&normalized).unwrap());
            let zc_norm_apply = check_zero_copy(
                &normalized,
                pipeline.normalize_apply_only(&normalized).unwrap(),
            );

            let id = format!("{}/{}/{}b", $case, lang.code(), raw_text.len());
            let fusion_status = if pipeline.uses_fusion() {
                "FUSION"
            } else {
                "APPLY"
            };

            println!(
                "  {} [{}] - Zero-Copy: Raw[normalize={} apply={}] Norm[normalize={} apply={}]",
                id,
                fusion_status,
                if zc_raw_norm { "✓" } else { "✗" },
                if zc_raw_apply { "✓" } else { "✗" },
                if zc_norm_norm { "✓" } else { "✗" },
                if zc_norm_apply { "✓" } else { "✗" },
            );

            // normalize/raw (smart routing)
            $group.bench_with_input(
                BenchmarkId::new("normalize/raw", &id),
                &raw_text,
                |b, &text| {
                    b.iter(|| black_box(pipeline.normalize(black_box(text)).unwrap()));
                },
            );

            // normalize_apply_only/raw (direct apply)
            $group.bench_with_input(
                BenchmarkId::new("normalize_apply_only/raw", &id),
                &raw_text,
                |b, &text| {
                    b.iter(|| black_box(pipeline.normalize_apply_only(black_box(text)).unwrap()));
                },
            );

            // normalize/normalized (smart routing)
            $group.bench_with_input(
                BenchmarkId::new("normalize/normalized", &id),
                &normalized,
                |b, text| {
                    b.iter(|| black_box(pipeline.normalize(black_box(text.as_str())).unwrap()));
                },
            );

            // normalize_apply_only/normalized (direct apply)
            $group.bench_with_input(
                BenchmarkId::new("normalize_apply_only/normalized", &id),
                &normalized,
                |b, text| {
                    b.iter(|| {
                        black_box(
                            pipeline
                                .normalize_apply_only(black_box(text.as_str()))
                                .unwrap(),
                        )
                    });
                },
            );
        }
    };
}

fn normalize_vs_apply_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("normalize_vs_apply_only");

    println!("\n=== Single Fusable Stage ===");
    bench_fusable_case!(group, "single_fusable", build_single_fusable, SAMPLES);

    println!("\n=== Multi Fusable Stages ===");
    bench_fusable_case!(group, "multi_fusable", build_multi_fusable, SAMPLES);

    println!("\n=== NFC Fusable Stage ===");
    bench_fusable_case!(group, "nfc_fusable", build_nfc_fusable, SAMPLES);

    println!("\n=== Mixed Pipeline (Has Barriers) ===");
    bench_fusable_case!(group, "mixed_pipeline", build_mixed_pipeline, SAMPLES);

    println!("\n=== Complex Pipeline (Has Barriers) ===");
    bench_fusable_case!(group, "complex_pipeline", build_complex_pipeline, SAMPLES);

    println!("\n=== Non-Fusable Only ===");
    bench_non_fusable_case!(group, "non_fusable", build_non_fusable, SAMPLES);

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
    targets = normalize_vs_apply_benches
);
criterion_main!(benches);
