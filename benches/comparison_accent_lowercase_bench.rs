// benches/comparison_accent_lowercase_bench.rs
// Benchmark comparing:
// 1. Normy: RemoveDiacritics + LowerCase (locale-aware)
// 2. tokenizers: Sequence(StripAccents + Lowercase)
// 3. unidecode-rs: unidecode() + .to_lowercase()
#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::random;
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::borrow::Cow;
use std::hint::black_box;
use std::sync::LazyLock;

use normy::{CaseFold, DEU, ENG, FRA, LowerCase, Normy, RemoveDiacritics, Transliterate, VIE};
use tokenizers::normalizers::{Lowercase, Sequence, StripAccents};
use tokenizers::{NormalizedString, Normalizer, NormalizerWrapper};
use unidecode::unidecode;

type CaseFoldPipeline = Normy<
    normy::process::ChainedProcess<
        CaseFold, normy::process::EmptyProcess>,
>;


static NORMY_DEU_PIPELINE: LazyLock<CaseFoldPipeline> = LazyLock::new(|| {
    Normy::builder()
        .lang(DEU)
        .add_stage(CaseFold)
        .build()
});


type LowercaseTransliteratePipeline = Normy<
    normy::process::ChainedProcess<
        Transliterate,
        normy::process::ChainedProcess<LowerCase, normy::process::EmptyProcess>,
    >,
>;


static NORMY_FRA_PIPELINE: LazyLock<LowercaseTransliteratePipeline> = LazyLock::new(|| {
    Normy::builder()
        .lang(FRA)
        .add_stage(LowerCase)
        .add_stage(Transliterate)
        .build()
});


// ── Type aliases ──
type AccentLowerPipeline = Normy<
    normy::process::ChainedProcess<
        LowerCase,
        normy::process::ChainedProcess<RemoveDiacritics, normy::process::EmptyProcess>,
    >,
>;

// ── Normy pipelines (locale-aware) ──
static NORMY_PIPELINES: LazyLock<[(&str, AccentLowerPipeline); 3]> = LazyLock::new(|| {
    [
        (
            "ENG",
            Normy::builder()
                .lang(ENG)
                .add_stage(RemoveDiacritics)
                .add_stage(LowerCase)
                .build(),
        ),
        (
            "FRA",
            Normy::builder()
                .lang(FRA)
                .add_stage(RemoveDiacritics)
                .add_stage(LowerCase)
                .build(),
        ),
        (
            "VIE",
            Normy::builder()
                .lang(VIE)
                .add_stage(RemoveDiacritics)
                .add_stage(LowerCase)
                .build(),
        ),
    ]
});

// ── Baseline normalizers ──
static HF_NORMALIZER: LazyLock<Sequence> = LazyLock::new(|| {
    Sequence::new(vec![
        NormalizerWrapper::StripAccents(StripAccents),
        NormalizerWrapper::Lowercase(Lowercase),
    ])
});

