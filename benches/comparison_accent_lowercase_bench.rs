// benches/comparison_accent_lowercase_bench.rs
// Benchmark comparing SEMANTICALLY EQUIVALENT normalizers:
// 1. German (DEU): Normy CaseFold vs unidecode+lowercase (both do ß→ss)
// 2. French (FRA): Normy LowerCase+Transliterate vs tokenizers StripAccents+Lowercase
#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::random;
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::borrow::Cow;
use std::hint::black_box;
use std::sync::LazyLock;
use std::time::Duration;

use normy::{CaseFold, DEU, FRA, LowerCase, Normy, Transliterate};
use tokenizers::normalizers::{Lowercase, Sequence, StripAccents};
use tokenizers::{NormalizedString, Normalizer, NormalizerWrapper};
use unidecode::unidecode;

// ── Normy pipelines ──
type CaseFoldPipeline =
    Normy<normy::process::ChainedProcess<CaseFold, normy::process::EmptyProcess>>;

type LowercaseTransliteratePipeline = Normy<
    normy::process::ChainedProcess<
        Transliterate,
        normy::process::ChainedProcess<LowerCase, normy::process::EmptyProcess>,
    >,
>;

static NORMY_DEU_PIPELINE: LazyLock<CaseFoldPipeline> =
    LazyLock::new(|| Normy::builder().lang(DEU).add_stage(CaseFold).build());

static NORMY_FRA_PIPELINE: LazyLock<LowercaseTransliteratePipeline> = LazyLock::new(|| {
    Normy::builder()
        .lang(FRA)
        .add_stage(LowerCase)
        .add_stage(Transliterate)
        .build()
});

// ── Baseline normalizers ──
static HF_NORMALIZER: LazyLock<Sequence> = LazyLock::new(|| {
    Sequence::new(vec![
        NormalizerWrapper::StripAccents(StripAccents),
        NormalizerWrapper::Lowercase(Lowercase),
    ])
});

// ── Corpus generators ──
fn corpus_german(seed: u64, kb: usize) -> String {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = String::with_capacity(kb * 1024);

    // German words with ß but NO Ö/Ü/Ä (unidecode strips those)
    let pool = german_pool();

    while out.len() < kb * 1024 {
        out.push_str(pool[rng.random_range(0..pool.len())]);
        if rng.random_bool(0.1) {
            out.push_str(" TEST ");
        }
    }
    truncate_to_boundary(&mut out, kb * 1024);
    out
}

fn german_pool() -> &'static [&'static str; 5] {
    (&[
        " Fußball Maßstab Straße ",
        " ßẞ GROß ",
        " Spaß Gruß muß ",
        " heißen weißer ",
        " HELLO WORLD TEST ",
    ]) as _
}

fn french_pool() -> &'static [&'static str; 5] {
    (&[
        " NAïve CAFé Résumé ",
        " Déjà-vu éléphant ",
        " être protégé crème ",
        " élève âme ",
        " HELLO WORLD TEST ", // some ASCII
    ]) as _
}

fn corpus_french(seed: u64, kb: usize) -> String {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = String::with_capacity(kb * 1024);

    let pool = french_pool();

    while out.len() < kb * 1024 {
        out.push_str(pool[rng.random_range(0..pool.len())]);
        if rng.random_bool(0.1) {
            out.push_str(" TEST ");
        }
    }
    truncate_to_boundary(&mut out, kb * 1024);
    out
}

