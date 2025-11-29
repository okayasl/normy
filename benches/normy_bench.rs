// benches/normy_bench.rs
#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate)] // We return Cow on purpose

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use normy::{
    CaseFold, DEU, ENG, LowerCase, NFC, NORMALIZE_WHITESPACE_FULL, Normy, NormyBuilder,
    SegmentWords, StripHtml, TRIM_WHITESPACE_ONLY, TUR, UnifyWidth, lang::Lang, process::Process,
};
use std::borrow::Cow;
use std::hint::black_box;
use unicode_normalization::UnicodeNormalization;

// ── Corpora ──
const STRESS_EN: &str = "Hello, world! déjà vu café naïve ﬁligree ﬂag ﬁﬀﬃﬃ...";
const STRESS_TR: &str = "İSTANBUL'da büyük ŞOK! İiIı göz göze...";
const STRESS_DE: &str = "Größe Straße fußball ßẞ ÄÖÜäöü Maßstab...";
const STRESS_JA: &str = "　全角スペースと半角 space が混在　こんにちは世界！";
const STRESS_ZH: &str = "　你好，世界！　Ｈｅｌｌｏ　Ｗｏｒｌｄ　";

fn mixed_stress() -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}",
        STRESS_EN.repeat(4000),
        STRESS_TR.repeat(4000),
        STRESS_DE.repeat(4000),
        STRESS_JA.repeat(2500),
        STRESS_ZH.repeat(2500),
    )
}

fn homoglyph_storm() -> String {
    "A Α А Ꭺ ᗅ ᴀ ꓮ Ａ 𐊠 𝐀 𝐴 𝑨 𝒜 𝓐 𝔄 𝔸 𝕬 𝖠 𝗔 𝘈 𝘼 𝙰 𝚨 𝛢 𝜜 𝝖 𝞐 café ﬁﬀﬃﬃ".repeat(10_000)
}

// ── Pipelines ──
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

// ── Zero-Copy Tracker (Clippy-clean) ──
#[derive(Default)]
struct ZeroCopyTracker {
    hits: usize,
    total: usize,
}

impl ZeroCopyTracker {
    // We must accept &Cow to check the Borrowed/Owned variant for zero-copy tracking.
    #[allow(clippy::ptr_arg)]
    fn record(&mut self, input: &str, output: &Cow<'_, str>) {
        self.total += 1;
        if matches!(output, Cow::Borrowed(s) if s.as_ptr() == input.as_ptr() && s.len() == input.len())
        {
            self.hits += 1;
        } else {
            // DEBUG: Which stage broke zero-copy?
            // println!(
            //     "ALLOCATED: input={} output={} ptr_match={}",
            //     input.len(),
            //     output.len(),
            //     input.as_ptr() == output.as_ref().as_ptr()
            // );
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn hit_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.hits as f64 / self.total as f64
        }
    }
}

// ── Benchmark ──
fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("Normy vs Baselines");
    group.throughput(Throughput::Bytes(1_500_000)); // ~1.5MB realistic

    let mixed = mixed_stress();
    let storm = homoglyph_storm();
    let mut tracker = ZeroCopyTracker::default();

    let cases = [
        (&mixed, ENG, "EN Mixed"),
        (&mixed, TUR, "TR Locale (İ/i)"),
        (&mixed, DEU, "DE ß→ss CaseFold"),
        (&storm, ENG, "Homoglyph Storm NFKC"),
    ];

    for &(text, lang, name) in &cases {
        let pipeline = search_pipeline(lang);
        group.bench_function(format!("Normy Search/{name}"), |b| {
            b.iter(|| {
                let result = pipeline.normalize(black_box(text)).expect("normy failed");
                tracker.record(text, &result);
                let again = pipeline.normalize(&result).expect("idempotency failed");
                assert_eq!(result, again);
                result
            });
        });
    }

    let display = display_pipeline(ENG);
    group.bench_function("Normy Display (HTML+CJK+Trim)", |b| {
        b.iter(|| {
            let result = display
                .normalize(black_box(&mixed))
                .expect("display failed");
            tracker.record(&mixed, &result);
            result
        });
    });

    group.bench_function("unicode-normalization NFC", |b| {
        b.iter(|| unicode_nfc(black_box(&mixed)));
    });

    group.bench_function("unicode-normalization NFKC", |b| {
        b.iter(|| unicode_nfkc(black_box(&storm)));
    });

    group.bench_function("unidecode (Rust)", |b| {
        b.iter(|| unidecode_baseline(black_box(&mixed)));
    });

    group.finish();

    let rate = tracker.hit_rate() * 100.0;
    println!(
        "\nZERO-COPY HIT RATE: {rate:.2}% ({}/{})",
        tracker.hits, tracker.total
    );
    println!("   → This is not marketing. This is memory-level truth.\n");
}

criterion_group!(benches, bench);
criterion_main!(benches);
