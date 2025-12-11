// benches/whitespace_bench.rs
#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use normy::{
    COLLAPSE_WHITESPACE_UNICODE, Normy, TRIM_WHITESPACE_UNICODE,
    context::Context,
    stage::{
        CharMapper, Stage,
        normalize_whitespace::{
            COLLAPSE_WHITESPACE, NORMALIZE_WHITESPACE_FULL, NormalizeWhitespace, TRIM_WHITESPACE,
        },
    },
};
use std::{borrow::Cow, hint::black_box};

// ── Real-world whitespace samples — deliberately messy ──────────────────────
// These are the exact kinds of inputs that appear in logs, HTML scraping,
// user-generated content, and tokenizer pipelines.

const SAMPLES: &[&str] = &[
    // --- 1. Zero-Copy Fast Paths (Crucial) ---
    // S1: Already perfectly normalized ASCII
    "hello world text",
    // S2: Empty string
    "",
    // S3: Already perfectly normalized Unicode text (no WS, no need for collapse/trim)
    "Hélló Wörld!ß",
    // S4: Single space (minimal)
    " ",
    // --- 2. ASCII Collapse/Preservation Edge Cases (Stress apply_ascii_fast) ---
    // S5: Leading/trailing mixed ASCII WS (trim + collapse)
    "   hello\tworld  \n\r   ",
    // S6: Collapse-Only, preserves \t identity (must result in 'a\tb')
    "a\t\t\tb\r\rc",
    // S7: Trim-Only, preserves internal runs (must result in 'a   b')
    "  a   b  ",
    // S8: Newline preservation (must result in "a\n\nb")
    "a\n\n b",
    // --- 3. Unicode Normalization Edge Cases (Stress apply_full) ---
    // S9: Unicode WS trim (must be empty with TRIM_WHITESPACE_UNICODE)
    "\u{00A0}\u{2003}\u{3000}",
    // S10: Mixed Unicode/ASCII collapse (must result in 'a b')
    "a\u{00A0}\t b\u{2003}\n c",
    // S11: Unicode Trim-Only (must result in 'clean\u{00A0}text')
    "\u{3000}clean\u{00A0}text\u{2003}",
    // --- 4. Stress and Allocation Tests ---
    // S12: Very long run of spaces (tests capacity & heap spill/copy)
    "start{}end", // Placeholder for 10,000 spaces run
    // S13: Single Unicode WS char (tests minimal allocation)
    "\u{00A0}",
    // S14: Non-ASCII text with embedded ASCII WS (ensures proper byte/char transition)
    "El niño\t está \u{3000} aquí.",
];

fn bench_whitespace_variants(c: &mut Criterion) {
    let mut group = c.benchmark_group("NormalizeWhitespace");

    // Generate the long space string once, outside the hot loop
    let long_spaces = format!("start{}end", " ".repeat(10_000));
    let mut samples = SAMPLES.to_vec();
    samples[12] = Box::leak(long_spaces.into_boxed_str()); // 'static leak, safe in bench

    // Reusable closure — now mutable because we mutate `group`
    let mut bench_variant = |name: &str, stage: NormalizeWhitespace| {
        let normy = Normy::builder().add_stage(stage).build();
        for &input in &samples {
            let input_len = input.len() as u64;
            group.throughput(Throughput::Bytes(input_len));

            // ── Changed input (real work) ─────────────────────────────────────
            let id_changed = BenchmarkId::new(name, format!("Changed - {input:.80}"));
            group.bench_function(id_changed, |b| {
                b.iter_batched(
                    || input,
                    |text| {
                        let result = normy.normalize(black_box(text)).unwrap();

                        // True zero-copy: same pointer + length
                        let zero_copy = matches!(result, Cow::Borrowed(s)
                            if std::ptr::eq(s.as_ptr(), text.as_ptr()) && s.len() == text.len()
                        );

                        // Return something to prevent over-optimization
                        black_box(zero_copy)
                    },
                    BatchSize::SmallInput,
                );
            });

            // ── Already-normalized input (fast-path ceiling) ─────────────────
            let normalized = normy.normalize(input).unwrap().into_owned();
            let id_unchanged = BenchmarkId::new(name, format!("Unchanged - {normalized:.80}"));
            group.bench_function(id_unchanged, |b| {
                b.iter_batched(
                    || normalized.as_str(),
                    |text| {
                        let result = normy.normalize(black_box(text)).unwrap();

                        let zero_copy = matches!(result, Cow::Borrowed(s)
                            if std::ptr::eq(s.as_ptr(), text.as_ptr()) && s.len() == text.len()
                        );

                        black_box(zero_copy)
                    },
                    BatchSize::SmallInput,
                );
            });
        }
    };

    // Run all variants
    bench_variant("FULL", NORMALIZE_WHITESPACE_FULL);
    bench_variant("COLLAPSE_ONLY", COLLAPSE_WHITESPACE);
    bench_variant("TRIM_ONLY", TRIM_WHITESPACE);
    bench_variant("TRIM_UNICODE", TRIM_WHITESPACE_UNICODE);
    bench_variant("COLLAPSE_UNICODE", COLLAPSE_WHITESPACE_UNICODE);

    // Prove the value of the flag — this should be noticeably faster on ASCII-only
    bench_variant(
        "CUSTOM_no_unicode",
        NormalizeWhitespace {
            collapse: true,
            trim: true,
            normalize_unicode: false,
            replacement_char: ' ',
        },
    );

    group.finish();
}

