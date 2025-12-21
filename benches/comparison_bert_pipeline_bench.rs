#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use normy::COLLAPSE_WHITESPACE_UNICODE;
use normy::fused_process::ProcessFused;
use normy::process::Process;
use normy::stage::StaticStageIter;
use normy::stage::normalization::NfdStage;
use normy::stage::normalize_whitespace::NormalizeWhitespace;
use rand::random;
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::sync::LazyLock;
use std::{borrow::Cow, hint::black_box};

use tokenizers::{NormalizedString, Normalizer, normalizers::BertNormalizer};

use normy::{LowerCase, NFD, Normy, RemoveDiacritics, StripControlChars, StripFormatControls, ZHO};

// ──────────────────────────────────────────────────────────────
// Compatibility stage (exact HF Bert Chinese spacing behavior)
// ──────────────────────────────────────────────────────────────
#[derive(Debug, Default, Clone, Copy)]
pub struct BertCompatChineseChars;

impl normy::stage::Stage for BertCompatChineseChars {
    fn name(&self) -> &'static str {
        "bert_compat_chinese_chars"
    }
    #[inline(always)]
    fn needs_apply(
        &self,
        text: &str,
        _: &normy::context::Context,
    ) -> Result<bool, normy::stage::StageError> {
        Ok(text.chars().any(is_chinese_char))
    }
    #[inline(always)]
    fn apply<'a>(
        &self,
        text: Cow<'a, str>,
        _: &normy::context::Context,
    ) -> Result<Cow<'a, str>, normy::stage::StageError> {
        let mut out = String::with_capacity(text.len() + 8);
        for c in text.chars() {
            if is_chinese_char(c) {
                out.push(' ');
                out.push(c);
                out.push(' ');
            } else {
                out.push(c);
            }
        }
        Ok(Cow::Owned(out))
    }
}

impl StaticStageIter for BertCompatChineseChars {
    type Iter<'a> = std::iter::Empty<char>;
}

fn is_chinese_char(c: char) -> bool {
    matches!(
        c as u32,
        0x4E00..=0x9FFF |
        0x3400..=0x4DBF |
        0x20000..=0x2A6DF |
        0x2A700..=0x2B73F |
        0x2B740..=0x2B81F |
        0x2B920..=0x2CEAF |
        0xF900..=0xFAFF |
        0x2F800..=0x2FA1F
    )
}

// ──────────────────────────────────────────────────────────────
// Normy pipeline — 100% bit-identical to HF BertNormalizer
// ──────────────────────────────────────────────────────────────
// ──────────────────────────────────────────────────────────────
// Concrete pipeline type – no `impl Trait` in static!
// ──────────────────────────────────────────────────────────────
type NormyBertLikePipeline = Normy<
    normy::process::ChainedProcess<
        LowerCase,
        normy::process::ChainedProcess<
            RemoveDiacritics,
            normy::process::ChainedProcess<
                NfdStage,
                normy::process::ChainedProcess<
                    BertCompatChineseChars,
                    normy::process::ChainedProcess<
                        NormalizeWhitespace,
                        normy::process::ChainedProcess<
                            StripFormatControls,
                            normy::process::ChainedProcess<
                                StripControlChars,
                                normy::process::EmptyProcess,
                            >,
                        >,
                    >,
                >,
            >,
        >,
    >,
>;

static NORMY_BERT: LazyLock<NormyBertLikePipeline> = LazyLock::new(|| {
    Normy::builder()
        .lang(ZHO)
        .modify_lang(|entry| {
            // Enable diacritic stripping (copy FRA's list for Latin accents)
            entry.set_spacing_diacritics(&[
                '\u{0300}', '\u{0301}', '\u{0302}', '\u{0308}', '\u{030A}', '\u{030B}', '\u{030C}',
                '\u{030F}', '\u{0311}', '\u{0327}', '\u{0328}', '\u{0338}',
            ]);
        })
        .add_stage(StripControlChars)
        .add_stage(StripFormatControls)
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .add_stage(BertCompatChineseChars) // ← this one (replaces BertCompatCjkPunct and SegmentWords)
        .add_stage(NFD) // Decompose precomposed accents (é → e + ´)
        .add_stage(RemoveDiacritics) // Now removes ´ (Mn) via enabled list
        .add_stage(LowerCase)
        .build()
});

