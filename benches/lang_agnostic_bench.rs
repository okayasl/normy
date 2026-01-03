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
// Named Meaningful Samples
// ============================================================================

const TEXT_MIXED_WIDTH_CTRL: &str = "Ｈｅｌｌｏ\u{0000}ｗｏｒｌｄ";
const TEXT_HTML_ACCENTS: &str = "<b>Hello naïve Café</b> <script>alert(1)</script>";
const TEXT_PUNCTUATION: &str = "Hello---world... café!!";
const TEXT_UNI_WHITESPACE: &str = "Hello\u{3000}world\u{2028}café";
const TEXT_FULLWIDTH: &str = "ＦＵＬＬＷＩＤＴＨ";
const TEXT_COMPATIBILITY: &str = "ﬁle ½ ① ﬁﬀ";
const TEXT_PADDING: &str = "    lots of padding    ";

// ============================================================================
// Utility & Core Logic
// ============================================================================

fn sanitize_id(s: &str) -> String {
    let mut cleaned = String::with_capacity(s.len());
    for c in s.chars().take(20) {
        match c {
            '\0' => cleaned.push_str("[NUL]"),
            c if c.is_ascii_control() => cleaned.push('.'),
            _ => cleaned.push(c),
        }
    }
    cleaned
}

fn bench_stage_focused<S, C>(c: &mut Criterion, stage_name: &str, constructor: C, input: &str)
where
    S: Stage + StaticFusableStage + 'static,
    C: Fn() -> S + Copy,
{
    let mut group = c.benchmark_group(stage_name);
    let lang = ENG;
    let ctx = Context::new(lang);

    // 1. PRE-CALCULATION: "Retrieve" the result for the second bench
    let stage = constructor();
    let normalized = stage
        .apply(Cow::Borrowed(input), &ctx)
        .unwrap()
        .into_owned();
    let is_unchanged = input == normalized.as_str();

    let mut run_suite = |label: &str, text: &str| {
        let safe_id = sanitize_id(text);

        // PIPELINE Bench
        group.bench_function(
            BenchmarkId::new(format!("{label}/normy pipeline"), &safe_id),
            |b| {
                b.iter(|| {
                    let s = constructor();
                    let normy = Normy::builder().lang(lang).add_stage(s).build();
                    black_box(normy.normalize(text).unwrap().into_owned())
                })
            },
        );

        // APPLY Bench
        group.bench_function(BenchmarkId::new(format!("{label}/apply"), &safe_id), |b| {
            b.iter(|| {
                let s = constructor();
                black_box(s.apply(Cow::Borrowed(text), &ctx).unwrap())
            })
        });

        // FUSION Bench
        if stage.supports_static_fusion() {
            group.bench_function(BenchmarkId::new(format!("{label}/fusion"), &safe_id), |b| {
                b.iter(|| {
                    let s = constructor();
                    let iter = s.static_fused_adapter(text.chars(), &ctx);
                    black_box(iter.collect::<String>())
                })
            });
        }
    };

    // First bench: The input that triggers logic
    run_suite("changed", input);

    // Second bench: The result of the first bench (tests short-circuiting)
    if !is_unchanged {
        run_suite("unchanged", &normalized);
    }

    group.finish();
}

// ============================================================================
// Macro & Target Registration
// ============================================================================

macro_rules! register_stage_bench {
    ($fn_name:ident, $name_str:expr, $constructor:expr, $input_text:expr) => {
        fn $fn_name(c: &mut Criterion) {
            bench_stage_focused(c, $name_str, || $constructor, $input_text);
        }
    };
}

register_stage_bench!(bench_unify_width, "UnifyWidth", UnifyWidth, TEXT_FULLWIDTH);
register_stage_bench!(bench_nfc, "NFC", NFC, TEXT_HTML_ACCENTS);
register_stage_bench!(bench_nfd, "NFD", NFD, TEXT_HTML_ACCENTS);
register_stage_bench!(bench_nfkc, "NFKC", NFKC, TEXT_COMPATIBILITY);
register_stage_bench!(bench_nfkd, "NFKD", NFKD, TEXT_COMPATIBILITY);
register_stage_bench!(
    bench_punct,
    "Punctuation",
    NormalizePunctuation,
    TEXT_PUNCTUATION
);
register_stage_bench!(
    bench_strip_ctrl,
    "StripCtrl",
    StripControlChars,
    TEXT_MIXED_WIDTH_CTRL
);
register_stage_bench!(bench_strip_html, "StripHtml", StripHtml, TEXT_HTML_ACCENTS);
register_stage_bench!(
    bench_ws_full,
    "WS_Full",
    NORMALIZE_WHITESPACE_FULL,
    TEXT_UNI_WHITESPACE
);
register_stage_bench!(
    bench_collapse,
    "WS_Collapse",
    COLLAPSE_WHITESPACE,
    TEXT_PADDING
);
register_stage_bench!(
    bench_collapse_uni,
    "WS_Collapse_Uni",
    COLLAPSE_WHITESPACE_UNICODE,
    TEXT_UNI_WHITESPACE
);
register_stage_bench!(bench_trim, "WS_Trim", TRIM_WHITESPACE, TEXT_PADDING);
register_stage_bench!(
    bench_trim_uni,
    "WS_Trim_Uni",
    TRIM_WHITESPACE_UNICODE,
    TEXT_PADDING
);

// ============================================================================
// Criterion Group
// ============================================================================

criterion_group!(
    name = agnostic_benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(2))
        .warm_up_time(Duration::from_secs(1))
        .sample_size(100);
    targets =
        bench_unify_width, bench_nfc, bench_nfd, bench_nfkc, bench_nfkd,
        bench_punct, bench_strip_ctrl, bench_strip_html,
        bench_ws_full, bench_collapse, bench_collapse_uni,
        bench_trim, bench_trim_uni
);

criterion_main!(agnostic_benches);
