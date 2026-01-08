use std::{borrow::Cow, hint::black_box, time::Duration};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{
    ARA, CAT, CES, CaseFold, DAN, DEU, ENG, FRA, HEB, HIN, ISL, JPN, KHM, KOR, LIT, LowerCase, NLD,
    NOR, Normy, POL, POR, RUS, RemoveDiacritics, SWE, SegmentWords, THA, TUR, Transliterate, VIE,
    ZHO,
    context::Context,
    lang::Lang,
    stage::{Stage, StaticFusableStage},
};

// ============================================================================
// Length Configuration
// ============================================================================

struct LengthConfig {
    name: &'static str,
    target_bytes: usize,
    description: &'static str,
}

const LENGTH_CONFIGS: &[LengthConfig] = &[
    LengthConfig {
        name: "short",
        target_bytes: 100,
        description: "Single sentence",
    },
    LengthConfig {
        name: "medium",
        target_bytes: 1000,
        description: "Paragraph",
    },
    LengthConfig {
        name: "long",
        target_bytes: 2000,
        description: "Multi-paragraph",
    },
    LengthConfig {
        name: "huge",
        target_bytes: 5000,
        description: "Document",
    },
];

// ============================================================================
// Text Generation Helper
// ============================================================================

fn generate_text(base: &str, target_len: usize) -> String {
    if target_len <= base.len() {
        return base.to_string();
    }

    let repetitions = (target_len / base.len()) + 1;
    let mut result = String::with_capacity(target_len);

    for _ in 0..repetitions {
        result.push_str(base);
        if result.len() >= target_len {
            break;
        }
    }

    if result.len() > target_len {
        let mut truncate_at = target_len;
        while truncate_at > 0 && !result.is_char_boundary(truncate_at) {
            truncate_at -= 1;
        }
        result.truncate(truncate_at);
    }

    result
}

// ============================================================================
// Base Samples
// ============================================================================

const LOWERCASE_SAMPLES: &[(&str, Lang)] = &[
    ("İSTANBUL İĞNE İĞDE ", TUR),
    ("IĮ Į Ĩ IĮ ĖĖ ŲŲ ", LIT),
    ("HELLO WORLD ", ENG),
];

const CASEFOLD_SAMPLES: &[(&str, Lang)] = &[
    ("GRÜẞE STRAẞE ", DEU),
    ("IJssEL Ĳssel ", NLD),
    ("İSTANBUL İĞNE ", TUR),
    ("IĮ Į Ĩ IĮ ĖĖ ", LIT),
    ("HELLO WORLD ", ENG),
];

const TRANSLITERATE_SAMPLES: &[(&str, Lang)] = &[
    ("Привет мир АЛКИ-ПАЛКИ ", RUS),
    ("Äöü ÄÖÜ Grüße ", DEU),
    ("Œuf çà Sœur ", FRA),
    ("Åse Ææble Øre ", DAN),
    ("Ærlig Øl Åtte ", NOR),
    ("Ålder Ääkta Öga ", SWE),
    ("Þórður Ægir Þóra ", ISL),
    ("Força Çà ", CAT),
    ("Hello World ", ENG),
];

const REMOVEDIACRITICS_SAMPLES: &[(&str, Lang)] = &[
    ("Việt Nam Phở Phở̉ ", VIE),
    ("José café naïve résumé ", POR),
    ("Café naïve à l'œuf ", FRA),
    ("Český řeřicha háček ", CES),
    ("Łódź żółć Kraków ", POL),
    ("اَلْكِتَابُ مُحَمَّدٌ ـــــ ", ARA),
    ("עִבְרִית ספר ", HEB),
    ("हिन्दी ज़िंदगी ", HIN),
    ("ภาษาไทย สวัสดี ", THA),
    ("Hello World ", ENG),
];

const SEGMENTWORDS_SAMPLES: &[(&str, Lang)] = &[
    ("汉字仮名글자漢字 ", ZHO),
    ("日本語テキストです ", JPN),
    ("한글 텍스트입니다 ", KOR),
    ("हिन्दी ज़िंदगी है ", HIN),
    ("ภาษาไทย สวัสดี ", THA),
    ("ភាសាខ្មែរ ", KHM),
    ("Hello World ", ENG),
];

