use std::{borrow::Cow, hint::black_box, time::Duration};

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{
    ARA, CAT, CES, CaseFold, DAN, DEU, ENG, FRA, HEB, HIN, ISL, JPN, KHM, KOR, LIT, LowerCase, NLD,
    NOR, Normy, POL, POR, RUS, RemoveDiacritics, SWE, SegmentWords, THA, TUR, Transliterate, VIE,
    ZHO,
    context::Context,
    lang::Lang,
    stage::{Stage, StaticFusableStage},
};

// ============================================================================
// Language-Specific Test Data
// ============================================================================

/// LowerCase uses case_map (TUR, LIT) + fallback to Unicode
const LOWERCASE_SAMPLES: &[(&str, Lang)] = &[
    // Languages with case_map
    ("İSTANBUL İĞNE İĞDE", TUR),
    ("IÌ Í Ĩ IĮ ĖĖ ŲŲ", LIT),
    // Baseline (no case_map, uses Unicode)
    ("HELLO WORLD", ENG),
];

/// CaseFold uses fold_map (DEU, NLD) + case_map (TUR, LIT) + fallback
const CASEFOLD_SAMPLES: &[(&str, Lang)] = &[
    // Languages with fold_map (multi-char expansions)
    ("GRÜßE STRAẞE", DEU),
    ("IJssEL Ĳssel", NLD),
    // Languages with only case_map (fallback)
    ("İSTANBUL İĞNE", TUR),
    ("IÌ Í Ĩ IĮ ĖĖ", LIT),
    // Baseline
    ("HELLO WORLD", ENG),
];

/// Transliterate - ordered by mapping count (RUS highest at 66)
const TRANSLITERATE_SAMPLES: &[(&str, Lang)] = &[
    // High complexity (66 mappings)
    ("Привет мир ЁЛКИ-ПАЛКИ", RUS),
    // Medium complexity (6 mappings each)
    ("Äöü ÄÖÜ Grüße", DEU),
    ("Œuf çà Sœur", FRA),
    ("Åse Ææble Øre", DAN),
    ("Ærlig Øl Åtte", NOR),
    ("Ålder Ääkta Öga", SWE),
    ("Þórður Ægir Ðóra", ISL),
    // Low complexity (2 mappings)
    ("Força Çà", CAT),
    // Baseline (0 mappings)
    ("Hello World", ENG),
];

/// RemoveDiacritics - both precomposed_to_base and spacing_diacritics
/// Ordered by total mapping count
const REMOVEDIACRITICS_SAMPLES: &[(&str, Lang)] = &[
    // Highest complexity (146 precomposed + 5 spacing = 151)
    ("Việt Nam Phở Phỏ̉", VIE),
    // High complexity (26 precomposed)
    ("José café naïve résumé", POR),
    ("Café naïve à l'œuf", FRA),
    // Medium complexity (18-20 precomposed)
    ("Český řeřicha háček", CES),
    ("Łódź żółć Kraków", POL),
    // Spacing diacritics only (no precomposed)
    ("ٱلْكِتَابُ مُحَمَّدٌ ـــــ", ARA), // 14 spacing
    ("עִבְרִית ספר", HEB),         // 20 spacing
    ("हिन्दी ज़िंदगी", HIN),       // 5 spacing
    ("ภาษาไทย สวัสดี", THA),      // 16 spacing
    // Baseline (0 mappings)
    ("Hello World", ENG),
];

/// SegmentWords - languages with segment_rules
const SEGMENTWORDS_SAMPLES: &[(&str, Lang)] = &[
    // CJK unigram (highest complexity)
    ("汉字仮名한글漢字", ZHO),
    // Script boundary segmentation
    ("日本語テキストです", JPN),
    ("한글 텍스트입니다", KOR),
    ("हिन्दी ज़िंदगी है", HIN),
    ("ภาษาไทย สวัสดี", THA),
    ("ភាសាខ្មែរ", KHM),
    // Baseline (no rules)
    ("Hello World", ENG),
];

// ============================================================================
// Benchmark Functions
// ============================================================================

