// benches/comparison_accent_lowercase_bench.rs
// New benchmark comparing:
// 1. Normy: RemoveDiacritics + LowerCase (locale-aware)
// 2. tokenizers: Sequence(StripAccents + Lowercase)
// 3. unidecode-rs: unidecode() + .to_lowercase()
//
// Focus: Accent-heavy corpora (French, Vietnamese, Turkish, German)
// Expected: Normy shows high zero-copy on already-lowercase ASCII-ish text
//          tokenizers & unidecode always allocate

#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::random;
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::borrow::Cow;
use std::hint::black_box;
use std::sync::LazyLock;

use normy::{ENG, FRA, LowerCase, Normy, RemoveDiacritics, TUR, VIE};
use tokenizers::normalizers::{Lowercase, Sequence, StripAccents};
use tokenizers::{NormalizedString, Normalizer, NormalizerWrapper};
use unidecode::unidecode;

// ── Normy pipelines ──

// Accent removal + lowercase (locale-aware via lang)
type AccentLowerPipeline = Normy<
    normy::process::ChainedProcess<
        LowerCase,
        normy::process::ChainedProcess<RemoveDiacritics, normy::process::EmptyProcess>,
    >,
>;

static NORMY_ACCENT_LOWER_ENG: LazyLock<AccentLowerPipeline> = LazyLock::new(|| {
    Normy::builder()
        .lang(ENG)
        .add_stage(RemoveDiacritics)
        .add_stage(LowerCase)
        .build()
});

static NORMY_ACCENT_LOWER_FRA: LazyLock<AccentLowerPipeline> = LazyLock::new(|| {
    Normy::builder()
        .lang(FRA)
        .add_stage(RemoveDiacritics)
        .add_stage(LowerCase)
        .build()
});

static NORMY_ACCENT_LOWER_TUR: LazyLock<AccentLowerPipeline> = LazyLock::new(|| {
    Normy::builder()
        .lang(TUR)
        .add_stage(RemoveDiacritics)
        .add_stage(LowerCase)
        .build()
});

static NORMY_ACCENT_LOWER_VIE: LazyLock<AccentLowerPipeline> = LazyLock::new(|| {
    Normy::builder()
        .lang(VIE)
        .add_stage(RemoveDiacritics)
        .add_stage(LowerCase)
        .build()
});

// ── tokenizers Sequence (StripAccents + Lowercase) ──
static HF_STRIP_ACCENTS_LOWER: LazyLock<Sequence> = LazyLock::new(|| {
    Sequence::new(vec![
        NormalizerWrapper::StripAccents(StripAccents),
        NormalizerWrapper::Lowercase(Lowercase),
    ])
});

// ── Corpora ──
fn corpus_accent_heavy(seed: u64, kb: usize) -> String {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = String::with_capacity(kb * 1024);
    let pool = &[
        " naïve café résumé déjà-vu éléphant François naïve ",
        " Việt Nam Phở Tiếng Việt đắt đỏ ",
        " İstanbul ğüş öş İıŞş ",
        " Größe Straße fußball Maßstab ßẞ ÄÖÜäöü ",
        " naïve café déjà-vu résumé éléphant ",
    ];
    while out.len() < kb * 1024 {
        let s = pool[rng.random_range(0..pool.len())];
        out.push_str(s);
        if rng.random_bool(0.1) {
            out.push_str(" TEST ");
        }
    }
    truncate_to_boundary(&mut out, kb * 1024);
    out
}

fn corpus_already_lowered_no_accents(seed: u64, kb: usize) -> String {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = String::with_capacity(kb * 1024);
    while out.len() < kb * 1024 {
        let word: String = (0..rng.random_range(5..25))
            .map(|_| (b'a' + (random::<u8>() % 26)) as char)
            .collect();
        out.push_str(&word);
        out.push(' ');
    }
    truncate_to_boundary(&mut out, kb * 1024);
    out
}

fn truncate_to_boundary(s: &mut String, max: usize) {
    if s.len() > max {
        while !s.is_char_boundary(max) {
            s.pop();
        }
        s.truncate(max);
    }
}

static CORPUS_64KB_ACCENT: LazyLock<String> = LazyLock::new(|| corpus_accent_heavy(0x517ee, 64));
static CORPUS_64KB_NORM: LazyLock<String> =
    LazyLock::new(|| corpus_already_lowered_no_accents(0x1a7fe, 64));

// ── Benchmark harness ──
fn bench_accent_lowercase_normalizers(c: &mut Criterion) {
    let mut group = c.benchmark_group("Accent Removal + Lowercase Comparison");
    group.throughput(Throughput::Bytes(64 * 1024));
    group.sample_size(200);
    group.measurement_time(std::time::Duration::from_secs(12));

    let corpora = [
        ("accent_heavy_64kb", &*CORPUS_64KB_ACCENT),
        ("already_lowered_no_accents_64kb", &*CORPUS_64KB_NORM),
    ];

    for (name, corpus) in corpora {
        bench_normy_eng(&mut group, name, corpus);
        bench_normy_fra(&mut group, name, corpus);
        bench_normy_tur(&mut group, name, corpus);
        bench_normy_vie(&mut group, name, corpus);
        bench_hf_strip_accents_lower(&mut group, name, corpus);
        bench_unidecode_lower(&mut group, name, corpus);
    }

    group.finish();
}