// ============================================================================
// RESTRUCTURED: Length Scaling Benchmark
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
    let mut group = c.benchmark_group(format!("{}_length_scaling", stage_name));

    println!("\n┌────────────────────────────────────────────────────────────┐");
    println!("│ 📊 {} - Length Scaling Benchmark", stage_name);
    println!("└────────────────────────────────────────────────────────────┘\n");

    for &(base_text, lang) in samples {
        println!("  🌍 Language: {} ({})", lang.name(), lang.code());

        for config in LENGTH_CONFIGS {
            let text = generate_text(base_text, config.target_bytes);
            let actual_len = text.len();

            println!(
                "    📏 {} ({} bytes - {})",
                config.name, actual_len, config.description
            );

            // Pre-normalize once to get unchanged version
            let ctx = Context::new(lang);
            let normalized = {
                let stage = constructor();
                stage
                    .apply(Cow::Borrowed(&text), &ctx)
                    .unwrap()
                    .into_owned()
            };

            let is_unchanged = text == normalized.as_str();
            let status = if is_unchanged { "unchanged" } else { "changed" };
            let supports_fusion = constructor().supports_static_fusion();

            let bench_id = format!("{}/{}/{}", lang.code(), config.name, status);

            // ═══════════════════════════════════════════════════════════
            // FIXED: Pre-construct all objects outside timing loop
            // ═══════════════════════════════════════════════════════════

            // 1. PIPELINE - Pure operation (no construction overhead)
            {
                let stage = constructor();
                let normy = Normy::builder().lang(lang).add_stage(stage).build();

                group.bench_function(BenchmarkId::new("pipeline", &bench_id), |b| {
                    b.iter(|| black_box(normy.normalize(&text).unwrap()))
                });
            }

            // 2. APPLY - Pure operation (no construction overhead)
            {
                let stage = constructor();
                let ctx = Context::new(lang);

                group.bench_function(BenchmarkId::new("apply", &bench_id), |b| {
                    b.iter(|| black_box(stage.apply(Cow::Borrowed(&text), &ctx).unwrap()))
                });
            }

            // 3. FUSION - Pure operation (no construction overhead)
            if supports_fusion {
                let stage = constructor();
                let ctx = Context::new(lang);

                group.bench_function(BenchmarkId::new("static_fusion", &bench_id), |b| {
                    b.iter(|| {
                        let iter = stage.static_fused_adapter(text.chars(), &ctx);
                        black_box(iter.collect::<String>())
                    })
                });
            }

            // ═══════════════════════════════════════════════════════════
            // UNCHANGED TEXT BENCHMARKS
            // ═══════════════════════════════════════════════════════════

            if !is_unchanged {
                let unchanged_bench_id = format!("{}/{}/unchanged", lang.code(), config.name);

                // Pipeline - unchanged
                {
                    let stage = constructor();
                    let normy = Normy::builder().lang(lang).add_stage(stage).build();

                    group.bench_function(BenchmarkId::new("pipeline", &unchanged_bench_id), |b| {
                        b.iter(|| black_box(normy.normalize(&normalized).unwrap()))
                    });
                }

                // Apply - unchanged
                {
                    let stage = constructor();
                    let ctx = Context::new(lang);

                    group.bench_function(BenchmarkId::new("apply", &unchanged_bench_id), |b| {
                        b.iter(|| black_box(stage.apply(Cow::Borrowed(&normalized), &ctx).unwrap()))
                    });
                }

                // Fusion - unchanged
                if supports_fusion {
                    let stage = constructor();
                    let ctx = Context::new(lang);

                    group.bench_function(
                        BenchmarkId::new("static_fusion", &unchanged_bench_id),
                        |b| {
                            b.iter(|| {
                                let iter = stage.static_fused_adapter(normalized.chars(), &ctx);
                                black_box(iter.collect::<String>())
                            })
                        },
                    );
                }
            }
        }
        println!();
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
// NEW: Construction Overhead Benchmark (separate from operation timing)
// ============================================================================

fn bench_construction_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("construction_overhead");

    println!("\n┌────────────────────────────────────────────────────────────┐");
    println!("│ 🏗️  Construction Overhead Analysis");
    println!("└────────────────────────────────────────────────────────────┘\n");

    // LowerCase
    {
        println!("  📦 LowerCase");

        group.bench_function("stage_construction/LowerCase", |b| {
            b.iter(|| black_box(LowerCase))
        });

        group.bench_function("context_construction/LowerCase", |b| {
            b.iter(|| black_box(Context::new(RUS)))
        });

        group.bench_function("pipeline_construction/LowerCase", |b| {
            b.iter(|| black_box(Normy::builder().lang(RUS).add_stage(LowerCase).build()))
        });
    }

    // Transliterate
    {
        println!("  📦 Transliterate");

        group.bench_function("stage_construction/Transliterate", |b| {
            b.iter(|| black_box(Transliterate))
        });

        group.bench_function("context_construction/Transliterate", |b| {
            b.iter(|| black_box(Context::new(RUS)))
        });

        group.bench_function("pipeline_construction/Transliterate", |b| {
            b.iter(|| black_box(Normy::builder().lang(RUS).add_stage(Transliterate).build()))
        });
    }

    // RemoveDiacritics
    {
        println!("  📦 RemoveDiacritics");

        group.bench_function("stage_construction/RemoveDiacritics", |b| {
            b.iter(|| black_box(RemoveDiacritics))
        });

        group.bench_function("context_construction/RemoveDiacritics", |b| {
            b.iter(|| black_box(Context::new(VIE)))
        });

        group.bench_function("pipeline_construction/RemoveDiacritics", |b| {
            b.iter(|| {
                black_box(
                    Normy::builder()
                        .lang(VIE)
                        .add_stage(RemoveDiacritics)
                        .build(),
                )
            })
        });
    }

    group.finish();
}

