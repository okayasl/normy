use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::context::Context;
use normy::stage::{Stage, StaticFusableStage};
use normy::{
    COLLAPSE_WHITESPACE, COLLAPSE_WHITESPACE_UNICODE, ENG, NFC, NFD, NFKC, NFKD,
    NORMALIZE_WHITESPACE_FULL, NormalizePunctuation, Normy, StripControlChars, StripHtml,
    TRIM_WHITESPACE, TRIM_WHITESPACE_UNICODE, UnifyWidth,
};
use std::{borrow::Cow, hint::black_box, time::Duration};

// ============================================================================
// Base Text Samples (Short patterns for repetition)
// ============================================================================
const TEXT_MIXED_WIDTH_CTRL: &str = "Ｈｅｌｌｏ\u{0000}ｗｏｒｌｄ ";
const TEXT_HTML_ACCENTS: &str = "<b>Hello naïve Café</b> <script>alert(1)</script> ";
const TEXT_PUNCTUATION: &str = "Hello⋯world... café!! ";
const TEXT_UNI_WHITESPACE: &str = "Hello\u{3000}world\u{2028}café ";
const TEXT_FULLWIDTH: &str = "ＦＵＬＬＷＩＤＴＨ ";
const TEXT_COMPATIBILITY: &str = "ﬁle ½ ① ﬁﬀ ";
const TEXT_PADDING: &str = "    lots of padding    ";

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
// FIXED: Restructured Benchmark Function
// ============================================================================
fn bench_stage_length_scaling<S, C>(
    c: &mut Criterion,
    stage_name: &str,
    constructor: C,
    base_input: &str,
) where
    S: Stage + StaticFusableStage + 'static,
    C: Fn() -> S + Copy,
{
    let mut group = c.benchmark_group(format!("{}_length_scaling", stage_name));
    let lang = ENG;

    println!("\n┌────────────────────────────────────────────────────────────┐");
    println!("│ 📊 {} - Length Scaling Benchmark", stage_name);
    println!("└────────────────────────────────────────────────────────────┘\n");

    for config in LENGTH_CONFIGS {
        let input = generate_text(base_input, config.target_bytes);
        let actual_len = input.len();

        println!(
            "  📏 {} ({} bytes - {})",
            config.name, actual_len, config.description
        );

        // Pre-calculate normalized result
        let ctx = Context::new(lang);
        let normalized = {
            let stage = constructor();
            stage
                .apply(Cow::Borrowed(&input), &ctx)
                .unwrap()
                .into_owned()
        };

        let is_unchanged = input == normalized.as_str();
        let status = if is_unchanged { "unchanged" } else { "changed" };
        let supports_fusion = constructor().supports_static_fusion();

        let bench_id = format!("{}/{}/{}", stage_name, config.name, status);

        // ═══════════════════════════════════════════════════════════
        // CHANGED INPUT - Pre-construct outside timing loop
        // ═══════════════════════════════════════════════════════════

        // Pipeline benchmark - FIXED
        {
            let stage = constructor();
            let normy = Normy::builder().lang(lang).add_stage(stage).build();

            group.bench_function(BenchmarkId::new("pipeline", &bench_id), |b| {
                b.iter(|| black_box(normy.normalize(&input).unwrap()))
            });
        }

        // Apply benchmark - FIXED
        {
            let stage = constructor();
            let ctx = Context::new(lang);

            group.bench_function(BenchmarkId::new("apply", &bench_id), |b| {
                b.iter(|| black_box(stage.apply(Cow::Borrowed(&input), &ctx).unwrap()))
            });
        }

        // Fusion benchmark - FIXED
        if supports_fusion {
            let stage = constructor();
            let ctx = Context::new(lang);

            group.bench_function(BenchmarkId::new("fusion", &bench_id), |b| {
                b.iter(|| {
                    let iter = stage.static_fused_adapter(input.chars(), &ctx);
                    black_box(iter.collect::<String>())
                })
            });
        }

        // ═══════════════════════════════════════════════════════════
        // UNCHANGED INPUT - Tests short-circuiting
        // ═══════════════════════════════════════════════════════════

        if !is_unchanged {
            let unchanged_bench_id = format!("{}/{}/unchanged", stage_name, config.name);

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

                group.bench_function(BenchmarkId::new("fusion", &unchanged_bench_id), |b| {
                    b.iter(|| {
                        let iter = stage.static_fused_adapter(normalized.chars(), &ctx);
                        black_box(iter.collect::<String>())
                    })
                });
            }
        }
    }

    println!();
    group.finish();
}

// ============================================================================
// Individual Stage Benchmarks
// ============================================================================

fn bench_unify_width(c: &mut Criterion) {
    bench_stage_length_scaling(c, "UnifyWidth", || UnifyWidth, TEXT_FULLWIDTH);
}

fn bench_nfc(c: &mut Criterion) {
    bench_stage_length_scaling(c, "NFC", || NFC, TEXT_HTML_ACCENTS);
}

fn bench_nfd(c: &mut Criterion) {
    bench_stage_length_scaling(c, "NFD", || NFD, TEXT_HTML_ACCENTS);
}

fn bench_nfkc(c: &mut Criterion) {
    bench_stage_length_scaling(c, "NFKC", || NFKC, TEXT_COMPATIBILITY);
}

fn bench_nfkd(c: &mut Criterion) {
    bench_stage_length_scaling(c, "NFKD", || NFKD, TEXT_COMPATIBILITY);
}

fn bench_punct(c: &mut Criterion) {
    bench_stage_length_scaling(c, "Punctuation", || NormalizePunctuation, TEXT_PUNCTUATION);
}

fn bench_strip_ctrl(c: &mut Criterion) {
    bench_stage_length_scaling(c, "StripCtrl", || StripControlChars, TEXT_MIXED_WIDTH_CTRL);
}

fn bench_strip_html(c: &mut Criterion) {
    bench_stage_length_scaling(c, "StripHtml", || StripHtml, TEXT_HTML_ACCENTS);
}

fn bench_ws_full(c: &mut Criterion) {
    bench_stage_length_scaling(
        c,
        "WS_Full",
        || NORMALIZE_WHITESPACE_FULL,
        TEXT_UNI_WHITESPACE,
    );
}

fn bench_ws_collapse(c: &mut Criterion) {
    bench_stage_length_scaling(c, "WS_Collapse", || COLLAPSE_WHITESPACE, TEXT_PADDING);
}

fn bench_ws_collapse_uni(c: &mut Criterion) {
    bench_stage_length_scaling(
        c,
        "WS_Collapse_Uni",
        || COLLAPSE_WHITESPACE_UNICODE,
        TEXT_UNI_WHITESPACE,
    );
}

fn bench_ws_trim(c: &mut Criterion) {
    bench_stage_length_scaling(c, "WS_Trim", || TRIM_WHITESPACE, TEXT_PADDING);
}

fn bench_ws_trim_uni(c: &mut Criterion) {
    bench_stage_length_scaling(c, "WS_Trim_Uni", || TRIM_WHITESPACE_UNICODE, TEXT_PADDING);
}

// ============================================================================
// Criterion Group
// ============================================================================

criterion_group!(
    name = agnostic_benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(3))
        .warm_up_time(Duration::from_secs(1))
        .sample_size(100);
    targets =
        bench_unify_width, bench_nfc, bench_nfd, bench_nfkc, bench_nfkd,
        bench_punct, bench_strip_ctrl, bench_strip_html,
        bench_ws_full, bench_ws_collapse, bench_ws_collapse_uni,
        bench_ws_trim, bench_ws_trim_uni
);

criterion_main!(agnostic_benches);
