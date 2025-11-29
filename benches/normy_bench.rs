// benches/normy_bench_clean.rs
// Cleaned-up Criterion benchmark for Normy with:
//  - Correct per-test throughput (based on actual input size)
//  - Per-test zero-copy stats (not aggregated across cases)
//  - Dedicated zero-copy microbench
//  - More realistic non-repetitive corpus generator (deterministic)
//  - Rebalanced Unidecode vs Normy fairness (same inputs per-case)
//  - Flamegraph/run template included below
//
// Notes for use:
// - Add these dev-dependencies to Cargo.toml: criterion, rand, unicode-normalization, unidecode
// - Ensure `normy` is available as a dependency (path or crate)
// - Build and run with `cargo bench --bench normy_bench_clean`

#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::borrow::Cow;
use std::hint::black_box;

// Import your library's public surface used in the original bench.
use normy::{
    CaseFold, DEU, ENG, LowerCase, NFC, NORMALIZE_WHITESPACE_FULL, Normy, NormyBuilder,
    SegmentWords, StripHtml, TRIM_WHITESPACE_ONLY, TUR, UnifyWidth, lang::Lang, process::Process,
};
use unicode_normalization::UnicodeNormalization;

// ── Corpus generator (deterministic, less repetitive) ──
fn realistic_corpus(seed: u64, size_kb: usize) -> String {
    // A small pool of varied sentences covering multiple scripts and punctuation.
    const POOL: &[&str] = &[
        "Hello, world!", // ASCII
        "This is a test sentence for bench.",
        "déjà vu café naïve — accents and dash.", // latin + accents
        "İstanbul'da büyük ŞOK! İiIı göz göze.",  // Turkish
        "Größe Straße fußball ßẞ ÄÖÜäöü Maßstab.", // German
        "　全角スペースと半角 space が混在　こんにちは世界！", // Japanese (with fullwidth)
        "你好，世界！Ｈｅｌｌｏ　Ｗｏｒｌｄ",     // Chinese + Fullwidth Latin
        "ﬁligree ﬂag ﬁﬀﬃﬃ — ligature soup",       // ligatures
        "Emoji 👍🏼 and other symbols ✨🚀",        // emojis
        "Numbers: 1234567890 and separators —,.;:", // numbers
    ];

    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = String::with_capacity(size_kb * 1024);
    while out.len() < size_kb * 1024 {
        let i = rng.random_range(0..POOL.len());
        // Add small random punctuation or repetition to increase realism
        let repeat = rng.random_range(1..4);
        for _ in 0..repeat {
            out.push_str(POOL[i]);
            out.push(' ');
        }
        // Occasionally add a long word of mixed-case latin to simulate identifiers
        if rng.random_bool(0.05) {
            let word_len = rng.random_range(8..32);
            for _ in 0..word_len {
                let c = (b'a' + (rng.random::<u8>() % 26)) as char;
                out.push(c);
            }
            out.push(' ');
        }
    }
    out.truncate(size_kb * 1024);
    out
}

fn homoglyph_storm() -> String {
    // Keep this as a stress input, but not overly repetitive: 200KB
    let sample = "A Α А Ꭺ ᗅ ᴀ ꓮ Ａ 𐊠 𝐀 𝐴 𝑨 𝒜 𝓐 𝔄 𝔸 𝕬 𝖠 𝗔 𝘈 𝘼 𝙰 𝚨 𝛢 𝜜 𝝖 𝞐 café ﬁﬀﬃﬃ";
    sample.repeat(1_300) // ~200KB-ish depending on sample
}

// ── Pipelines (preserve intent) ──
fn search_pipeline(lang: Lang) -> Normy<impl Process> {
    NormyBuilder::default()
        .lang(lang)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .add_stage(SegmentWords)
        .build()
}

fn display_pipeline(lang: Lang) -> Normy<impl Process> {
    NormyBuilder::default()
        .lang(lang)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(StripHtml)
        .add_stage(UnifyWidth)
        .add_stage(TRIM_WHITESPACE_ONLY)
        .build()
}

// ── Baselines ──
fn unicode_nfc(text: &str) -> String {
    text.nfc().collect()
}
fn unicode_nfkc(text: &str) -> String {
    text.nfkc().collect()
}
fn unidecode_baseline(text: &str) -> String {
    unidecode::unidecode(text)
}