fn corpus_already_normalized(seed: u64, kb: usize) -> String {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = String::with_capacity(kb * 1024);

    while out.len() < kb * 1024 {
        let word: String = (0..rng.random_range(5..25))
            .map(|_| (b'a' + random::<u8>() % 26) as char)
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

static CORPUS_GERMAN: LazyLock<String> = LazyLock::new(|| corpus_german(0x517ee, 64));
static CORPUS_FRENCH: LazyLock<String> = LazyLock::new(|| corpus_french(0x1a7fe, 64));
static CORPUS_NORMALIZED: LazyLock<String> =
    LazyLock::new(|| corpus_already_normalized(0x2beef, 64));

// ── Benchmark functions ──
#[allow(clippy::cast_precision_loss)]
fn bench_normy_deu(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(BenchmarkId::new("Normy (DEU CaseFold)", scenario), |b| {
        b.iter(|| {
            total += 1;
            let result = NORMY_DEU_PIPELINE.normalize(black_box(corpus)).unwrap();
            if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr()) {
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
    println!("  Normy DEU - {scenario:25}: zero-copy {zero_copy_hits:4}/{total:4} ({pct:5.2}%)");
}

#[allow(clippy::cast_precision_loss)]
fn bench_normy_fused_deu(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(
        BenchmarkId::new("Normy Fused (DEU CaseFold)", scenario),
        |b| {
            b.iter(|| {
                total += 1;
                let result = NORMY_DEU_PIPELINE
                    .normalize_fused(black_box(corpus))
                    .unwrap();
                if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr()) {
                    zero_copy_hits += 1;
                }
                black_box(result);
            });
        },
    );

    let pct = if total > 0 {
        (zero_copy_hits as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "  Normy Fused DEU - {scenario:25}: zero-copy {zero_copy_hits:4}/{total:4} ({pct:5.2}%)"
    );
}

#[allow(clippy::cast_precision_loss)]
fn bench_normy_static_fused_deu(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(
        BenchmarkId::new("Normy Static Fused (DEU CaseFold)", scenario),
        |b| {
            b.iter(|| {
                total += 1;
                let result = NORMY_DEU_PIPELINE
                    .normalize_static_fused(black_box(corpus))
                    .unwrap();
                if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr()) {
                    zero_copy_hits += 1;
                }
                black_box(result);
            });
        },
    );

    let pct = if total > 0 {
        (zero_copy_hits as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "  Normy Static Fused DEU - {scenario:25}: zero-copy {zero_copy_hits:4}/{total:4} ({pct:5.2}%)"
    );
}

fn bench_unidecode_deu(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    group.bench_function(BenchmarkId::new("unidecode+lowercase", scenario), |b| {
        b.iter(|| {
            let result = unidecode(black_box(corpus)).to_lowercase();
            black_box(result);
        });
    });
    println!("  unidecode   - {scenario:25}: always allocates (0% zero-copy)");
}

#[allow(clippy::cast_precision_loss)]
fn bench_normy_fra(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(
        BenchmarkId::new("Normy (FRA LowerCase+Transliterate)", scenario),
        |b| {
            b.iter(|| {
                total += 1;
                let result = NORMY_FRA_PIPELINE.normalize(black_box(corpus)).unwrap();
                if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr()) {
                    zero_copy_hits += 1;
                }
                black_box(result);
            });
        },
    );

    let pct = if total > 0 {
        (zero_copy_hits as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    println!("  Normy FRA - {scenario:25}: zero-copy {zero_copy_hits:4}/{total:4} ({pct:5.2}%)");
}

#[allow(clippy::cast_precision_loss)]
fn bench_normy_fused_fra(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(
        BenchmarkId::new("Normy Fused (FRA LowerCase+Transliterate)", scenario),
        |b| {
            b.iter(|| {
                total += 1;
                let result = NORMY_FRA_PIPELINE
                    .normalize_fused(black_box(corpus))
                    .unwrap();
                if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr()) {
                    zero_copy_hits += 1;
                }
                black_box(result);
            });
        },
    );

    let pct = if total > 0 {
        (zero_copy_hits as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "  Normy Fused FRA - {scenario:25}: zero-copy {zero_copy_hits:4}/{total:4} ({pct:5.2}%)"
    );
}

#[allow(clippy::cast_precision_loss)]
fn bench_normy_static_fused_fra(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(
        BenchmarkId::new("Normy Static Fused (FRA LowerCase+Transliterate)", scenario),
        |b| {
            b.iter(|| {
                total += 1;
                let result = NORMY_FRA_PIPELINE
                    .normalize_static_fused(black_box(corpus))
                    .unwrap();
                if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr()) {
                    zero_copy_hits += 1;
                }
                black_box(result);
            });
        },
    );

    let pct = if total > 0 {
        (zero_copy_hits as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "  Normy Static Fused FRA - {scenario:25}: zero-copy {zero_copy_hits:4}/{total:4} ({pct:5.2}%)"
    );
}

fn bench_tokenizers_fra(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    group.bench_function(
        BenchmarkId::new("tokenizers (StripAccents+Lowercase)", scenario),
        |b| {
            b.iter(|| {
                let mut ns = NormalizedString::from(black_box(corpus));
                HF_NORMALIZER.normalize(&mut ns).unwrap();
                black_box(ns.get());
            });
        },
    );
    println!("  tokenizers  - {scenario:25}: always allocates (0% zero-copy)");
}

// ── Main benchmark ──
fn bench_german_normalizers(c: &mut Criterion) {
    let mut group = c.benchmark_group("German (DEU) - CaseFold");
    group.throughput(Throughput::Bytes(64 * 1024));
    group.sample_size(200);
    group.measurement_time(std::time::Duration::from_secs(10));

    let scenarios = [
        ("german_with_eszett", &*CORPUS_GERMAN),
        ("already_normalized", &*CORPUS_NORMALIZED),
    ];

    for (scenario, corpus) in scenarios {
        println!("\n[German: {scenario}]");
        bench_normy_deu(&mut group, scenario, corpus);
        bench_unidecode_deu(&mut group, scenario, corpus);
        bench_normy_fused_deu(&mut group, scenario, corpus);
        bench_normy_static_fused_deu(&mut group, scenario, corpus);
    }

    group.finish();
}

fn bench_french_normalizers(c: &mut Criterion) {
    let mut group = c.benchmark_group("French (FRA) - Transliterate+LowerCase");
    group.throughput(Throughput::Bytes(64 * 1024));
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(10));

    let scenarios = [
        ("french_with_accents", &*CORPUS_FRENCH),
        ("already_normalized", &*CORPUS_NORMALIZED),
    ];

    for (scenario, corpus) in scenarios {
        println!("\n[French: {scenario}]");
        bench_normy_fra(&mut group, scenario, corpus);
        bench_tokenizers_fra(&mut group, scenario, corpus);
        bench_normy_fused_fra(&mut group, scenario, corpus);
        bench_normy_static_fused_fra(&mut group, scenario, corpus);
    }

    group.finish();
}

criterion_group!(benches, bench_german_normalizers, bench_french_normalizers);
criterion_main!(benches);

// ── TESTS: Verify semantic equivalence BEFORE benchmarking ──
#[cfg(test)]
mod tests {
    use crate::{
        CORPUS_FRENCH, CORPUS_GERMAN, CORPUS_NORMALIZED, HF_NORMALIZER, NORMY_DEU_PIPELINE,
        NORMY_FRA_PIPELINE, french_pool, german_pool,
    };
    use std::borrow::Cow;
    use tokenizers::{NormalizedString, Normalizer};
    use unidecode::unidecode;

    #[test]
    fn test_german_vs_unidecode_semantic_equivalence() {
        for input in german_pool() {
            let normy_result = NORMY_DEU_PIPELINE.normalize(input).unwrap();
            let normy_fused_result = NORMY_DEU_PIPELINE.normalize_fused(input).unwrap();
            let unidecode_result = unidecode(input).to_lowercase();

            assert_eq!(
                normy_result.as_ref(),
                unidecode_result,
                "\n❌ SEMANTIC MISMATCH on German input: {input:?}\n\
                 Normy (DEU):  {normy_result:?}\n\
                 unidecode:    {unidecode_result:?}\n"
            );

            assert_eq!(
                normy_fused_result.as_ref(),
                unidecode_result,
                "\n❌ SEMANTIC MISMATCH on German input: {input:?}\n\
                 Normy Fused (DEU):  {normy_fused_result:?}\n\
                 unidecode:    {unidecode_result:?}\n"
            );
        }
    }

    #[test]
    fn test_french_vs_tokenizers_semantic_equivalence() {
        for input in french_pool() {
            let normy_result = NORMY_FRA_PIPELINE.normalize(input).unwrap();
            let normy_fused_result = NORMY_FRA_PIPELINE.normalize_fused(input).unwrap();

            let mut ns = NormalizedString::from(*input);
            HF_NORMALIZER.normalize(&mut ns).unwrap();
            let hf_result = ns.get();

            assert_eq!(
                normy_result.as_ref(),
                hf_result,
                "\n❌ SEMANTIC MISMATCH on French input: {input:?}\n\
                 Normy (FRA):  {normy_result:?}\n\
                 tokenizers:   {hf_result:?}\n"
            );
            assert_eq!(
                normy_fused_result.as_ref(),
                hf_result,
                "\n❌ SEMANTIC MISMATCH on French input: {input:?}\n\
                 Normy Fused (FRA):  {normy_fused_result:?}\n\
                 tokenizers:   {hf_result:?}\n"
            );
        }
    }

    #[test]
    fn test_german_corpus_semantic_correctness() {
        let normy_result = NORMY_DEU_PIPELINE.normalize(&CORPUS_GERMAN).unwrap();
        let unidecode_result = unidecode(&CORPUS_GERMAN).to_lowercase();

        assert_eq!(
            normy_result.len(),
            unidecode_result.len(),
            "❌ Length mismatch on German corpus"
        );

        // Sample check first 500 chars
        let sample_len = 500.min(normy_result.len());
        assert_eq!(
            &normy_result.as_ref()[..sample_len],
            &unidecode_result[..sample_len],
            "❌ Content mismatch on German corpus (first 500 chars)"
        );
    }

    #[test]
    fn test_french_corpus_semantic_correctness() {
        let normy_result = NORMY_FRA_PIPELINE.normalize(&CORPUS_FRENCH).unwrap();

        let mut ns = NormalizedString::from(CORPUS_FRENCH.as_str());
        HF_NORMALIZER.normalize(&mut ns).unwrap();
        let hf_result = ns.get();

        assert_eq!(
            normy_result.len(),
            hf_result.len(),
            "❌ Length mismatch on French corpus"
        );

        // Sample check first 500 chars
        let sample_len = 500.min(normy_result.len());
        assert_eq!(
            &normy_result.as_ref()[..sample_len],
            &hf_result[..sample_len],
            "❌ Content mismatch on French corpus (first 500 chars)"
        );
    }

    #[test]
    fn test_zero_copy_on_already_normalized() {
        let already_normalized = "hello world this is lowercase ascii";

        // German
        let result = NORMY_DEU_PIPELINE.normalize(already_normalized).unwrap();
        assert!(
            matches!(result, Cow::Borrowed(s) if s.as_ptr() == already_normalized.as_ptr()),
            "❌ Zero-copy FAILED for German on already-normalized input"
        );

        // French
        let result = NORMY_FRA_PIPELINE.normalize(already_normalized).unwrap();
        assert!(
            matches!(result, Cow::Borrowed(s) if s.as_ptr() == already_normalized.as_ptr()),
            "❌ Zero-copy FAILED for French on already-normalized input"
        );
    }

    #[test]
    fn test_normalized_corpus_zero_copy() {
        // German
        let result = NORMY_DEU_PIPELINE.normalize(&CORPUS_NORMALIZED).unwrap();
        assert!(
            matches!(result, Cow::Borrowed(s) if s.as_ptr() == CORPUS_NORMALIZED.as_ptr()),
            "❌ Zero-copy FAILED for German on normalized corpus"
        );

        // French
        let result = NORMY_FRA_PIPELINE.normalize(&CORPUS_NORMALIZED).unwrap();
        assert!(
            matches!(result, Cow::Borrowed(s) if s.as_ptr() == CORPUS_NORMALIZED.as_ptr()),
            "❌ Zero-copy FAILED for French on normalized corpus"
        );
    }
}
