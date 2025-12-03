#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use icu::normalizer::{ComposingNormalizerBorrowed, DecomposingNormalizerBorrowed};
use normy::{
    process::{ChainedProcess, EmptyProcess},
    stage::normalization::{NfcStage, NfdStage, NfkcStage, NfkdStage},
};
use rand::{Rng, SeedableRng, random, rngs::StdRng};
use std::sync::LazyLock;
use std::{borrow::Cow, hint::black_box};

use tokenizers::{
    NormalizedString, Normalizer,
    normalizers::{
        Sequence, unicode::NFC as tokenizerNFC, unicode::NFD as tokenizerNFD,
        unicode::NFKC as tokenizerNFKC, unicode::NFKD as tokenizerNFKD,
    },
};

use normy::{NFC, NFD, NFKC, NFKD, Normy, NormyBuilder};
use unicode_normalization::UnicodeNormalization;
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

fn truncate_to_char_boundary(s: &mut String, max_len: usize) {
    if s.len() > max_len {
        while !s.is_char_boundary(max_len) && !s.is_empty() {
            s.pop();
        }
        s.truncate(max_len);
    }
}

// ── Zero-Copy Tracker ──
#[derive(Default)]
struct ZeroCopyTracker {
    name: String,
    hits: usize,
    total: usize,
}

impl ZeroCopyTracker {
    fn new(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

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

    fn print(&self) {
        println!(
            "Case: {} → ZERO-COPY: {:.2}% ({}/{})",
            self.name,
            self.hit_rate_pct(),
            self.hits,
            self.total
        );
    }
}

// ----------------------------- ICU4X -----------------------------
// NFC
static ICU4X_NFC_NORMALIZER: LazyLock<ComposingNormalizerBorrowed<'static>> =
    LazyLock::new(ComposingNormalizerBorrowed::new_nfc);
fn icu4x_nfc(text: &str) -> Cow<'_, str> {
    ICU4X_NFC_NORMALIZER.normalize(text)
}

// NFKC
static ICU4X_NFKC_NORMALIZER: LazyLock<ComposingNormalizerBorrowed<'static>> =
    LazyLock::new(ComposingNormalizerBorrowed::new_nfkc);
fn icu4x_nfkc(text: &str) -> Cow<'_, str> {
    ICU4X_NFKC_NORMALIZER.normalize(text)
}

// NFD
static ICU4X_NFD_NORMALIZER: LazyLock<DecomposingNormalizerBorrowed<'static>> =
    LazyLock::new(DecomposingNormalizerBorrowed::new_nfd);
fn icu4x_nfd(text: &str) -> Cow<'_, str> {
    ICU4X_NFD_NORMALIZER.normalize(text)
}

// NFKD
static ICU4X_NFKD_NORMALIZER: LazyLock<DecomposingNormalizerBorrowed<'static>> =
    LazyLock::new(DecomposingNormalizerBorrowed::new_nfkd);
fn icu4x_nfkd(text: &str) -> Cow<'_, str> {
    ICU4X_NFKD_NORMALIZER.normalize(text)
}

// ----------------------------- HF Tokenizers -----------------------------
static HF_TOKENIZERS_NORMALIZER_NFC: LazyLock<Sequence> =
    LazyLock::new(|| Sequence::new(vec![tokenizers::NormalizerWrapper::NFC(tokenizerNFC)]));
fn hf_tokenizers_nfc(text: &str) -> String {
    let mut n = NormalizedString::from(text);
    HF_TOKENIZERS_NORMALIZER_NFC.normalize(&mut n).unwrap();
    n.get().to_string()
}

static HF_TOKENIZERS_NORMALIZER_NFKC: LazyLock<Sequence> =
    LazyLock::new(|| Sequence::new(vec![tokenizers::NormalizerWrapper::NFKC(tokenizerNFKC)]));
fn hf_tokenizers_nfkc(text: &str) -> String {
    let mut n = NormalizedString::from(text);
    HF_TOKENIZERS_NORMALIZER_NFKC.normalize(&mut n).unwrap();
    n.get().to_string()
}

static HF_TOKENIZERS_NORMALIZER_NFD: LazyLock<Sequence> =
    LazyLock::new(|| Sequence::new(vec![tokenizers::NormalizerWrapper::NFD(tokenizerNFD)]));
fn hf_tokenizers_nfd(text: &str) -> String {
    let mut n = NormalizedString::from(text);
    HF_TOKENIZERS_NORMALIZER_NFD.normalize(&mut n).unwrap();
    n.get().to_string()
}

static HF_TOKENIZERS_NORMALIZER_NFKD: LazyLock<Sequence> =
    LazyLock::new(|| Sequence::new(vec![tokenizers::NormalizerWrapper::NFKD(tokenizerNFKD)]));
fn hf_tokenizers_nfkd(text: &str) -> String {
    let mut n = NormalizedString::from(text);
    HF_TOKENIZERS_NORMALIZER_NFKD.normalize(&mut n).unwrap();
    n.get().to_string()
}

// ----------------------------- Normy -----------------------------
static NORMY_NFC: LazyLock<Normy<ChainedProcess<NfcStage, EmptyProcess>>> =
    LazyLock::new(|| NormyBuilder::default().add_stage(NFC).build());