// ── Zero-Copy Tracker (per-case / deterministic) ──
#[derive(Default)]
struct ZeroCopyTracker {
    hits: usize,
    total: usize,
}
impl ZeroCopyTracker {
    #[allow(clippy::ptr_arg)]
    fn record(&mut self, input: &str, output: &Cow<'_, str>) {
        self.total += 1;
        if matches!(output, Cow::Borrowed(s) if s.as_ptr() == input.as_ptr() && s.len() == input.len())
        {
            self.hits += 1;
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn hit_rate_pct(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.hits as f64 / self.total as f64) * 100.0
        }
    }
}

// ── Bench: Main suite (preserves original tests, but with per-case throughput & per-case tracker) ──
fn benches_main(c: &mut Criterion) {
    let mut group = c.benchmark_group("Normy vs Baselines (clean)");

    // Generate corpora: realistic mixed (1.5MB), storm (~200KB)
    let mixed = realistic_corpus(0xDEAD_BEEF, 1536); // 1536 KB ~ 1.5MB
    let storm = homoglyph_storm();

    // Cases: pair each text with the Lang and a label — keep original intent
    let cases = [
        (&mixed as &str, ENG, "EN Mixed"),
        (&mixed as &str, TUR, "TR Locale (İ/i)"),
        (&mixed as &str, DEU, "DE ß→ss CaseFold"),
        (&storm as &str, ENG, "Homoglyph Storm NFKC"),
    ];

    for &(text, lang, name) in &cases {
        // Build pipeline and baseline(s)
        let pipeline = search_pipeline(lang);
        let unidecode_name = format!("unidecode (same input) / {name}");

        // Set throughput to the actual byte size of the input for accurate MiB/s
        group.throughput(Throughput::Bytes(text.len() as u64));

        // Zero-copy tracker dedicated to this case
        let mut tracker = ZeroCopyTracker::default();

        // Normy Search bench for this case
        group.bench_function(format!("Normy Search/{name}"), |b| {
            b.iter(|| {
                let result = pipeline.normalize(black_box(text)).expect("normy failed");
                tracker.record(text, &result);
                result
            });
        });

        // Unidecode baseline on the same input to keep fair
        group.bench_function(unidecode_name, |b| {
            b.iter(|| unidecode_baseline(black_box(text)));
        });

        // Collect stats *before* printing (borrow checker friendly)
        let hits = tracker.hits;
        let total = tracker.total;
        let rate = tracker.hit_rate_pct();

        // Report
        println!("Case: {name}  →  ZERO-COPY HIT RATE: {rate:.2}% ({hits}/{total})");
    }

    // Display bench (use mixed corpus)
    group.throughput(Throughput::Bytes(mixed.len() as u64));
    let display = display_pipeline(ENG);
    let mut display_tracker = ZeroCopyTracker::default();

    group.bench_function("Normy Display (HTML+CJK+Trim)", |b| {
        b.iter(|| {
            let result = display
                .normalize(black_box(&mixed))
                .expect("display failed");
            display_tracker.record(&mixed, &result);
            result
        });
    });

    // Report display stats
    let dhits = display_tracker.hits;
    let dtotal = display_tracker.total;
    let drate = display_tracker.hit_rate_pct();

    println!("Display ZERO-COPY HIT RATE: {drate:.2}% ({dhits}/{dtotal})");

    // Unicode-normalization NFC on mixed
    group.throughput(Throughput::Bytes(mixed.len() as u64));
    group.bench_function("unicode-normalization NFC", |b| {
        b.iter(|| unicode_nfc(black_box(&mixed)));
    });

    // Unicode-normalization NFKC on storm
    group.throughput(Throughput::Bytes(storm.len() as u64));
    group.bench_function("unicode-normalization NFKC", |b| {
        b.iter(|| unicode_nfkc(black_box(&storm)));
    });

    // Unidecode on mixed (explicit aggregate bench too)
    group.throughput(Throughput::Bytes(mixed.len() as u64));
    group.bench_function("unidecode (Rust) - mixed", |b| {
        b.iter(|| unidecode_baseline(black_box(&mixed)));
    });

    group.finish();
}

// Inputs that *should* be eligible for zero-copy if the pipeline is conservative:
const ASCII_SAFE: &str =
    "Hello simple ascii no accents 12345 - Keep this lightweight and already-normalized";
