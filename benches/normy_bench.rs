// benches/normy_bench_improved.rs
// Improved Criterion benchmark for Normy with:
//  - Correct zero-copy expectations and test cases
//  - Separate benchmarks for "needs transformation" vs "already normalized"
//  - More realistic corpus with configurable normalization ratio
//  - Per-stage pipeline benchmarks to identify bottlenecks
//  - Better tracking and reporting of zero-copy behavior
//
// Notes for use:
// - Add these dev-dependencies to Cargo.toml: criterion, rand, unicode-normalization, unidecode
// - Ensure `normy` is available as a dependency (path or crate)
// - Build and run with `cargo bench --bench normy_bench_improved`

#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use icu_normalizer::ComposingNormalizerBorrowed;
use normy::process::BuildIter;
use rand::{Rng, SeedableRng, random, rngs::StdRng};
use regex::Regex;
use std::hint::black_box;
use std::{borrow::Cow, sync::LazyLock};

use tokenizers::{
    NormalizedString, Normalizer,
    normalizers::{Lowercase, Sequence, StripAccents, unicode::NFC as tokenizerNFC},
};

use normy::{
    CaseFold, DEU, ENG, LowerCase, NFC, NFKC, NORMALIZE_WHITESPACE_FULL, Normy, NormyBuilder,
    RemoveDiacritics, SegmentWords, StripHtml, TRIM_WHITESPACE, TUR, Transliterate, UnifyWidth,
    lang::Lang,
};
use unicode_normalization::UnicodeNormalization;

// ── Corpus generators ──

/// Generate corpus with mixed content (uppercase, diacritics, special chars)
/// This corpus is EXPECTED to have low zero-copy rate with full pipeline
fn realistic_corpus_needs_transform(seed: u64, size_kb: usize) -> String {
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
    truncate_to_char_boundary(&mut out, size_kb * 1024);
    out
}

/// Generate corpus that's already normalized (lowercase ASCII, no diacritics)
/// This corpus is EXPECTED to have high zero-copy rate with full pipeline
fn realistic_corpus_already_normalized(seed: u64, size_kb: usize) -> String {
    const POOL: &[&str] = &[
        "hello world this is a test",
        "the quick brown fox jumps over the lazy dog",
        "sample text for benchmark testing purposes",
        "normalized content without special characters",
        "simple ascii text with numbers 123456",
        "another example sentence for testing",
        "common english words and phrases here",
        "basic text processing benchmark data",
    ];

    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = String::with_capacity(size_kb * 1024);
    while out.len() < size_kb * 1024 {
        let i = rng.random_range(0..POOL.len());
        let repeat = rng.random_range(1..3);
        for _ in 0..repeat {
            out.push_str(POOL[i]);
            out.push(' ');
        }
    }
    truncate_to_char_boundary(&mut out, size_kb * 1024);
    out
}

/// Generate mixed corpus with configurable ratio of already-normalized content
/// This simulates real-world NLP workloads
/// Returns a Vec of strings so we can measure zero-copy per-string, not per-corpus
fn realistic_corpus_mixed_batch(seed: u64, count: usize, normalized_ratio: f64) -> Vec<String> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut batch = Vec::with_capacity(count);

    let normalized_pool = [
        "hello world this is a test sentence",
        "simple test for normalization checking",
        "normalized content without any uppercase",
        "basic text processing example here",
        "another lowercase example with numbers 123",
    ];

    let needs_transform_pool = [
        "Hello World This Needs Transform",
        "Another Example With Uppercase",
        "Mixed Case Content Here",
        "Testing Transformation Logic",
        "Sample Text With CAPS",
    ];

    for _ in 0..count {
        if rng.random_bool(normalized_ratio) {
            let i = rng.random_range(0..normalized_pool.len());
            batch.push(normalized_pool[i].to_string());
        } else {
            let i = rng.random_range(0..needs_transform_pool.len());
            batch.push(needs_transform_pool[i].to_string());
        }
    }

    batch
}