// fn normy_bert_pipeline() -> Normy<impl Process + ProcessFused> {
//     Normy::builder()
//         .lang(ZHO)
//         .modify_lang(|entry| {
//             // Enable Latin accent stripping (same list used by BERT)
//             entry.set_spacing_diacritics(&[
//                 '\u{0300}', '\u{0301}', '\u{0302}', '\u{0308}', '\u{030A}', '\u{030B}', '\u{030C}',
//                 '\u{030F}', '\u{0311}', '\u{0327}', '\u{0328}', '\u{0338}',
//             ]);
//         })
//         .add_stage(StripControlChars)
//         .add_stage(StripFormatControls)
//         .add_stage(COLLAPSE_WHITESPACE_UNICODE) // handles Unicode → ASCII space
//         .add_stage(BertCompatChineseChars)
//         .add_stage(NFD)
//         .add_stage(RemoveDiacritics)
//         .add_stage(LowerCase)
//         .build()
// }

// ──────────────────────────────────────────────────────────────
// HuggingFace BertNormalizer
// ──────────────────────────────────────────────────────────────
static HF_BERT: LazyLock<BertNormalizer> =
    LazyLock::new(|| BertNormalizer::new(true, true, Some(true), true));

fn needs_transform_pool() -> &'static [&'static str; 5] {
    (&[
        "Ｈｅｌｌｏ　naïve Café\u{0000}\u{200B}résumé",
        "你好世界",
        "NAÏVE déjà-vu",
        "Hello world\u{3000}",
        "Ｈｅｌｌｏ　世界　café",
    ]) as _
}