// NFC-clean composed latin characters (no decomposition needed)
const LATIN_COMPOSED: &str = "Café with precomposed e-acute (U+00E9) and simple ASCII suffix";
// ── Dedicated zero-copy microbench
fn bench_zero_copy_micro(c: &mut Criterion) {
    let mut group = c.benchmark_group("Normy Zero-Copy Microbench");

    // Minimal pipeline to allow zero-copy on clean ASCII
    let fast_ascii_pipeline = NormyBuilder::default()
        .lang(ENG)
        .add_stage(TRIM_WHITESPACE_ONLY) // no transformations
        .build();

    let pipeline = NormyBuilder::default()
        .lang(ENG)
        .add_stage(NFC)
        .add_stage(LowerCase)
        // Avoid CaseFold to allow zero-copy hits
        .add_stage(TRIM_WHITESPACE_ONLY)
        .build();

    // Track separately
    let mut tracker_ascii = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(ASCII_SAFE.len() as u64));
    group.bench_function("zero-copy / ascii-safe", |b| {
        b.iter(|| {
            let r = pipeline
                .normalize(black_box(ASCII_SAFE))
                .expect("normy failed");
            tracker_ascii.record(ASCII_SAFE, &r);
            r
        });
    });
    println!(
        "Zero-copy ascii-safe hit rate: {rate:.2}% ({hits}/{total})",
        rate = tracker_ascii.hit_rate_pct(),
        hits = tracker_ascii.hits,
        total = tracker_ascii.total
    );

    // Fast-path Normy Search for clean ASCII (should hit zero-copy)
    let mut tracker_fast = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(ASCII_SAFE.len() as u64));
    group.bench_function("Normy Search fast-path / ascii-safe", |b| {
        b.iter(|| {
            let r = fast_ascii_pipeline
                .normalize(black_box(ASCII_SAFE))
                .expect("normy failed");
            tracker_fast.record(ASCII_SAFE, &r);
            r
        });
    });
    println!(
        "Fast-path Normy Search zero-copy hit rate: {rate:.2}% ({hits}/{total})",
        rate = tracker_fast.hit_rate_pct(),
        hits = tracker_fast.hits,
        total = tracker_fast.total
    );

    let mut tracker_composed = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(LATIN_COMPOSED.len() as u64));
    group.bench_function("zero-copy / composed-latin", |b| {
        b.iter(|| {
            let r = pipeline
                .normalize(black_box(LATIN_COMPOSED))
                .expect("normy failed");
            tracker_composed.record(LATIN_COMPOSED, &r);
            r
        });
    });
    println!(
        "Zero-copy composed-latin hit rate: {rate:.2}% ({hits}/{total})",
        rate = tracker_composed.hit_rate_pct(),
        hits = tracker_composed.hits,
        total = tracker_composed.total
    );

    group.finish();
}

// ── Criterion harness
criterion_group!(benches, benches_main, bench_zero_copy_micro);
criterion_main!(benches);

/*
--- Flamegraph & profiling template ---

Recommended approaches depending on your environment:

1) cargo-flamegraph (easy, linux):
   - Install: `cargo install flamegraph` (requires perf)
   - Run: `cargo flamegraph --bin <your-binary>` or for benches:
     `cargo +nightly bench --bench normy_bench_clean` then use the produced binary under target to perf-record.

2) perf + FlameGraph script (Linux):
   - Build release bench binary: `cargo bench --bench normy_bench_clean -- --nocapture --profile release`
   - Locate the benchmark binary under `target/release/deps/` (it is produced by criterion)
   - Record: `sudo perf record -F 99 -g -- target/release/deps/<bench-binary>`
   - Generate: `sudo perf script | /path/to/FlameGraph/stackcollapse-perf.pl | /path/to/FlameGraph/flamegraph.pl > flamegraph.svg`

3) macOS (Instruments) or Windows (Windows Performance Recorder):
   - Use OS-native profilers to sample the `target/release/deps/<bench-binary>` while running the bench.

Notes:
 - Criterion runs the benchmarks many times. When profiling you may want to run the bench a few times and target the specific worst-case function with a microbench.
 - Another approach is to add `#[inline(never)]` to candidate functions temporarily and microbenchmark them directly so call stacks are clearer.
 - If you want, I can generate a short script that finds the correct bench binary by pattern and runs perf/FlameGraph for you.
*/
