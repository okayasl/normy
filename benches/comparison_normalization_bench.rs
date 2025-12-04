#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use icu_normalizer::{ComposingNormalizerBorrowed, DecomposingNormalizerBorrowed};
use normy::{
    NFC, NFD, NFKC, NFKD, Normy, NormyBuilder,
    process::{ChainedProcess, EmptyProcess},
    stage::normalization::{NfcStage, NfdStage, NfkcStage, NfkdStage},
};
use rand::{Rng, SeedableRng, random, rngs::StdRng};
use std::borrow::Cow;
use std::{hint::black_box, sync::LazyLock};
use tokenizers::{
    NormalizedString, Normalizer,
    normalizers::{
        Sequence, unicode::NFC as tokenizerNFC, unicode::NFD as tokenizerNFD,
        unicode::NFKC as tokenizerNFKC, unicode::NFKD as tokenizerNFKD,
    },
};
use unicode_normalization::UnicodeNormalization;

// ──────────────────────────────────────────────────────────────
// Stress Samples
// ──────────────────────────────────────────────────────────────
static STRESS_POOL_NFC_NFD: &[&str] = &[
    "Tiếng Việt Quốc ngữ Phở Hà Nội",
    "Sœur naïve à l'œuf ŒUF déjà-vu",
    "Fußball Straße Maßstab GRÜNE STRAẞE",
    "İSTANBUL İĞNE İĞDE ıiIİ",
    "¡España mañana José Peña!",
    "Łódź żółć ŻÓŁĆ Żubrówka",
    "Žemaitija Šiauliai Jurgis",
    "Þetta er íslenska ÐðÞþ",
    "Ștefan Țară România",
    "Đuro Đaković Ljiljana Njiva",
    "Ἀρχιμήδης Ἑλλάς σοφός",
    "Ёлки-палки всё А́нна",
    "الْكِتَابُ مُحَمَّدٌ ـــ",
    "סֵפֶר עִבְרִית שׂ",
    "हिन्दी ज़िंदगी क़िला",
    "ภาษาไทย สวัสดีครับ ๑๒๓",
    "한글 ＫＯＲＥＡ 한국어",
    "ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟ ーー こんにちは",
    "ＨＴＭＬ　＜ｔａｇ＞　你好世界",
    "👨‍👩‍👧‍👦 👍🏼 ✨ 🚀",
    "ﬁﬂﬃﬄﬆﬀﬁﬃﬃﬃ",
];

static STRESS_POOL_NFKC_NFKD: &[&str] = &[
    "ﬀ ﬁ ﬂ ﬃ ﬄ ﬆ ﬁﬀﬃﬃ",
    "½ ⅓ ¼ ⅕ ⅙ ⅛ ⅔ ¾",
    "①②③④⑤ ⑩ ⑴⑵⑶ ⒈⒉⒊",
    "Ｈｅｌｌｏ　Ｗｏｒｌｄ　＆　＜＞",
    "㈱ ㈲ ㎏ ㎞ ㎡",
    "№ ℡ ™ © ®",
];

fn realistic_corpus(seed: u64, size_kb: usize) -> String {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = String::with_capacity(size_kb * 1024);
    let pools = [STRESS_POOL_NFC_NFD, STRESS_POOL_NFKC_NFKD];

    while out.len() < size_kb * 1024 {
        let pool = pools[rng.random_range(0..pools.len())];
        let text = pool[rng.random_range(0..pool.len())];
        for _ in 0..rng.random_range(1..=5) {
            out.push_str(text);
            out.push(' ');
        }
        if rng.random_bool(0.1) {
            let word: String = (0..rng.random_range(5..20))
                .map(|_| (b'a' + (random::<u8>() % 26)) as char)
                .collect();
            out.push_str(&word);
            out.push(' ');
        }
    }

    // Truncate at a valid UTF-8 boundary
    let max_len = size_kb * 1024;
    if out.len() > max_len {
        let mut truncate_at = max_len;
        while truncate_at > 0 && !out.is_char_boundary(truncate_at) {
            truncate_at -= 1;
        }
        out.truncate(truncate_at);
    }
    out
}

// Corpus generators
fn corpus_needs_nfc(seed: u64, size_kb: usize) -> String {
    let mut base = realistic_corpus(seed, size_kb);
    base.push_str(STRESS_POOL_NFC_NFD[0]);
    base.push_str(STRESS_POOL_NFC_NFD[1]);
    base.nfd().collect()
}

fn corpus_needs_nfd(seed: u64, size_kb: usize) -> String {
    realistic_corpus(seed, size_kb).nfc().collect()
}