static NFC_NORMALIZER: LazyLock<ComposingNormalizerBorrowed<'static>> =
    LazyLock::new(ComposingNormalizerBorrowed::new_nfc);
// ICU4X: NFC (Normalization Form Canonical Composition)
fn icu4x_nfc(text: &str) -> String {
    NFC_NORMALIZER.normalize(text).to_string()
}

static NFKC_NORMALIZER: LazyLock<ComposingNormalizerBorrowed<'static>> =
    LazyLock::new(ComposingNormalizerBorrowed::new_nfkc);
// ICU4X: NFKC (Normalization Form Compatibility Composition)
fn icu4x_nfkc(text: &str) -> String {
    NFKC_NORMALIZER.normalize(text).to_string()
}

// tokenizers: Hugging Face pipeline (NFC + Lowercase + Strip accents)
fn tokenizers_normalize(text: &str) -> String {
    static NORMALIZER: LazyLock<Sequence> = LazyLock::new(|| {
        Sequence::new(vec![
            tokenizers::NormalizerWrapper::NFC(tokenizerNFC),
            tokenizers::NormalizerWrapper::Lowercase(Lowercase),
            tokenizers::NormalizerWrapper::StripAccents(StripAccents),
        ])
    });
    let mut normalized = NormalizedString::from(text.to_owned()); // owned once
    NORMALIZER
        .normalize(&mut normalized)
        .expect("tokenizers failed");
    normalized.get().to_string()
}

// Regex baseline: Simple homoglyph normalization (e.g., for "storm" corpus)
static HOMOGYLPH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ΑАᎪᗅᴀꓮＡ𐊠𝐀𝐴𝑨𝒜𝓐𝔄𝔸𝕬𝖠𝗔𝘈𝘼𝙰𝚨𝛢𝜜𝝖𝞐]").unwrap());
fn regex_homoglyph(text: &str) -> String {
    HOMOGYLPH_RE.replace_all(text, "A").to_string() // Placeholder; extend for full
}

fn homoglyph_storm() -> String {
    let sample = "A Α А Ꭺ ᗅ ᴀ ꓮ Ａ 𐊠 𝐀 𝐴 𝑨 𝒜 𝓐 𝔄 𝔸 𝕬 𝖠 𝗔 𝘈 𝘼 𝙰 𝚨 𝛢 𝜜 𝝖 𝞐 café ﬁﬀﬃﬃ";
    sample.repeat(1_300)
}

fn truncate_to_char_boundary(s: &mut String, max_len: usize) {
    if s.len() > max_len {
        while !s.is_char_boundary(max_len) && !s.is_empty() {
            s.pop();
        }
        s.truncate(max_len);
    }
}