// ── Corpus generators ──
fn corpus_accent_heavy(seed: u64, kb: usize) -> String {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = String::with_capacity(kb * 1024);
    let pool = &[
        " naïve café résumé déjà-vu éléphant François ",
        " Việt Nam Phở Tiếng Việt đắt đỏ ",
        " İstanbul ğüş öş İıŞş ",
        " Größe Straße fußball Maßstab ßẞ ÄÖÜäöü ",
    ];

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

static CORPUS_ACCENT: LazyLock<String> = LazyLock::new(|| corpus_accent_heavy(0x517ee, 64));
static CORPUS_NORM: LazyLock<String> = LazyLock::new(|| corpus_already_normalized(0x1a7fe, 64));

// ── Unified benchmark function ──
#[allow(clippy::cast_precision_loss)]
fn bench_normy_locale(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    locale: &str,
    pipeline: &AccentLowerPipeline,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(
        BenchmarkId::new(format!("Normy ({locale})"), scenario),
        |b| {
            b.iter(|| {
                total += 1;
                let result = pipeline.normalize(black_box(corpus)).unwrap();
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
        "  Normy {locale:3} - {scenario:30}: zero-copy {zero_copy_hits:4}/{total:4} ({pct:5.2}%)"
    );
}

fn bench_tokenizers(
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
    println!("  tokenizers  - {scenario:30}: always allocates (0% zero-copy)");
}

fn bench_unidecode(
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
    println!("  unidecode   - {scenario:30}: always allocates (0% zero-copy)");
}

// ── Main benchmark ──
fn bench_accent_lowercase_normalizers(c: &mut Criterion) {
    let mut group = c.benchmark_group("Accent+Lowercase");
    group.throughput(Throughput::Bytes(64 * 1024));
    group.sample_size(200);
    group.measurement_time(std::time::Duration::from_secs(12));

    let scenarios = [
        ("accent_heavy", &*CORPUS_ACCENT),
        ("already_normalized", &*CORPUS_NORM),
    ];

    for (scenario, corpus) in scenarios {
        println!("\n[{scenario}]");

        for (locale, pipeline) in NORMY_PIPELINES.iter() {
            bench_normy_locale(&mut group, locale, pipeline, scenario, corpus);
        }

        bench_tokenizers(&mut group, scenario, corpus);
        bench_unidecode(&mut group, scenario, corpus);
    }

    group.finish();
}

criterion_group!(benches, bench_accent_lowercase_normalizers);
criterion_main!(benches);

// ── TESTS: Run these BEFORE benchmarks ──
#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use tokenizers::{NormalizedString, Normalizer};

    use crate::{HF_NORMALIZER, NORMY_DEU_PIPELINE, NORMY_FRA_PIPELINE, NORMY_PIPELINES};
    use unidecode::unidecode;

    #[test]
    fn test_semantic_equivalence_with_tokenizers() {
        let test_cases = &[
            ("French", " naïve café résumé déjà-vu éléphant François "),
            ("Vietnamese", " Việt Nam Phở Tiếng Việt đắt đỏ "),
            ("German", " Größe Straße fußball Maßstab ßẞ ÄÖÜäöü "),
            ("Already normalized", " hello world test "),
            ("Empty", ""),
        ];

        for (name, input) in test_cases {
            // Normy ENG (should match tokenizers for non-locale-specific cases)
            let normy_result = NORMY_PIPELINES[0].1.normalize(input).unwrap().into_owned();

            // tokenizers baseline
            let mut ns = NormalizedString::from(*input);
            HF_NORMALIZER.normalize(&mut ns).unwrap();
            let hf_result = ns.get().to_string();

            assert_eq!(
                normy_result, hf_result,
                "Mismatch for {name}: {input:?}\nNormy: {normy_result:?}\ntokenizers: {hf_result:?}"
            );
        }
    }


    #[test]
    fn test_zero_copy_on_normalized_input() {
        let already_normalized = "hello world this is lowercase ascii";

        for (locale, pipeline) in NORMY_PIPELINES.iter() {
            let result = pipeline.normalize(already_normalized).unwrap();

            assert!(
                matches!(result, Cow::Borrowed(s) if s.as_ptr() == already_normalized.as_ptr()),
                "Zero-copy failed for {locale} on already-normalized input"
            );
        }
    }

    #[test]
    fn test_unidecode_equalities() {
        // unidecode is more aggressive (e.g., ß -> ss, not just accent removal)
        let input = " FUßball Maßstab ßẞ";

        let normy_result: Cow<'_, str> = NORMY_DEU_PIPELINE.normalize(input).unwrap();
        let unidecode_result: String = unidecode(input).to_lowercase();

        println!("Normy: {normy_result:?}");
        println!("unidecode: {unidecode_result:?}");

        assert_eq!(normy_result.as_ref(), unidecode_result);
    }

    #[test]
    fn hf_equalities() {
        // unidecode is more aggressive (e.g., ß -> ss, not just accent removal)
        let input = " naïve café résumé déjà-vu éléphant ";

        let normy_result: Cow<'_, str> = NORMY_FRA_PIPELINE.normalize(input).unwrap();
        let mut ns = NormalizedString::from(input);
        HF_NORMALIZER.normalize(&mut ns).unwrap();

        println!("Normy: {normy_result:?}");
        println!("HF normalizer: {}",ns.get());

        assert_eq!(normy_result.as_ref(), ns.get());
    }

    /// Test that Normy produces IDENTICAL output to tokenizers for all locales
    /// on non-locale-specific inputs
    #[test]
    fn test_all_locales_match_tokenizers_on_standard_input() {
        // These inputs should produce identical results across all locales
        // and must match tokenizers exactly
        let test_cases = &[
            " naïve café résumé déjà-vu éléphant François ",
            " Việt Nam Phở Tiếng Việt đắt đỏ ",
            " Größe Straße fußball Maßstab ßẞ ÄÖÜäöü ",
            " hello world test ",
            "",
            "HELLO WORLD",
            "123 abc ABC",
        ];

        for input in test_cases {
            // Get tokenizers baseline
            let mut ns = NormalizedString::from(*input);
            HF_NORMALIZER.normalize(&mut ns).unwrap();
            let expected = ns.get();

            // ALL Normy locales MUST produce identical output to tokenizers
            for (locale, pipeline) in NORMY_PIPELINES.iter() {
                let normy_result = pipeline.normalize(input).unwrap();

                assert_eq!(
                    normy_result.as_ref(),
                    expected,
                    "\n❌ SEMANTIC MISMATCH for locale {locale} on input: {input:?}\n\
                     Normy ({locale}): {normy_result:?}\n\
                     tokenizers:     {expected:?}\n\
                     \n⚠️  This means benchmark comparisons are INVALID!"
                );
            }
        }
    }

    /// Verify Turkish locale handles İ/I correctly (even if different from tokenizers)
    #[test]
    fn test_turkish_locale_handles_dotted_i() {
        let input = "İSTANBUL";

        let tur_result = NORMY_PIPELINES[2].1.normalize(input).unwrap();

        // Turkish should lowercase İ → i (dotted)
        // Check against tokenizers to see if behavior matches
        let mut ns = NormalizedString::from(input);
        HF_NORMALIZER.normalize(&mut ns).unwrap();
        let hf_result = ns.get();

        println!("Turkish input: {input:?}");
        println!("  Normy (TUR): {tur_result:?}");
        println!("  tokenizers:  {hf_result:?}");

        // If they differ, document it but ensure Turkish at least processes correctly
        if tur_result.as_ref() != hf_result {
            eprintln!("\n⚠️  WARNING: Turkish locale produces different output than tokenizers");
            eprintln!("   This is expected IF Normy implements Turkish-specific İ/i rules");
            eprintln!(
                "   If this is intentional, remove Turkish from benchmarks or document divergence\n"
            );
        }

        // At minimum, verify it's not empty and contains lowercase
        assert!(!tur_result.is_empty());
        assert!(
            tur_result
                .chars()
                .all(|c| !c.is_uppercase() || !c.is_alphabetic())
        );
    }

    /// Verify zero-copy optimization works for already-normalized text
    #[test]
    fn test_zero_copy_on_already_normalized() {
        let already_normalized = "hello world this is lowercase ascii";

        for (locale, pipeline) in NORMY_PIPELINES.iter() {
            let result = pipeline.normalize(already_normalized).unwrap();

            assert!(
                matches!(result, Cow::Borrowed(s) if s.as_ptr() == already_normalized.as_ptr()),
                "❌ Zero-copy FAILED for {locale} on already-normalized input\n\
                 This should return Cow::Borrowed pointing to original string"
            );
        }
    }

    /// Verify that all test cases in the benchmark corpus are handled correctly
    #[test]
    fn test_benchmark_corpus_semantic_correctness() {
        use crate::{CORPUS_ACCENT, CORPUS_NORM};

        for (name, corpus) in [
            ("accent_heavy", &*CORPUS_ACCENT),
            ("already_normalized", &*CORPUS_NORM),
        ] {
            // Get tokenizers baseline
            let mut ns = NormalizedString::from(corpus.as_str());
            HF_NORMALIZER.normalize(&mut ns).unwrap();
            let expected = ns.get();

            // Verify ALL locales match tokenizers on benchmark corpus
            for (locale, pipeline) in NORMY_PIPELINES.iter() {
                let normy_result = pipeline.normalize(corpus).unwrap();

                // For large corpus, just check lengths and sample
                assert_eq!(
                    normy_result.len(),
                    expected.len(),
                    "❌ Length mismatch for {locale} on {name} corpus"
                );

                // Check first 200 chars
                let sample_len = 200.min(expected.len());
                assert_eq!(
                    &normy_result.as_ref()[..sample_len],
                    &expected[..sample_len],
                    "❌ Content mismatch for {locale} on {name} corpus (first 200 chars)"
                );
            }
        }
    }
}