fn corpus_needs_nfkc(seed: u64, size_kb: usize) -> String {
    let base = realistic_corpus(seed, size_kb);
    let mut s: String = base.nfkd().collect();
    s = s.nfc().collect();
    s.push_str(" ﬁﬂﬃﬄﬆﬀﬁﬃﬃﬃ ①②③ ½⅓¼ Ｈｅｌｌｏ ＜＞ ＆");
    s
}

fn corpus_needs_nfkd(seed: u64, size_kb: usize) -> String {
    let s: String = realistic_corpus(seed, size_kb).nfkc().collect();
    format!("{} ﬁ ﬂ ﬃ ﬀ ﬆ ﬁﬀﬃﬃ ① ½ ＆ Ｈｅｌｌｏ ＜＞", s)
}

fn corpus_already_nfc(seed: u64, size_kb: usize) -> String {
    realistic_corpus(seed, size_kb).nfc().collect()
}

fn corpus_already_nfd(seed: u64, size_kb: usize) -> String {
    realistic_corpus(seed, size_kb).nfd().collect()
}

fn corpus_already_nfkc(seed: u64, size_kb: usize) -> String {
    realistic_corpus(seed, size_kb).nfkc().collect()
}

fn corpus_already_nfkd(seed: u64, size_kb: usize) -> String {
    realistic_corpus(seed, size_kb).nfkd().collect()
}

// ── ICU4X ──
static ICU4X_NFC: LazyLock<ComposingNormalizerBorrowed<'static>> =
    LazyLock::new(ComposingNormalizerBorrowed::new_nfc);
static ICU4X_NFKC: LazyLock<ComposingNormalizerBorrowed<'static>> =
    LazyLock::new(ComposingNormalizerBorrowed::new_nfkc);
static ICU4X_NFD: LazyLock<DecomposingNormalizerBorrowed<'static>> =
    LazyLock::new(DecomposingNormalizerBorrowed::new_nfd);
static ICU4X_NFKD: LazyLock<DecomposingNormalizerBorrowed<'static>> =
    LazyLock::new(DecomposingNormalizerBorrowed::new_nfkd);

// ── HF Tokenizers ──
static HF_NFC: LazyLock<Sequence> =
    LazyLock::new(|| Sequence::new(vec![tokenizers::NormalizerWrapper::NFC(tokenizerNFC)]));
static HF_NFKC: LazyLock<Sequence> =
    LazyLock::new(|| Sequence::new(vec![tokenizers::NormalizerWrapper::NFKC(tokenizerNFKC)]));
static HF_NFD: LazyLock<Sequence> =
    LazyLock::new(|| Sequence::new(vec![tokenizers::NormalizerWrapper::NFD(tokenizerNFD)]));
static HF_NFKD: LazyLock<Sequence> =
    LazyLock::new(|| Sequence::new(vec![tokenizers::NormalizerWrapper::NFKD(tokenizerNFKD)]));

fn hf_normalize(text: &str, normalizer: &Sequence) -> String {
    let mut n = NormalizedString::from(text);
    normalizer.normalize(&mut n).unwrap();
    n.get().to_string()
}

// ── Normy ──
static NORMY_NFC: LazyLock<Normy<ChainedProcess<NfcStage, EmptyProcess>>> =
    LazyLock::new(|| NormyBuilder::default().add_stage(NFC).build());
static NORMY_NFKC: LazyLock<Normy<ChainedProcess<NfkcStage, EmptyProcess>>> =
    LazyLock::new(|| NormyBuilder::default().add_stage(NFKC).build());
static NORMY_NFD: LazyLock<Normy<ChainedProcess<NfdStage, EmptyProcess>>> =
    LazyLock::new(|| NormyBuilder::default().add_stage(NFD).build());
static NORMY_NFKD: LazyLock<Normy<ChainedProcess<NfkdStage, EmptyProcess>>> =
    LazyLock::new(|| NormyBuilder::default().add_stage(NFKD).build());