fn bench_stage_focused<S, C>(
    c: &mut Criterion,
    stage_name: &str,
    samples: &[(&str, Lang)],
    constructor: C,
) where
    S: Stage + StaticFusableStage + 'static,
    C: Fn() -> S + Copy,
{
    let mut group = c.benchmark_group(stage_name.to_string());

    for &(text, lang) in samples {
        let stage = constructor();
        let ctx = Context::new(lang);

        // Get unchanged version by normalizing once
        let normalized = {
            let stage = constructor();
            stage.apply(Cow::Borrowed(text), &ctx).unwrap().into_owned()
        };

        let is_unchanged = text == normalized.as_str();
        let status = if is_unchanged { "unchanged" } else { "changed" };

        let supports_fusion = stage.supports_static_fusion();

        // ====================================================================
        // Changed Input Benchmarks
        // ====================================================================

        // Full pipeline (includes needs_apply overhead)
        let id = format!("{}/{}/{}", lang.code(), status, text);
        group.bench_function(BenchmarkId::new("pipeline", &id), |b| {
            b.iter_batched(
                || text,
                |t| {
                    let stage = constructor();
                    let normy = Normy::builder().lang(lang).add_stage(stage).build();
                    black_box(normy.normalize(t).unwrap().into_owned())
                },
                BatchSize::SmallInput,
            )
        });

        // Direct apply (no pipeline overhead)
        group.bench_function(BenchmarkId::new("apply", &id), |b| {
            b.iter_batched(
                constructor,
                |stage| {
                    let ctx = Context::new(lang);
                    black_box(stage.apply(Cow::Borrowed(text), &ctx).unwrap())
                },
                BatchSize::SmallInput,
            )
        });

        if supports_fusion {
            group.bench_function(BenchmarkId::new("static_fusion", &id), |b| {
                b.iter_batched(
                    constructor,
                    |stage| {
                        let ctx = Context::new(lang);
                        let iter = stage.static_fused_adapter(text.chars(), &ctx);
                        black_box(iter.collect::<String>())
                    },
                    BatchSize::SmallInput,
                )
            });
        }

        // ====================================================================
        // Unchanged Input Benchmarks (if different from changed)
        // ====================================================================

        if !is_unchanged {
            let unchanged_id = format!("{}/unchanged/{}", lang.code(), normalized);

            // Full pipeline
            group.bench_function(BenchmarkId::new("pipeline", &unchanged_id), |b| {
                b.iter_batched(
                    || normalized.as_str(),
                    |t| {
                        let stage = constructor();
                        let normy = Normy::builder().lang(lang).add_stage(stage).build();
                        black_box(normy.normalize(t).unwrap().into_owned())
                    },
                    BatchSize::SmallInput,
                )
            });

            // Direct apply
            group.bench_function(BenchmarkId::new("apply", &unchanged_id), |b| {
                b.iter_batched(
                    constructor,
                    |stage| {
                        let ctx = Context::new(lang);
                        black_box(stage.apply(Cow::Borrowed(&normalized), &ctx).unwrap())
                    },
                    BatchSize::SmallInput,
                )
            });

            // Static fusion
            if supports_fusion {
                group.bench_function(BenchmarkId::new("static_fusion", &unchanged_id), |b| {
                    b.iter_batched(
                        constructor,
                        |stage| {
                            let ctx = Context::new(lang);
                            let iter = stage.static_fused_adapter(normalized.chars(), &ctx);
                            black_box(iter.collect::<String>())
                        },
                        BatchSize::SmallInput,
                    )
                });
            }
        }
    }

    group.finish();
}

// ============================================================================
// Individual Stage Benchmarks
// ============================================================================

fn bench_lowercase_focused(c: &mut Criterion) {
    bench_stage_focused(c, "LowerCase", LOWERCASE_SAMPLES, || LowerCase);
}

fn bench_casefold_focused(c: &mut Criterion) {
    bench_stage_focused(c, "CaseFold", CASEFOLD_SAMPLES, || CaseFold);
}

fn bench_transliterate_focused(c: &mut Criterion) {
    bench_stage_focused(c, "Transliterate", TRANSLITERATE_SAMPLES, || Transliterate);
}

fn bench_removediacritics_focused(c: &mut Criterion) {
    bench_stage_focused(c, "RemoveDiacritics", REMOVEDIACRITICS_SAMPLES, || {
        RemoveDiacritics
    });
}

fn bench_segmentwords_focused(c: &mut Criterion) {
    bench_stage_focused(c, "SegmentWords", SEGMENTWORDS_SAMPLES, || SegmentWords);
}

// ============================================================================
// Comparison Benchmark: Show fusion overhead clearly
// ============================================================================

fn bench_fusion_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("fusion_overhead_analysis");

    // Test cases that show the pattern clearly
    let test_cases = [
        ("Fast op (no-op)", "Hello", ENG, RemoveDiacritics),
        ("Fast op (work)", "café", FRA, RemoveDiacritics),
        ("Medium op", "Привет", RUS, RemoveDiacritics),
        ("Slow op", "Việt Nam Phỏ̉", VIE, RemoveDiacritics),
    ];

    for (label, text, lang, stage) in test_cases {
        let ctx = Context::new(lang);

        // Baseline: apply
        group.bench_function(BenchmarkId::new("apply", label), |b| {
            b.iter(|| black_box(stage.apply(Cow::Borrowed(text), &ctx).unwrap()))
        });

        // Fusion
        if stage.supports_static_fusion() {
            group.bench_function(BenchmarkId::new("fusion", label), |b| {
                b.iter(|| {
                    let iter = stage.static_fused_adapter(text.chars(), &ctx);
                    black_box(iter.collect::<String>())
                })
            });
        }

        // Calculate overhead
        println!("  {} overhead will be measured", label);
    }

    group.finish();
}

// ============================================================================
// Criterion Configuration
// ============================================================================

criterion_group!(
    name = focused_benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(3))
        .warm_up_time(Duration::from_secs(1))
        .sample_size(500)
        .noise_threshold(0.015)
        .significance_level(0.05);
    targets =
        bench_lowercase_focused,
        bench_casefold_focused,
        bench_transliterate_focused,
        bench_removediacritics_focused,
        bench_segmentwords_focused,
        bench_fusion_overhead
);

criterion_main!(focused_benches);