#[allow(clippy::cast_precision_loss)]
fn bench_normy_eng(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;
    group.bench_function(BenchmarkId::new("Normy (ENG RemoveDiacritics + LowerCase)", scenario), |b| {
        b.iter(|| {
            total += 1;
            let result = NORMY_ACCENT_LOWER_ENG.normalize(black_box(corpus)).unwrap();
            if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr() && s.len() == corpus.len()) {
                zero_copy_hits += 1;
            }
            black_box(result);
        });
    });
    let pct = if total > 0 {
        (zero_copy_hits as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    println!(" Normy ENG - {scenario}: ZERO-COPY {zero_copy_hits}/{total} ({pct:.2}%)");
}

#[allow(clippy::cast_precision_loss)]
fn bench_normy_fra(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;
    group.bench_function(BenchmarkId::new("Normy (FRA RemoveDiacritics + LowerCase)", scenario), |b| {
        b.iter(|| {
            total += 1;
            let result = NORMY_ACCENT_LOWER_FRA.normalize(black_box(corpus)).unwrap();
            if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr() && s.len() == corpus.len()) {
                zero_copy_hits += 1;
            }
            black_box(result);
        });
    });
    let pct = if total > 0 {
        (zero_copy_hits as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    println!(" Normy FRA - {scenario}: ZERO-COPY {zero_copy_hits}/{total} ({pct:.2}%)");
}

#[allow(clippy::cast_precision_loss)]
fn bench_normy_tur(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;
    group.bench_function(BenchmarkId::new("Normy (TUR RemoveDiacritics + LowerCase)", scenario), |b| {
        b.iter(|| {
            total += 1;
            let result = NORMY_ACCENT_LOWER_TUR.normalize(black_box(corpus)).unwrap();
            if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr() && s.len() == corpus.len()) {
                zero_copy_hits += 1;
            }
            black_box(result);
        });
    });
    let pct = if total > 0 {
        (zero_copy_hits as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    println!(" Normy TUR - {scenario}: ZERO-COPY {zero_copy_hits}/{total} ({pct:.2}%)");
}

#[allow(clippy::cast_precision_loss)]
fn bench_normy_vie(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;
    group.bench_function(BenchmarkId::new("Normy (VIE RemoveDiacritics + LowerCase)", scenario), |b| {
        b.iter(|| {
            total += 1;
            let result = NORMY_ACCENT_LOWER_VIE.normalize(black_box(corpus)).unwrap();
            if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr() && s.len() == corpus.len()) {
                zero_copy_hits += 1;
            }
            black_box(result);
        });
    });
    let pct = if total > 0 {
        (zero_copy_hits as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    println!(" Normy VIE - {scenario}: ZERO-COPY {zero_copy_hits}/{total} ({pct:.2}%)");
}

fn bench_hf_strip_accents_lower(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    group.bench_function(
        BenchmarkId::new("tokenizers Sequence (StripAccents + Lowercase)", scenario),
        |b| {
            b.iter(|| {
                let mut ns = NormalizedString::from(black_box(corpus));
                HF_STRIP_ACCENTS_LOWER.normalize(&mut ns).unwrap();
                black_box(ns.get());
            });
        },
    );
    println!(" tokenizers - {scenario}: Always allocates (0.0% zero-copy)");
}

fn bench_unidecode_lower(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    group.bench_function(
        BenchmarkId::new("unidecode + to_lowercase()", scenario),
        |b| {
            b.iter(|| {
                let translit = unidecode(black_box(corpus));
                let lowered = translit.to_lowercase();
                black_box(lowered);
            });
        },
    );
    println!(" unidecode - {scenario}: Always allocates (0.0% zero-copy)");
}

criterion_group!(benches, bench_accent_lowercase_normalizers);
criterion_main!(benches);

#[cfg(test)]
mod tests {
    #[test]
    fn accent_lowercase_semantic_equivalence() {
        let cases = &[
            " naïve café résumé déjà-vu éléphant François ",
            " Việt Nam Phở Tiếng Việt đắt đỏ ",
            " İstanbul ğüş öş İıŞş ",
            " Größe Straße fußball Maßstab ßẞ ÄÖÜäöü ",
            " hello world ", // already normalized
            "",
        ];

        for &input in cases {
            // Normy ENG (baseline)
            let normy_eng = NORMY_ACCENT_LOWER_ENG
                .normalize(input)
                .unwrap()
                .into_owned();

            // tokenizers StripAccents + Lowercase
            let mut ns = NormalizedString::from(input);
            HF_STRIP_ACCENTS_LOWER.normalize(&mut ns).unwrap();
            let hf_output = ns.get().to_string();

            // unidecode + lowercase (note: more aggressive)
            let unidecode_output = unidecode(input).to_lowercase();

            // Semantic check vs Normy ENG (adjust expectations for unidecode if needed)
            assert_eq!(
                normy_eng, hf_output,
                "tokenizers mismatch on input: {:?}",
                input
            );

            // Zero-copy on unchanged
            if input
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_whitespace())
            {
                let result = NORMY_ACCENT_LOWER_ENG.normalize(input).unwrap();
                assert!(
                    matches!(result, Cow::Borrowed(s) if s.as_ptr() == input.as_ptr() && s.len() == input.len()),
                    "Zero-copy failed on already-normalized input: {:?}",
                    input
                );
            }
        }
    }
}