fn bench_bind_vs_apply(c: &mut Criterion) {
    let mut group = c.benchmark_group("NormalizeWhitespace_Bind_vs_Apply");
    let ctx = Context::default();

    // Generate the long space string once
    let long_spaces = format!("start{}end", " ".repeat(10_000));
    let mut samples = SAMPLES.to_vec();
    // Safety: The original code uses this Box::leak, we must do the same.
    samples[12] = Box::leak(long_spaces.into_boxed_str());

    // ── New reusable closure for BIND vs APPLY comparison ─────────────────────
    let mut bench_bind_apply_variant = |name: &str, stage: NormalizeWhitespace| {
        for &input in &samples {
            let input_len = input.len() as u64;
            group.throughput(Throughput::Bytes(input_len));

            // 1. Benchmark: Via stage.apply()
            // Measures the dedicated String allocation/write path (e.g., apply_full or apply_ascii_fast)
            let id_apply = BenchmarkId::new(format!("{name}/APPLY"), format!("{input:.80}"));
            group.bench_function(id_apply, |b| {
                b.iter_batched(
                    || input,
                    |text| {
                        let cow = Cow::Borrowed(black_box(text));
                        let result = stage.apply(cow, &ctx).unwrap();
                        black_box(result.len())
                    },
                    BatchSize::SmallInput,
                );
            });

            // 2. Benchmark: Via stage.bind().collect()
            // Measures the Iterator-based path (e.g., WhitespaceCollapseIter) followed by collection
            let id_bind = BenchmarkId::new(format!("{name}/BIND_COLLECT"), format!("{input:.80}"));
            group.bench_function(id_bind, |b| {
                b.iter_batched(
                    || input,
                    |text| {
                        // Call bind and collect into a String
                        let result: String = stage.bind(black_box(text), &ctx).collect();
                        black_box(result.len())
                    },
                    BatchSize::SmallInput,
                );
            });
        }
    };
    // ─────────────────────────────────────────────────────────────────────────

    // Run the comparisons for multiple configurations to ensure all paths are tested
    bench_bind_apply_variant("FULL", NORMALIZE_WHITESPACE_FULL);
    bench_bind_apply_variant("COLLAPSE_UNICODE", COLLAPSE_WHITESPACE_UNICODE);
    bench_bind_apply_variant("TRIM_UNICODE", TRIM_WHITESPACE_UNICODE);
    bench_bind_apply_variant("COLLAPSE_ASCII", COLLAPSE_WHITESPACE);

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(std::time::Duration::from_secs(2))
        .warm_up_time(std::time::Duration::from_secs(2))
        .sample_size(500)
        .noise_threshold(0.02)
        .significance_level(0.05);
    targets = bench_bind_vs_apply
}

criterion_main!(benches);