// ── Pipelines ──
fn full_pipeline(lang: Lang) -> Normy<impl BuildIter> {
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

fn display_pipeline(lang: Lang) -> Normy<impl BuildIter> {
    NormyBuilder::default()
        .lang(lang)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(StripHtml)
        .add_stage(UnifyWidth)
        .add_stage(TRIM_WHITESPACE)
        .build()
}

fn normy_nfc_only(lang: Lang) -> Normy<impl BuildIter> {
    NormyBuilder::default().lang(lang).add_stage(NFC).build()
}

fn normy_nfkc_only(lang: Lang) -> Normy<impl BuildIter> {
    NormyBuilder::default().lang(lang).add_stage(NFKC).build()
}

// Incremental pipeline stages for bottleneck analysis
fn pipeline_nfc_only(lang: Lang) -> Normy<impl BuildIter> {
    NormyBuilder::default().lang(lang).add_stage(NFC).build()
}

fn pipeline_nfc_lowercase(lang: Lang) -> Normy<impl BuildIter> {
    NormyBuilder::default()
        .lang(lang)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .build()
}

fn pipeline_nfc_lowercase_casefold(lang: Lang) -> Normy<impl BuildIter> {
    NormyBuilder::default()
        .lang(lang)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
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

/// Main benchmark suite with correct expectations
fn benches_main(c: &mut Criterion) {
    let mut group = c.benchmark_group("Normy Full Pipeline");

    // Test 1: Content that NEEDS transformation (expect 0% zero-copy - that's correct!)
    let needs_transform = realistic_corpus_needs_transform(0xDEAD_BEEF, 128);
    let storm = homoglyph_storm();

    let cases_needs_transform = [
        (&needs_transform as &str, ENG, "EN Mixed (needs transform)"),
        (&needs_transform as &str, TUR, "TR Locale (needs transform)"),
        (&needs_transform as &str, DEU, "DE ß→ss (needs transform)"),
        (&storm as &str, ENG, "Homoglyph Storm"),
    ];

    for &(text, lang, name) in &cases_needs_transform {
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

        println!(
            "Case: {name} → ZERO-COPY: {:.2}% ({}/{}) [EXPECTED: ~0% - needs transformation]",
            tracker.hit_rate_pct(),
            tracker.hits,
            tracker.total
        );
    }

    // Test 2: Already normalized content (expect HIGH zero-copy rate)
    let already_normalized = realistic_corpus_already_normalized(0xCAFE_BABE, 128);
    let pipeline_en = full_pipeline(ENG);
    let mut tracker_normalized = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(already_normalized.len() as u64));

    group.bench_function(
        "Normy → Already Normalized (expect high zero-copy)",
        |b| {
            b.iter(|| {
                let r = pipeline_en
                    .normalize(black_box(&already_normalized))
                    .expect("normy failed");
                tracker_normalized.record(&already_normalized, &r);
                r
            });
        },
    );

    println!(
        "Already Normalized → ZERO-COPY: {:.2}% ({}/{}) [EXPECTED: ~100%]",
        tracker_normalized.hit_rate_pct(),
        tracker_normalized.hits,
        tracker_normalized.total
    );

    // Test 3: Mixed corpus (70% normalized, 30% needs transform)
    // Process each string individually to get accurate per-string zero-copy rate
    let mixed_70_batch = realistic_corpus_mixed_batch(0xBEEF_CAFE, 1000, 0.7);
    let total_bytes: usize = mixed_70_batch.iter().map(std::string::String::len).sum();
    let mut tracker_mixed = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(total_bytes as u64));

    group.bench_function("Normy → Mixed 70% Normalized (batch)", |b| {
        b.iter(|| {
            for text in &mixed_70_batch {
                let r = pipeline_en
                    .normalize(black_box(text))
                    .expect("normy failed");
                tracker_mixed.record(text, &r);
            }
        });
    });

    println!(
        "Mixed 70% (batch of {} strings) → ZERO-COPY: {:.2}% ({}/{}) [EXPECTED: ~70%]",
        mixed_70_batch.len(),
        tracker_mixed.hit_rate_pct(),
        tracker_mixed.hits,
        tracker_mixed.total
    );

    // Test 4: Display pipeline (HTML + CJK + Trim)
    let display = display_pipeline(ENG);
    let mut display_tracker = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(needs_transform.len() as u64));

    group.bench_function("Normy Display (HTML+CJK+Trim)", |b| {
        b.iter(|| {
            let r = display
                .normalize(black_box(&needs_transform))
                .expect("display failed");
            display_tracker.record(&needs_transform, &r);
            r
        });
    });

    println!(
        "Display Pipeline → ZERO-COPY: {:.2}% ({}/{}) [Content-dependent]",
        display_tracker.hit_rate_pct(),
        display_tracker.hits,
        display_tracker.total
    );

    group.finish();
}