// ============================================================================
// Fusion Overhead Analysis
// ============================================================================

fn bench_fusion_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("fusion_overhead_analysis");

    println!("\n┌────────────────────────────────────────────────────────────┐");
    println!("│ 📊 Fusion Overhead Analysis");
    println!("└────────────────────────────────────────────────────────────┘\n");

    // Fast op (no-op) - English
    {
        let label = "Fast op (no-op)";
        let base_text = "Hello ";
        let lang = ENG;
        let stage = RemoveDiacritics;

        println!("  📊 Testing: {}", label);

        for config in LENGTH_CONFIGS {
            let text = generate_text(base_text, config.target_bytes);
            let ctx = Context::new(lang);

            println!("    📏 {} ({} bytes)", config.name, text.len());

            let bench_id = format!("{}/{}", label, config.name);

            group.bench_function(BenchmarkId::new("apply", &bench_id), |b| {
                b.iter(|| black_box(stage.apply(Cow::Borrowed(&text), &ctx).unwrap()))
            });

            if stage.supports_static_fusion() {
                group.bench_function(BenchmarkId::new("fusion", &bench_id), |b| {
                    b.iter(|| {
                        let iter = stage.static_fused_adapter(text.chars(), &ctx);
                        black_box(iter.collect::<String>())
                    })
                });
            }
        }
        println!();
    }

    // Fast op (work) - French
    {
        let label = "Fast op (work)";
        let base_text = "café ";
        let lang = FRA;
        let stage = RemoveDiacritics;

        println!("  📊 Testing: {}", label);

        for config in LENGTH_CONFIGS {
            let text = generate_text(base_text, config.target_bytes);
            let ctx = Context::new(lang);

            println!("    📏 {} ({} bytes)", config.name, text.len());

            let bench_id = format!("{}/{}", label, config.name);

            group.bench_function(BenchmarkId::new("apply", &bench_id), |b| {
                b.iter(|| black_box(stage.apply(Cow::Borrowed(&text), &ctx).unwrap()))
            });

            if stage.supports_static_fusion() {
                group.bench_function(BenchmarkId::new("fusion", &bench_id), |b| {
                    b.iter(|| {
                        let iter = stage.static_fused_adapter(text.chars(), &ctx);
                        black_box(iter.collect::<String>())
                    })
                });
            }
        }
        println!();
    }

    // Medium op - Russian
    {
        let label = "Medium op";
        let base_text = "Привет ";
        let lang = RUS;
        let stage = RemoveDiacritics;

        println!("  📊 Testing: {}", label);

        for config in LENGTH_CONFIGS {
            let text = generate_text(base_text, config.target_bytes);
            let ctx = Context::new(lang);

            println!("    📏 {} ({} bytes)", config.name, text.len());

            let bench_id = format!("{}/{}", label, config.name);

            group.bench_function(BenchmarkId::new("apply", &bench_id), |b| {
                b.iter(|| black_box(stage.apply(Cow::Borrowed(&text), &ctx).unwrap()))
            });

            if stage.supports_static_fusion() {
                group.bench_function(BenchmarkId::new("fusion", &bench_id), |b| {
                    b.iter(|| {
                        let iter = stage.static_fused_adapter(text.chars(), &ctx);
                        black_box(iter.collect::<String>())
                    })
                });
            }
        }
        println!();
    }

    // Slow op - Vietnamese
    {
        let label = "Slow op";
        let base_text = "Việt Nam Phở̉ ";
        let lang = VIE;
        let stage = RemoveDiacritics;

        println!("  📊 Testing: {}", label);

        for config in LENGTH_CONFIGS {
            let text = generate_text(base_text, config.target_bytes);
            let ctx = Context::new(lang);

            println!("    📏 {} ({} bytes)", config.name, text.len());

            let bench_id = format!("{}/{}", label, config.name);

            group.bench_function(BenchmarkId::new("apply", &bench_id), |b| {
                b.iter(|| black_box(stage.apply(Cow::Borrowed(&text), &ctx).unwrap()))
            });

            if stage.supports_static_fusion() {
                group.bench_function(BenchmarkId::new("fusion", &bench_id), |b| {
                    b.iter(|| {
                        let iter = stage.static_fused_adapter(text.chars(), &ctx);
                        black_box(iter.collect::<String>())
                    })
                });
            }
        }
        println!();
    }

    group.finish();
}