fn benches_normalization_forms(c: &mut Criterion) {
    let mut group = c.benchmark_group("Normalization Forms");
    group.measurement_time(std::time::Duration::from_secs(10));

    let scenarios = [
        ("NFC", "Needs NFC", corpus_needs_nfc(0x517ea41e, 128)),
        ("NFC", "Already NFC", corpus_already_nfc(0x1a71c0fe, 128)),
        ("NFD", "Needs NFD", corpus_needs_nfd(0xdeadbeef, 128)),
        ("NFD", "Already NFD", corpus_already_nfd(0xb1a9c3d4, 128)),
        ("NFKC", "Needs NFKC", corpus_needs_nfkc(0x1337c0de, 128)),
        ("NFKC", "Already NFKC", corpus_already_nfkc(0x76543210, 128)),
        ("NFKD", "Needs NFKD", corpus_needs_nfkd(0xcafef00d, 128)),
        ("NFKD", "Already NFKD", corpus_already_nfkd(0xabcdef01, 128)),
    ];

    for (form, scenario, corpus) in &scenarios {
        group.throughput(Throughput::Bytes(corpus.len() as u64));

        // Benchmark each library
        match *form {
            "NFC" => {
                bench_with_cow("Normy", form, scenario, &mut group, corpus, |s| {
                    NORMY_NFC.normalize(s).unwrap()
                });
                bench_with_cow("ICU4X", form, scenario, &mut group, corpus, |s| {
                    ICU4X_NFC.normalize(s)
                });
                bench_no_cow("Unicode", form, scenario, &mut group, corpus, |s: &str| {
                    s.nfc().collect::<String>()
                });
                bench_no_cow("HF Tokenizers", form, scenario, &mut group, corpus, |s| {
                    hf_normalize(s, &HF_NFC)
                });
            }
            "NFD" => {
                bench_with_cow("Normy", form, scenario, &mut group, corpus, |s| {
                    NORMY_NFD.normalize(s).unwrap()
                });
                bench_with_cow("ICU4X", form, scenario, &mut group, corpus, |s| {
                    ICU4X_NFD.normalize(s)
                });
                bench_no_cow("Unicode", form, scenario, &mut group, corpus, |s: &str| {
                    s.nfd().collect::<String>()
                });
                bench_no_cow("HF Tokenizers", form, scenario, &mut group, corpus, |s| {
                    hf_normalize(s, &HF_NFD)
                });
            }
            "NFKC" => {
                bench_with_cow("Normy", form, scenario, &mut group, corpus, |s| {
                    NORMY_NFKC.normalize(s).unwrap()
                });
                bench_with_cow("ICU4X", form, scenario, &mut group, corpus, |s| {
                    ICU4X_NFKC.normalize(s)
                });
                bench_no_cow("Unicode", form, scenario, &mut group, corpus, |s: &str| {
                    s.nfkc().collect::<String>()
                });
                bench_no_cow("HF Tokenizers", form, scenario, &mut group, corpus, |s| {
                    hf_normalize(s, &HF_NFKC)
                });
            }
            "NFKD" => {
                bench_with_cow("Normy", form, scenario, &mut group, corpus, |s| {
                    NORMY_NFKD.normalize(s).unwrap()
                });
                bench_with_cow("ICU4X", form, scenario, &mut group, corpus, |s| {
                    ICU4X_NFKD.normalize(s)
                });
                bench_no_cow("Unicode", form, scenario, &mut group, corpus, |s: &str| {
                    s.nfkd().collect::<String>()
                });
                bench_no_cow("HF Tokenizers", form, scenario, &mut group, corpus, |s| {
                    hf_normalize(s, &HF_NFKD)
                });
            }
            _ => unreachable!(),
        }
    }

    group.finish();
}

fn bench_with_cow<F>(
    lib: &str,
    form: &str,
    scenario: &str,
    group: &mut criterion::BenchmarkGroup<criterion::measurement::WallTime>,
    corpus: &str,
    mut func: F,
) where
    F: FnMut(&str) -> Cow<'_, str>,
{
    let mut zero_copy_count = 0;
    let mut total_count = 0;

    group.bench_function(BenchmarkId::new(format!("{} {}", lib, form), scenario), |b| {
        b.iter(|| {
            let result = func(black_box(corpus));
            total_count += 1;
            // Check for zero-copy: pointer is the same AND length is the same (handle NF*D being same length)
            if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr() && s.len() == corpus.len()) {
                zero_copy_count += 1;
            }
            result
        })
    });

    // Print the zero-copy info directly to console output (not recorded in Criterion's data)
    let zero_copy_pct = if total_count > 0 {
        (zero_copy_count as f64 / total_count as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "  {} {} - {}: Zero-Copy {:.1}% ({}/{})",
        lib, form, scenario, zero_copy_pct, zero_copy_count, total_count
    );
}

fn bench_no_cow<F>(
    lib: &str,
    form: &str,
    scenario: &str,
    group: &mut criterion::BenchmarkGroup<criterion::measurement::WallTime>,
    corpus: &str,
    func: F,
) where
    F: Fn(&str) -> String,
{
    group.bench_function(
        BenchmarkId::new(format!("{} {}", lib, form), scenario),
        |b| b.iter(|| func(black_box(corpus))),
    );

    println!(
        "  {} {} - {}: Always allocates (0.0% Zero-Copy)",
        lib, form, scenario
    );
}

criterion_group!(benches, benches_normalization_forms);
criterion_main!(benches);