/// Benchmark individual normalization forms
fn benches_normalization_forms(c: &mut Criterion) {
    let mut group = c.benchmark_group("Normalization Forms");

    let needs_transform = realistic_corpus_needs_transform(0xDEAD_BEEF, 128);
    let already_normalized = realistic_corpus_already_normalized(0xCAFE_BABE, 128);

    for (corpus, corpus_name) in [
        (&needs_transform, "Needs Transform"),
        (&already_normalized, "Already Normalized"),
    ] {
        group.throughput(Throughput::Bytes(corpus.len() as u64));

        // Normy implementations
        group.bench_function(BenchmarkId::new("Normy NFC", corpus_name), |b| {
            let nfc_pipeline = normy_nfc_only(ENG);
            b.iter(|| {
                nfc_pipeline
                    .normalize(black_box(corpus))
                    .expect("normy NFC failed")
            });
        });

        group.bench_function(BenchmarkId::new("Normy NFKC", corpus_name), |b| {
            let nfkc_pipeline = normy_nfkc_only(ENG);
            b.iter(|| {
                nfkc_pipeline
                    .normalize(black_box(corpus))
                    .expect("normy NFKC failed")
            });
        });

        // Baseline comparisons
        group.bench_function(BenchmarkId::new("Unicode NFC", corpus_name), |b| {
            b.iter(|| unicode_nfc(black_box(corpus)));
        });

        group.bench_function(BenchmarkId::new("Unicode NFKC", corpus_name), |b| {
            b.iter(|| unicode_nfkc(black_box(corpus)));
        });

        group.bench_function(BenchmarkId::new("Unidecode", corpus_name), |b| {
            b.iter(|| unidecode_baseline(black_box(corpus)));
        });

        group.bench_function(BenchmarkId::new("ICU4X NFC", corpus_name), |b| {
            b.iter(|| icu4x_nfc(black_box(corpus)));
        });
        group.bench_function(BenchmarkId::new("ICU4X NFKC", corpus_name), |b| {
            b.iter(|| icu4x_nfkc(black_box(corpus)));
        });

        // tokenizers baseline
        group.bench_function(BenchmarkId::new("tokenizers HF", corpus_name), |b| {
            b.iter(|| tokenizers_normalize(black_box(corpus)));
        });

        // Regex for homoglyph-specific (only on "Needs Transform")
        if corpus_name == "Needs Transform" {
            group.bench_function(BenchmarkId::new("Regex Homoglyph", corpus_name), |b| {
                b.iter(|| regex_homoglyph(black_box(corpus)));
            });
        }
    }

    group.finish();
}

/// Incremental pipeline benchmark to identify bottlenecks
fn benches_incremental_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("Pipeline Stages (Incremental)");

    let corpus = realistic_corpus_needs_transform(0xDEAD_BEEF, 128);
    group.throughput(Throughput::Bytes(corpus.len() as u64));

    let p1 = pipeline_nfc_only(ENG);
    group.bench_function("1 stage: NFC", |b| {
        b.iter(|| p1.normalize(black_box(&corpus)).expect("failed"));
    });

    let p2 = pipeline_nfc_lowercase(ENG);
    group.bench_function("2 stages: NFC + LowerCase", |b| {
        b.iter(|| p2.normalize(black_box(&corpus)).expect("failed"));
    });

    let p3 = pipeline_nfc_lowercase_casefold(ENG);
    group.bench_function("3 stages: NFC + LowerCase + CaseFold", |b| {
        b.iter(|| p3.normalize(black_box(&corpus)).expect("failed"));
    });

    let p_full = full_pipeline(ENG);
    group.bench_function("Full pipeline (7 stages)", |b| {
        b.iter(|| p_full.normalize(black_box(&corpus)).expect("failed"));
    });

    group.finish();
}
const ASCII_WITH_UPPERCASE: &str = "Hello simple ascii no accents 12345 - Keep this lightweight";
const ASCII_LOWERCASE: &str = "hello simple ascii no accents 12345 keep this lightweight";
const LATIN_COMPOSED_UPPER: &str = "Café with precomposed e-acute (U+00E9) and ASCII";
const LATIN_COMPOSED_LOWER: &str = "café with precomposed e-acute already normalized";

