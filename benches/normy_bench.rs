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
use rand::{Rng, SeedableRng, random, rngs::StdRng};
use std::borrow::Cow;
use std::hint::black_box;

use normy::{
    CaseFold, DEU, ENG, LowerCase, NFC, NFKC, NORMALIZE_WHITESPACE_FULL, Normy, NormyBuilder,
    RemoveDiacritics, SegmentWords, StripHtml, TRIM_WHITESPACE_ONLY, TUR, Transliterate,
    UnifyWidth, lang::Lang, process::Process,
};
use unicode_normalization::UnicodeNormalization;

// ── Corpus generator ──
fn realistic_corpus(seed: u64, size_kb: usize) -> String {
    const POOL: &[&str] = &[
        "Hello, world!",
        "This is a test sentence for bench.",
        "déjà vu café naïve — accents and dash.",
        "İstanbul'da büyük ŞOK! İiIı göz göze.",
        "Größe Straße fußball ßẞ ÄÖÜäöü Maßstab.",
        "　全角スペースと半角 space が混在　こんにちは世界！",
        "你好，世界！Ｈｅｌｌｏ　Ｗｏｒｌｄ",
        "ﬁligree ﬂag ﬁﬀﬃﬃ — ligature soup",
        "Emoji 👍🏼 and other symbols ✨🚀",
        "Numbers: 1234567890 and separators —,.;:",
    ];

    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = String::with_capacity(size_kb * 1024);
    while out.len() < size_kb * 1024 {
        let i = rng.random_range(0..POOL.len());
        let repeat = rng.random_range(1..4);
        for _ in 0..repeat {
            out.push_str(POOL[i]);
            out.push(' ');
        }
        if rng.random_bool(0.05) {
            let word_len = rng.random_range(8..32);
            for _ in 0..word_len {
                let c = (b'a' + (random::<u8>() % 26)) as char;
                out.push(c);
            }
            out.push(' ');
        }
    }
    let max_len = size_kb * 1024;
    if out.len() > max_len {
        while !out.is_char_boundary(max_len) {
            // move back to previous char boundary
            out.pop();
        }
        out.truncate(max_len);
    }
    out
}

fn homoglyph_storm() -> String {
    let sample = "A Α А Ꭺ ᗅ ᴀ ꓮ Ａ 𐊠 𝐀 𝐴 𝑨 𝒜 𝓐 𝔄 𝔸 𝕬 𝖠 𝗔 𝘈 𝘼 𝙰 𝚨 𝛢 𝜜 𝝖 𝞐 café ﬁﬀﬃﬃ";
    sample.repeat(1_300)
}

// ── Pipelines ──
fn full_pipeline(lang: Lang) -> Normy<impl Process> {
    NormyBuilder::default()
        .lang(lang)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .add_stage(Transliterate)
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

fn normy_nfc_only(lang: Lang) -> Normy<impl Process> {
    NormyBuilder::default().lang(lang).add_stage(NFC).build()
}

fn normy_nfkc_only(lang: Lang) -> Normy<impl Process> {
    NormyBuilder::default().lang(lang).add_stage(NFKC).build()
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

// ── Zero-Copy Tracker ──
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

// ── Benchmarks ──
fn benches_main(c: &mut Criterion) {
    let mut group = c.benchmark_group("Normy Benchmarks");

    let mixed = realistic_corpus(0xDEAD_BEEF, 128);
    let storm = homoglyph_storm();

    let cases = [
        (&mixed as &str, ENG, "EN Mixed"),
        (&mixed as &str, TUR, "TR Locale"),
        (&mixed as &str, DEU, "DE ß→ss"),
        (&storm as &str, ENG, "Homoglyph Storm"),
    ];

    for &(text, lang, name) in &cases {
        let pipeline = full_pipeline(lang);
        let mut tracker = ZeroCopyTracker::default();
        group.throughput(Throughput::Bytes(text.len() as u64));

        group.bench_function(format!("Normy → {name}"), |b| {
            b.iter(|| {
                let r = pipeline.normalize(black_box(text)).expect("normy failed");
                tracker.record(text, &r);
                r
            });
        });

        group.bench_function(format!("Normy NFC → {name}"), |b| {
            let nfc_pipeline = normy_nfc_only(lang);
            b.iter(|| {
                nfc_pipeline
                    .normalize(black_box(text))
                    .expect("normy NFC failed")
            });
        });
        group.bench_function(format!("Normy NFKC → {name}"), |b| {
            let nfkc_pipeline = normy_nfkc_only(lang);
            b.iter(|| {
                nfkc_pipeline
                    .normalize(black_box(text))
                    .expect("normy NFKC failed")
            });
        });

        group.bench_function(format!("Unidecode → {name}"), |b| {
            b.iter(|| unidecode_baseline(black_box(text)));
        });

        group.bench_function(format!("Unicode NFC → {name}"), |b| {
            b.iter(|| unicode_nfc(black_box(text)));
        });

        group.bench_function(format!("Unicode NFKC → {name}"), |b| {
            b.iter(|| unicode_nfkc(black_box(text)));
        });

        println!(
            "Case: {name} → ZERO-COPY HIT RATE: {:.2}% ({}/{})",
            tracker.hit_rate_pct(),
            tracker.hits,
            tracker.total
        );
    }

    let display = display_pipeline(ENG);
    let mut display_tracker = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(mixed.len() as u64));
    group.bench_function("Normy Display (HTML+CJK+Trim)", |b| {
        b.iter(|| {
            let r = display
                .normalize(black_box(&mixed))
                .expect("display failed");
            display_tracker.record(&mixed, &r);
            r
        });
    });
    println!(
        "Display ZERO-COPY HIT RATE: {:.2}% ({}/{})",
        display_tracker.hit_rate_pct(),
        display_tracker.hits,
        display_tracker.total
    );

    group.finish();
}

// ── Zero-copy microbench ──
const ASCII_SAFE: &str = "Hello simple ascii no accents 12345 - Keep this lightweight";
const LATIN_COMPOSED: &str = "Café with precomposed e-acute (U+00E9) and ASCII";

fn bench_zero_copy_micro(c: &mut Criterion) {
    let mut group = c.benchmark_group("Normy Zero-Copy Microbench");

    let fast_pipeline = NormyBuilder::default()
        .lang(ENG)
        .add_stage(TRIM_WHITESPACE_ONLY)
        .build();

    let pipeline = NormyBuilder::default()
        .lang(ENG)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(TRIM_WHITESPACE_ONLY)
        .build();

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
        "Zero-copy ascii-safe hit rate: {:.2}% ({}/{})",
        tracker_ascii.hit_rate_pct(),
        tracker_ascii.hits,
        tracker_ascii.total
    );

    let mut tracker_fast = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(ASCII_SAFE.len() as u64));
    group.bench_function("Normy Search fast-path / ascii-safe", |b| {
        b.iter(|| {
            let r = fast_pipeline
                .normalize(black_box(ASCII_SAFE))
                .expect("normy failed");
            tracker_fast.record(ASCII_SAFE, &r);
            r
        });
    });
    println!(
        "Fast-path Normy zero-copy hit rate: {:.2}% ({}/{})",
        tracker_fast.hit_rate_pct(),
        tracker_fast.hits,
        tracker_fast.total
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
        "Zero-copy composed-latin hit rate: {:.2}% ({}/{})",
        tracker_composed.hit_rate_pct(),
        tracker_composed.hits,
        tracker_composed.total
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