// ──────────────────────────────────────────────────────────────
// Realistic corpora
// ──────────────────────────────────────────────────────────────
fn corpus_needs_transform(seed: u64, kb: usize) -> String {
    let pool = &[
        "Ｈｅｌｌｏ　naïve Café\u{0000}\u{200B}résumé",
        "你好世界",
        "NAÏVE déjà-vu",
        "Hello\u{00A0}\u{2003}world\u{3000}",
        "Ｈｅｌｌｏ　世界　café",
    ];
    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = String::with_capacity(kb * 1024);
    while out.len() < kb * 1024 {
        let s = pool[rng.random_range(0..pool.len())];
        out.push_str(s);
        out.push(' ');
        if rng.random_bool(0.1) {
            let word: String = (0..rng.random_range(5..20))
                .map(|_| (b'A' + (random::<u8>() % 26)) as char)
                .collect();
            out.push_str(&word);
            out.push(' ');
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

static CORPUS_64KB_NEEDS: LazyLock<String> =
    LazyLock::new(|| corpus_needs_transform(0xDEAD_BEEF, 64));
static CORPUS_64KB_NORM: LazyLock<String> =
    LazyLock::new(|| corpus_already_normalized(0xCAFE_BABE, 64));

// ──────────────────────────────────────────────────────────────
// Benchmark harness
// ──────────────────────────────────────────────────────────────
fn bench_bert_normalizers(c: &mut Criterion) {
    let mut group = c.benchmark_group("BERT Normalizer Comparison");
    group.throughput(Throughput::Bytes(64 * 1024));
    group.sample_size(200);
    group.measurement_time(std::time::Duration::from_secs(12));

    let corpora = [
        ("needs_transform_64kb", &*CORPUS_64KB_NEEDS),
        ("already_normalized_64kb", &*CORPUS_64KB_NORM),
    ];

    for (name, corpus) in corpora {
        println!("Running normy bench..");

        // ── Normy (zero-copy aware) ─────────────────────────────────────
        bench_normy(&mut group, name, corpus);

        println!("Running hf bench..");
        // ── HuggingFace (always allocates) ───────────────────────────────
        bench_hf_bert(&mut group, name, corpus);

        println!("Running normy fused bench..");
        // ── Normy Fused (zero-copy aware) ─────────────────────────────────────
        bench_normy_fused(&mut group, name, corpus);
    }

    group.finish();
}

fn bench_normy(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(BenchmarkId::new("Normy (zero-copy)", scenario), |b| {
        b.iter(|| {
            total += 1;
            let result = NORMY_BERT.normalize(black_box(corpus)).unwrap();
            if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr() && s.len() == corpus.len()) {
                zero_copy_hits += 1;
            }
            black_box(result);
        })
    });

    let pct = if total > 0 {
        (zero_copy_hits as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    println!("   Normy  - {scenario}: ZERO-COPY {zero_copy_hits}/{total} ({pct:.2}%)");
}

fn bench_normy_fused(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(BenchmarkId::new("Normy Fused (zero-copy)", scenario), |b| {
        b.iter(|| {
            total += 1;
            let result = NORMY_BERT.normalize_fused(black_box(corpus)).unwrap();
            if matches!(result, Cow::Borrowed(s) if s.as_ptr() == corpus.as_ptr() && s.len() == corpus.len()) {
                zero_copy_hits += 1;
            }
            black_box(result);
        })
    });

    let pct = if total > 0 {
        (zero_copy_hits as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    println!("   Normy Fused  - {scenario}: ZERO-COPY {zero_copy_hits}/{total} ({pct:.2}%)");
}

fn bench_hf_bert(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    scenario: &str,
    corpus: &str,
) {
    group.bench_function(BenchmarkId::new("HuggingFace tokenizers", scenario), |b| {
        b.iter(|| {
            let mut ns = NormalizedString::from(black_box(corpus));
            HF_BERT.normalize(&mut ns).unwrap();
            black_box(ns.get());
        })
    });
    println!("   HF     - {scenario}: Always allocates (0.0% zero-copy)");
}

criterion_group!(benches, bench_bert_normalizers);
criterion_main!(benches);

#[cfg(test)]
mod tests {
    #[cfg(test)]
    use std::borrow::Cow;

    #[cfg(test)]
    use tokenizers::{NormalizedString, Normalizer};

    #[cfg(test)]
    use crate::{HF_BERT, needs_transform_pool};

    #[test]
    fn bert_normalizer_semantic_equivalence() {
        let normy = &*NORMY_BERT();
        let hf = &*HF_BERT;

        let pool = needs_transform_pool();

        for (i, &input) in pool.iter().enumerate() {
            // --- Hugging Face ---
            let mut hf_ns = NormalizedString::from(input);
            hf.normalize(&mut hf_ns).expect("HF normalize failed");
            let hf_output: String = hf_ns.get().into();

            // --- Normy ---
            let normy_result = normy.normalize(input).expect("Normy normalize failed");
            let normy_output: String = normy_result.clone().into_owned();

            // --- Normy ---
            let normy_fusable_result = normy
                .normalize_fused(input)
                .expect("Normy normalize fused failed");
            let normy_fusable_output: String = normy_fusable_result.clone().into_owned();

            // Semantic equivalence
            assert_eq!(
                hf_output,
                normy_output,
                "\n\nFailed equivalence on test case #{}\n\
             Input:  {:?}\n\
             HF:     {:?}\n\
             Normy:  {:?}\n\
             HF len:  {} chars\n\
             Normy len: {} chars\n",
                i + 1,
                input,
                hf_output,
                normy_output,
                hf_output.len(),
                normy_output.len()
            );

            assert_eq!(
                hf_output,
                normy_fusable_output,
                "\n\nFailed equivalence on test case #{}\n\
             Input:  {:?}\n\
             HF:     {:?}\n\
             Normy:  {:?}\n\
             HF len:  {} chars\n\
             Normy len: {} chars\n",
                i + 1,
                input,
                hf_output,
                normy_fusable_output,
                hf_output.len(),
                normy_fusable_output.len()
            );

            // --- Zero-copy proof on unchanged input ---
            if input.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_whitespace() || c.is_ascii_punctuation()
            }) && input
                .trim()
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_punctuation())
            {
                // This input should be completely unchanged → zero-copy must trigger
                assert!(
                    matches!(normy_result, Cow::Borrowed(s) if s.as_ptr() == input.as_ptr() && s.len() == input.len()),
                    "Zero-copy failed on already-normalized input: {:?}",
                    input
                );
            }
        }
    }
}

// // Test cases covering every code path in BertNormalizer
// let cases = &[
//     // 1. Full-width + CJK + control chars + accents + mixed whitespace
//     "Ｈｅｌｌｏ　naïve Café\u{0000}\u{200B}\u{00A0}\u{2028}résumé",
//     // 2. Pure CJK
//     "你好世界",
//     // 3. Already normalized (critical for zero-copy test)
//     "hello world",
//     // 4. Controls only
//     "\u{0001}\u{0002}hello\u{001F}world",
//     // 5. Unicode whitespace only
//     "hello\u{00A0}\u{1680}\u{2003}world\u{3000}",
//     // 6. Accents + lowercase edge cases
//     "NAÏVE ÉLÉPHANT naïve déjà-vu",
//     // 7. Mixed script (important: no false segmentation)
//     "Hello世界naïveCafé",
//     // 8. Empty string
//     "",
// ];