/// Zero-copy microbenchmark with correct expectations
fn bench_zero_copy_micro(c: &mut Criterion) {
    let mut group = c.benchmark_group("Zero-Copy Microbench");

    // Test case 1: Pure ASCII with uppercase (LowerCase MUST allocate)

    let pipeline = NormyBuilder::default()
        .lang(ENG)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(TRIM_WHITESPACE)
        .build();

    let mut tracker_uppercase = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(ASCII_WITH_UPPERCASE.len() as u64));
    group.bench_function("ascii with uppercase (expect 0% zero-copy)", |b| {
        b.iter(|| {
            let r = pipeline
                .normalize(black_box(ASCII_WITH_UPPERCASE))
                .expect("normy failed");
            tracker_uppercase.record(ASCII_WITH_UPPERCASE, &r);
            r
        });
    });
    println!(
        "ASCII with uppercase → zero-copy: {:.2}% [EXPECTED: 0% - has uppercase]",
        tracker_uppercase.hit_rate_pct()
    );

    // Test case 2: Pure ASCII lowercase (should be 100% zero-copy!)

    let mut tracker_lowercase = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(ASCII_LOWERCASE.len() as u64));
    group.bench_function("ascii lowercase (expect 100% zero-copy)", |b| {
        b.iter(|| {
            let r = pipeline
                .normalize(black_box(ASCII_LOWERCASE))
                .expect("normy failed");
            tracker_lowercase.record(ASCII_LOWERCASE, &r);
            r
        });
    });
    println!(
        "ASCII lowercase → zero-copy: {:.2}% [EXPECTED: 100%]",
        tracker_lowercase.hit_rate_pct()
    );

    // Test case 3: Fast-path (no transformations needed)
    let fast_pipeline = NormyBuilder::default()
        .lang(ENG)
        .add_stage(TRIM_WHITESPACE)
        .build();

    let mut tracker_fast = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(ASCII_LOWERCASE.len() as u64));
    group.bench_function("fast-path trim only (expect 100% zero-copy)", |b| {
        b.iter(|| {
            let r = fast_pipeline
                .normalize(black_box(ASCII_LOWERCASE))
                .expect("normy failed");
            tracker_fast.record(ASCII_LOWERCASE, &r);
            r
        });
    });
    println!(
        "Fast-path (trim only) → zero-copy: {:.2}% [EXPECTED: 100%]",
        tracker_fast.hit_rate_pct()
    );

    // Test case 4: Precomposed unicode with uppercase (LowerCase must allocate)

    let mut tracker_composed = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(LATIN_COMPOSED_UPPER.len() as u64));
    group.bench_function("composed unicode with uppercase (expect 0%)", |b| {
        b.iter(|| {
            let r = pipeline
                .normalize(black_box(LATIN_COMPOSED_UPPER))
                .expect("normy failed");
            tracker_composed.record(LATIN_COMPOSED_UPPER, &r);
            r
        });
    });
    println!(
        "Composed unicode with uppercase → zero-copy: {:.2}% [EXPECTED: 0% - has uppercase]",
        tracker_composed.hit_rate_pct()
    );

    // Test case 5: Precomposed unicode lowercase (NFC already, no diacritics to remove)

    // Use pipeline that removes diacritics - this WILL allocate
    let pipeline_diacritics = NormyBuilder::default()
        .lang(ENG)
        .add_stage(NFC)
        .add_stage(LowerCase)
        .add_stage(RemoveDiacritics)
        .build();

    let mut tracker_diacritics = ZeroCopyTracker::default();
    group.throughput(Throughput::Bytes(LATIN_COMPOSED_LOWER.len() as u64));
    group.bench_function(
        "composed lowercase + remove diacritics (expect 100%)",
        |b| {
            b.iter(|| {
                let r = pipeline_diacritics
                    .normalize(black_box(LATIN_COMPOSED_LOWER))
                    .expect("normy failed");
                tracker_diacritics.record(LATIN_COMPOSED_LOWER, &r);
                r
            });
        },
    );
    println!(
        "Composed lowercase (removing diacritics) → zero-copy: {:.2}% [EXPECTED: 100% - no diacritics for EN]",
        tracker_diacritics.hit_rate_pct()
    );

    group.finish();
}