fn normy_nfc(text: &str) -> Cow<'_, str> {
    NORMY_NFC.normalize(text).unwrap()
}

static NORMY_NFKC: LazyLock<Normy<ChainedProcess<NfkcStage, EmptyProcess>>> =
    LazyLock::new(|| NormyBuilder::default().add_stage(NFKC).build());
fn normy_nfkc(text: &str) -> Cow<'_, str> {
    NORMY_NFKC.normalize(text).unwrap()
}

static NORMY_NFD: LazyLock<Normy<ChainedProcess<NfdStage, EmptyProcess>>> =
    LazyLock::new(|| NormyBuilder::default().add_stage(NFD).build());
fn normy_nfd(text: &str) -> Cow<'_, str> {
    NORMY_NFD.normalize(text).unwrap()
}

static NORMY_NFKD: LazyLock<Normy<ChainedProcess<NfkdStage, EmptyProcess>>> =
    LazyLock::new(|| NormyBuilder::default().add_stage(NFKD).build());
fn normy_nfkd(text: &str) -> Cow<'_, str> {
    NORMY_NFKD.normalize(text).unwrap()
}

// ----------------------------- Unicode -----------------------------
fn unicode_nfc(text: &str) -> String {
    text.nfc().collect()
}
fn unicode_nfkc(text: &str) -> String {
    text.nfkc().collect()
}
fn unicode_nfd(text: &str) -> String {
    text.nfd().collect()
}
fn unicode_nfkd(text: &str) -> String {
    text.nfkd().collect()
}

/// Benchmark individual normalization forms
fn benches_normalization_forms(c: &mut Criterion) {
    let mut group = c.benchmark_group("Normalization Forms");
    group.measurement_time(std::time::Duration::from_secs(10));

    let needs_transform = realistic_corpus_needs_transform(0xDEAD_BEEF, 128);
    let already_normalized = realistic_corpus_already_normalized(0xCAFE_BABE, 128);

    for (corpus, corpus_name) in [
        (&needs_transform, "Needs Transform"),
        (&already_normalized, "Already Normalized"),
    ] {
        group.throughput(Throughput::Bytes(corpus.len() as u64));

        // Normy implementations with ZeroCopyTracker
        macro_rules! bench_normy_zc {
            ($name:expr, $func:expr) => {{
                let mut zct = ZeroCopyTracker::new($name.to_string());
                group.bench_function(BenchmarkId::new($name, corpus_name), |b| {
                    b.iter(|| {
                        let result = $func(black_box(corpus));
                        zct.record(corpus, &result);
                        result
                    })
                });
                zct.print();
            }};
        }

        bench_normy_zc!("Normy NFC", normy_nfc);
        bench_normy_zc!("Normy NFKC", normy_nfkc);
        bench_normy_zc!("Normy NFD", normy_nfd);
        bench_normy_zc!("Normy NFKD", normy_nfkd);

        // Unicode baselines
        group.bench_function(BenchmarkId::new("Unicode NFC", corpus_name), |b| {
            b.iter(|| unicode_nfc(black_box(corpus)));
        });

        group.bench_function(BenchmarkId::new("Unicode NFKC", corpus_name), |b| {
            b.iter(|| unicode_nfkc(black_box(corpus)));
        });

        group.bench_function(BenchmarkId::new("Unicode NFD", corpus_name), |b| {
            b.iter(|| unicode_nfd(black_box(corpus)));
        });

        group.bench_function(BenchmarkId::new("Unicode NFKD", corpus_name), |b| {
            b.iter(|| unicode_nfkd(black_box(corpus)));
        });

        // ICU4X implementations with ZeroCopyTracker
        macro_rules! bench_icu_zc {
            ($name:expr, $func:expr) => {{
                let mut zct = ZeroCopyTracker::new($name.to_string());
                group.bench_function(BenchmarkId::new($name, corpus_name), |b| {
                    b.iter(|| {
                        let result = $func(black_box(corpus));
                        zct.record(corpus, &result);
                        result
                    })
                });
                zct.print();
            }};
        }

        bench_icu_zc!("ICU4X NFC", icu4x_nfc);
        bench_icu_zc!("ICU4X NFKC", icu4x_nfkc);
        bench_icu_zc!("ICU4X NFD", icu4x_nfd);
        bench_icu_zc!("ICU4X NFKD", icu4x_nfkd);

        // Huggingface tokenizers
        group.bench_function(
            BenchmarkId::new("Huggingface tokenizers NFC", corpus_name),
            |b| b.iter(|| hf_tokenizers_nfc(black_box(corpus))),
        );
        group.bench_function(
            BenchmarkId::new("Huggingface tokenizers NFKC", corpus_name),
            |b| b.iter(|| hf_tokenizers_nfkc(black_box(corpus))),
        );
        group.bench_function(
            BenchmarkId::new("Huggingface tokenizers NFD", corpus_name),
            |b| b.iter(|| hf_tokenizers_nfd(black_box(corpus))),
        );
        group.bench_function(
            BenchmarkId::new("Huggingface tokenizers NFKD", corpus_name),
            |b| b.iter(|| hf_tokenizers_nfkd(black_box(corpus))),
        );
    }

    group.finish();
}

criterion_group!(benches, benches_normalization_forms,);
criterion_main!(benches);
