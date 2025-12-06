// benches/whitespace_bench.rs
#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use normy::{
    Normy,
    stage::normalize_whitespace::{
        COLLAPSE_WHITESPACE_ONLY, NORMALIZE_WHITESPACE_FULL, NormalizeWhitespace,
        TRIM_WHITESPACE_ONLY,
    },
};
use std::{borrow::Cow, hint::black_box};

// ── Real-world whitespace samples — deliberately messy ──────────────────────
// These are the exact kinds of inputs that appear in logs, HTML scraping,
// user-generated content, and tokenizer pipelines.

const SAMPLES: &[&str] = &[
    // 1. Leading/trailing + mixed ASCII whitespace
    "   hello\tworld  \n\r   ",
    // 2. Unicode whitespace soup (NBSP, EM SPACE, IDEOGRAPHIC SPACE, etc.)
    "hello\u{00A0}\u{2003}\u{3000}world\u{2009}\u{202F}test",
    // 3. Sequential collapse required
    "a   b\t\tc\n\nd   e",
    // 4. Already perfectly normalized → must be 100% zero-copy
    "hello world",
    // 5. Only trimming needed
    "  clean text  ",
    // 6. Only collapse needed (no trim, no unicode)
    "a  b   c    d",
    // 7. Mixed unicode + ASCII + line separators
    "line1\nline2\u{00A0}\u{2007}\u{2028}line3",
    // 8. Very long run of spaces — stress capacity hinting & allocation path
    // Generated at runtime, not in const context
    "", // placeholder — will be replaced in setup
    // 9. Edge cases
    "",
    "unchanged",
    "no_whitespace_here!",
];

fn bench_whitespace_variants(c: &mut Criterion) {
    let mut group = c.benchmark_group("NormalizeWhitespace");

    // Generate the long space string once, outside the hot loop
    let long_spaces = format!("start{}end", " ".repeat(10_000));
    let mut samples = SAMPLES.to_vec();
    samples[7] = Box::leak(long_spaces.into_boxed_str()); // 'static leak, safe in bench

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
    bench_variant("COLLAPSE_ONLY", COLLAPSE_WHITESPACE_ONLY);
    bench_variant("TRIM_ONLY", TRIM_WHITESPACE_ONLY);

    // Prove the value of the flag — this should be noticeably faster on ASCII-only
    bench_variant(
        "CUSTOM_no_unicode",
        NormalizeWhitespace {
            collapse_sequential: true,
            trim_edges: true,
            normalize_unicode: false,
        },
    );

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(std::time::Duration::from_secs(2))
        .warm_up_time(std::time::Duration::from_secs(2))
        .sample_size(500)           // High for stable zero-copy stats
        .noise_threshold(0.02)
        .significance_level(0.05);
    targets = bench_whitespace_variants
}

criterion_main!(benches);