fn benches_full_baselines(c: &mut Criterion) {
    // tokenizers full (NFC + Lower + Strip + Whitespace collapse via regex)
    static WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

    let mut group = c.benchmark_group("Full Pipeline Baselines");
    let needs_transform = realistic_corpus_needs_transform(0xDEAD_BEEF, 128);
    let already_normalized = realistic_corpus_already_normalized(0xCAFE_BABE, 128);
    for (corpus, name) in [
        (&needs_transform, "Needs"),
        (&already_normalized, "Normalized"),
    ] {
        group.throughput(Throughput::Bytes(corpus.len() as u64));

        // Normy (existing)
        let pipeline = full_pipeline(ENG);
        group.bench_function(BenchmarkId::new("Normy Full", name), |b| {
            b.iter(|| pipeline.normalize(black_box(corpus)).expect("failed"));
        });

        // ICU4X equivalent (NFC + Lowercase + NFKC + Strip via regex fallback)
        group.bench_function(BenchmarkId::new("ICU4X Equivalent", name), |b| {
            b.iter(|| {
                let nfc = icu4x_nfc(corpus);
                let lower = nfc.to_lowercase();
                icu4x_nfkc(&lower) // + diacritics via NFKC
            });
        });

        group.bench_function(BenchmarkId::new("tokenizers Full", name), |b| {
            b.iter(|| {
                let norm = tokenizers_normalize(corpus);
                WS_RE.replace_all(&norm, " ").trim().to_string()
            });
        });
    }
    group.finish();
}

fn benches_memory_bandwidth_pressure(c: &mut Criterion) {
    let mut group = c.benchmark_group("Memory Bandwidth Pressure (Already Normalized)");
    let corpus = realistic_corpus_already_normalized(0xCAFE_BABE, 1024); // 1 MiB

    group.throughput(Throughput::Bytes(corpus.len() as u64));
    group.bench_function("ICU4X NFC", |b| b.iter(|| icu4x_nfc(black_box(&corpus))));
    group.bench_function("Normy NFC (zero-copy)", |b| {
        let pipeline = normy_nfc_only(ENG);
        b.iter(|| pipeline.normalize(black_box(&corpus)).unwrap());
    });

    // Add sample_size(10) because this is allocation-heavy
    group.sample_size(10);
    group.finish();
}

// ── Criterion harness
criterion_group!(
    benches,
    benches_main,
    benches_normalization_forms,
    benches_incremental_pipeline,
    bench_zero_copy_micro,
    benches_full_baselines,
    benches_memory_bandwidth_pressure,
);
criterion_main!(benches);

/*
--- Flamegraph & profiling template ---

Recommended approaches depending on your environment:

1) cargo-flamegraph (easy, linux):
   - Install: `cargo install flamegraph` (requires perf)
   - Run: `cargo flamegraph --bin <your-binary>` or for benches:
     `cargo +nightly bench --bench normy_bench_improved` then use the produced binary under target to perf-record.

2) perf + FlameGraph script (Linux):
   - Build release bench binary: `cargo bench --bench normy_bench_improved -- --nocapture --profile release`
   - Locate the benchmark binary under `target/release/deps/` (it is produced by criterion)
   - Record: `sudo perf record -F 99 -g -- target/release/deps/<bench-binary>`
   - Generate: `sudo perf script | /path/to/FlameGraph/stackcollapse-perf.pl | /path/to/FlameGraph/flamegraph.pl > flamegraph.svg`

3) macOS (Instruments) or Windows (Windows Performance Recorder):
   - Use OS-native profilers to sample the `target/release/deps/<bench-binary>` while running the bench.

Notes:
 - Criterion runs the benchmarks many times. When profiling you may want to run the bench a few times and target the specific worst-case function with a microbench.
 - Another approach is to add `#[inline(never)]` to candidate functions temporarily and microbenchmark them directly so call stacks are clearer.
*/