// ============================================================================
// NEW: needs_apply vs apply comparison (the actual overhead we care about)
// ============================================================================

fn bench_needs_apply_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("needs_apply_overhead");

    println!("\n┌────────────────────────────────────────────────────────────┐");
    println!("│ 🔍 needs_apply vs apply Overhead");
    println!("└────────────────────────────────────────────────────────────┘\n");

    // Russian Transliterate
    {
        let label = "Russian Transliterate";
        let base_text = "Privet mir ";
        let lang = RUS;
        let stage = Transliterate;

        println!("  🔬 {}", label);

        for config in LENGTH_CONFIGS {
            let text = generate_text(base_text, config.target_bytes);
            let ctx = Context::new(lang);
            let bench_id = format!("{}/{}", label, config.name);

            group.bench_function(BenchmarkId::new("needs_apply", &bench_id), |b| {
                b.iter(|| black_box(stage.needs_apply(&text, &ctx).unwrap()))
            });

            group.bench_function(BenchmarkId::new("apply", &bench_id), |b| {
                b.iter(|| black_box(stage.apply(Cow::Borrowed(&text), &ctx).unwrap()))
            });
        }
    }

    // Turkish LowerCase
    {
        let label = "Turkish LowerCase";
        let base_text = "istanbul igne ";
        let lang = TUR;
        let stage = LowerCase;

        println!("  🔬 {}", label);

        for config in LENGTH_CONFIGS {
            let text = generate_text(base_text, config.target_bytes);
            let ctx = Context::new(lang);
            let bench_id = format!("{}/{}", label, config.name);

            group.bench_function(BenchmarkId::new("needs_apply", &bench_id), |b| {
                b.iter(|| black_box(stage.needs_apply(&text, &ctx).unwrap()))
            });

            group.bench_function(BenchmarkId::new("apply", &bench_id), |b| {
                b.iter(|| black_box(stage.apply(Cow::Borrowed(&text), &ctx).unwrap()))
            });
        }
    }

    // Vietnamese RemoveDiacritics
    {
        let label = "Vietnamese RemoveDiacritics";
        let base_text = "Viet Nam Pho ";
        let lang = VIE;
        let stage = RemoveDiacritics;

        println!("  🔬 {}", label);

        for config in LENGTH_CONFIGS {
            let text = generate_text(base_text, config.target_bytes);
            let ctx = Context::new(lang);
            let bench_id = format!("{}/{}", label, config.name);

            group.bench_function(BenchmarkId::new("needs_apply", &bench_id), |b| {
                b.iter(|| black_box(stage.needs_apply(&text, &ctx).unwrap()))
            });

            group.bench_function(BenchmarkId::new("apply", &bench_id), |b| {
                b.iter(|| black_box(stage.apply(Cow::Borrowed(&text), &ctx).unwrap()))
            });
        }
    }

    // English baseline
    {
        let label = "English baseline";
        let base_text = "hello world ";
        let lang = ENG;
        let stage = LowerCase;

        println!("  🔬 {}", label);

        for config in LENGTH_CONFIGS {
            let text = generate_text(base_text, config.target_bytes);
            let ctx = Context::new(lang);
            let bench_id = format!("{}/{}", label, config.name);

            group.bench_function(BenchmarkId::new("needs_apply", &bench_id), |b| {
                b.iter(|| black_box(stage.needs_apply(&text, &ctx).unwrap()))
            });

            group.bench_function(BenchmarkId::new("apply", &bench_id), |b| {
                b.iter(|| black_box(stage.apply(Cow::Borrowed(&text), &ctx).unwrap()))
            });
        }
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
        .sample_size(200)
        .noise_threshold(0.015)
        .significance_level(0.05);
    targets =
        bench_lowercase_focused,
        bench_casefold_focused,
        bench_transliterate_focused,
        bench_removediacritics_focused,
        bench_segmentwords_focused,
        bench_construction_overhead,
        bench_needs_apply_overhead,
        bench_fusion_overhead
);

criterion_main!(focused_benches);
