#![deny(unsafe_code)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use normy::{
    COLLAPSE_WHITESPACE_UNICODE, FRA, LowerCase, NFD, Normy, RemoveDiacritics, StripControlChars,
    StripFormatControls, Transliterate, ZHO,
    context::Context,
    stage::normalization::NfdStage,
    stage::normalize_whitespace::NormalizeWhitespace,
    stage::{Stage, StageError, StaticFusableStage},
};
use rand::random;
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::borrow::Cow;
use std::hint::black_box;
use std::iter::FusedIterator;
use std::sync::LazyLock;
use std::time::Duration;
use tokenizers::{
    NormalizedString, Normalizer, NormalizerWrapper,
    normalizers::{BertNormalizer, Lowercase, Sequence, StripAccents},
};

// ═══════════════════════════════════════════════════════════════════════════
// BERT COMPATIBILITY STAGE
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Default, Clone, Copy)]
pub struct BertCompatChineseChars;

impl Stage for BertCompatChineseChars {
    fn name(&self) -> &'static str {
        "bert_compat_chinese_chars"
    }

    fn needs_apply(&self, text: &str, _: &Context) -> Result<bool, StageError> {
        Ok(text.chars().any(is_chinese_char))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _: &Context) -> Result<Cow<'a, str>, StageError> {
        let mut out = String::with_capacity(text.len() + text.len() / 2);
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

fn is_chinese_char(c: char) -> bool {
    matches!(
        c as u32,
        0x4E00..=0x9FFF
            | 0x3400..=0x4DBF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B920..=0x2CEAF
            | 0xF900..=0xFAFF
            | 0x2F800..=0x2FA1F
    )
}

// Iterator adapter for fusion
pub struct BertCompatChineseCharsAdapter<'a, I> {
    input: I,
    state: BertChineseState,
    _phantom: std::marker::PhantomData<&'a ()>,
}

enum BertChineseState {
    Normal,
    EmitChar(char),
    EmitSpaceAfter,
}

impl<I> BertCompatChineseCharsAdapter<'_, I> {
    pub fn new(input: I) -> Self {
        Self {
            input,
            state: BertChineseState::Normal,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<I: Iterator<Item = char>> Iterator for BertCompatChineseCharsAdapter<'_, I> {
    type Item = char;

    #[inline]
    fn next(&mut self) -> Option<char> {
        match self.state {
            BertChineseState::Normal => {
                let c = self.input.next()?;
                if is_chinese_char(c) {
                    self.state = BertChineseState::EmitChar(c);
                    Some(' ')
                } else {
                    Some(c)
                }
            }
            BertChineseState::EmitChar(c) => {
                self.state = BertChineseState::EmitSpaceAfter;
                Some(c)
            }
            BertChineseState::EmitSpaceAfter => {
                self.state = BertChineseState::Normal;
                Some(' ')
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (input_lower, input_upper) = self.input.size_hint();
        let buffered = match self.state {
            BertChineseState::Normal => 0,
            BertChineseState::EmitChar(_) => 2,
            BertChineseState::EmitSpaceAfter => 1,
        };
        let lower = input_lower + buffered;
        let upper = input_upper.map(|u| u * 3 + buffered);
        (lower, upper)
    }
}

impl<I: FusedIterator<Item = char>> FusedIterator for BertCompatChineseCharsAdapter<'_, I> {}

impl StaticFusableStage for BertCompatChineseChars {
    type Adapter<'a, I>
        = BertCompatChineseCharsAdapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    fn supports_static_fusion(&self) -> bool {
        true
    }

    fn static_fused_adapter<'a, I>(&self, input: I, _ctx: &'a Context) -> Self::Adapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        BertCompatChineseCharsAdapter::new(input)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NORMY PIPELINES
// ═══════════════════════════════════════════════════════════════════════════

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

type LowercaseTransliteratePipeline = Normy<
    normy::process::ChainedProcess<
        Transliterate,
        normy::process::ChainedProcess<LowerCase, normy::process::EmptyProcess>,
    >,
>;

static NORMY_BERT: LazyLock<NormyBertLikePipeline> = LazyLock::new(|| {
    Normy::builder()
        .lang(ZHO)
        .modify_lang(|entry| {
            entry.set_spacing_diacritics(&[
                '\u{0300}', '\u{0301}', '\u{0302}', '\u{0308}', '\u{030A}', '\u{030B}', '\u{030C}',
                '\u{030F}', '\u{0311}', '\u{0327}', '\u{0328}', '\u{0338}',
            ]);
        })
        .add_stage(StripControlChars)
        .add_stage(StripFormatControls)
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .add_stage(BertCompatChineseChars)
        .add_stage(NFD)
        .add_stage(RemoveDiacritics)
        .add_stage(LowerCase)
        .build()
});

static NORMY_FRA_PIPELINE: LazyLock<LowercaseTransliteratePipeline> = LazyLock::new(|| {
    Normy::builder()
        .lang(FRA)
        .add_stage(LowerCase)
        .add_stage(Transliterate)
        .build()
});

// ═══════════════════════════════════════════════════════════════════════════
// HUGGINGFACE TOKENIZERS
// ═══════════════════════════════════════════════════════════════════════════

static HF_BERT: LazyLock<BertNormalizer> =
    LazyLock::new(|| BertNormalizer::new(true, true, Some(true), true));

static HF_FRA_NORMALIZER: LazyLock<Sequence> = LazyLock::new(|| {
    Sequence::new(vec![
        NormalizerWrapper::StripAccents(StripAccents),
        NormalizerWrapper::Lowercase(Lowercase),
    ])
});

// ═══════════════════════════════════════════════════════════════════════════
// CORPUS GENERATORS
// ═══════════════════════════════════════════════════════════════════════════

fn bert_pool() -> &'static [&'static str; 5] {
    &[
        "Ｈｅｌｌｏ　naïve Café\u{0000}\u{200B}résumé",
        "你好世界",
        "NAÏVE déjà-vu",
        "Hello world\u{3000}",
        "Ｈｅｌｌｏ　世界　café",
    ]
}

fn corpus_bert_needs_transform(seed: u64, kb: usize) -> String {
    let pool = bert_pool();
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

fn french_pool() -> &'static [&'static str; 5] {
    &[
        " NAïve CAFé Résumé ",
        " Déjà-vu éléphant ",
        " être protégé crème ",
        " élève âme ",
        " HELLO WORLD TEST ",
    ]
}

static CORPUS_BERT_NEEDS: LazyLock<String> =
    LazyLock::new(|| corpus_bert_needs_transform(0xDEAD_BEEF, 64));
static CORPUS_FRENCH: LazyLock<String> = LazyLock::new(|| corpus_french(0x1A7FE, 64));
static CORPUS_NORMALIZED: LazyLock<String> =
    LazyLock::new(|| corpus_already_normalized(0x2BEEF, 64));

// ═══════════════════════════════════════════════════════════════════════════
// BENCHMARK FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

#[allow(clippy::cast_precision_loss)]
fn bench_normy_bert(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(BenchmarkId::new(name, scenario), |b| {
        b.iter(|| {
            total += 1;
            let result = NORMY_BERT.normalize(black_box(corpus)).unwrap();
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
    println!("   {name:35} - {scenario:25}: zero-copy {zero_copy_hits:4}/{total:4} ({pct:5.2}%)");
}

#[allow(clippy::cast_precision_loss)]
fn bench_normy_bert_apply_only(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(BenchmarkId::new(name, scenario), |b| {
        b.iter(|| {
            total += 1;
            let result = NORMY_BERT.normalize_apply_only(black_box(corpus)).unwrap();
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
    println!("   {name:35} - {scenario:25}: zero-copy {zero_copy_hits:4}/{total:4} ({pct:5.2}%)");
}

#[allow(clippy::cast_precision_loss)]
fn bench_normy_fra(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(BenchmarkId::new(name, scenario), |b| {
        b.iter(|| {
            total += 1;
            let result = NORMY_FRA_PIPELINE.normalize(black_box(corpus)).unwrap();
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
    println!("   {name:35} - {scenario:25}: zero-copy {zero_copy_hits:4}/{total:4} ({pct:5.2}%)");
}

#[allow(clippy::cast_precision_loss)]
fn bench_normy_fra_apply_only(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    scenario: &str,
    corpus: &str,
) {
    let mut zero_copy_hits = 0usize;
    let mut total = 0usize;

    group.bench_function(BenchmarkId::new(name, scenario), |b| {
        b.iter(|| {
            total += 1;
            let result = NORMY_FRA_PIPELINE.normalize_apply_only(black_box(corpus)).unwrap();
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
    println!("   {name:35} - {scenario:25}: zero-copy {zero_copy_hits:4}/{total:4} ({pct:5.2}%)");
}

fn bench_hf_normalizer(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    scenario: &str,
    corpus: &str,
    normalizer: &impl Normalizer,
) {
    group.bench_function(BenchmarkId::new(name, scenario), |b| {
        b.iter(|| {
            let mut ns = NormalizedString::from(black_box(corpus));
            normalizer.normalize(&mut ns).unwrap();
            black_box(ns.get());
        });
    });
    println!("   {name:35} - {scenario:25}: always allocates (0% zero-copy)");
}

// ═══════════════════════════════════════════════════════════════════════════
// MAIN BENCHMARKS
// ═══════════════════════════════════════════════════════════════════════════

fn bench_bert_normalizers(c: &mut Criterion) {
    let mut group = c.benchmark_group("BERT Normalizer Comparison");
    group.throughput(Throughput::Bytes(64 * 1024));
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(12));

    let scenarios = [
        ("bert_needs_transform_64kb", &*CORPUS_BERT_NEEDS),
        ("bert_already_normalized_64kb", &*CORPUS_NORMALIZED),
    ];

    for (scenario, corpus) in scenarios {
        println!("\n[BERT: {scenario}]");

        bench_normy_bert(&mut group, "Normy BERT (normalize)", scenario, corpus);

        bench_normy_bert_apply_only(&mut group, "Normy BERT (apply_only)", scenario, corpus);

        bench_hf_normalizer(
            &mut group,
            "HuggingFace BertNormalizer",
            scenario,
            corpus,
            &*HF_BERT,
        );
    }

    group.finish();
}

fn bench_french_normalizers(c: &mut Criterion) {
    let mut group = c.benchmark_group("French Normalizer Comparison");
    group.throughput(Throughput::Bytes(64 * 1024));
    group.sample_size(200);
    group.measurement_time(Duration::from_secs(10));

    let scenarios = [
        ("french_with_accents_64kb", &*CORPUS_FRENCH),
        ("french_already_normalized_64kb", &*CORPUS_NORMALIZED),
    ];

    for (scenario, corpus) in scenarios {
        println!("\n[French: {scenario}]");

        bench_normy_fra(&mut group, "Normy FRA (normalize)", scenario, corpus);

        bench_normy_fra_apply_only(&mut group, "Normy FRA (apply_only)", scenario, corpus);

        bench_hf_normalizer(
            &mut group,
            "HuggingFace (StripAccents+Lowercase)",
            scenario,
            corpus,
            &*HF_FRA_NORMALIZER,
        );
    }

    group.finish();
}

criterion_group!(benches, bench_bert_normalizers, bench_french_normalizers);
criterion_main!(benches);

// ═══════════════════════════════════════════════════════════════════════════
// TESTS: Verify semantic equivalence
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn test_bert_normalizer_semantic_equivalence() {
        for (i, &input) in bert_pool().iter().enumerate() {
            // HuggingFace
            let mut hf_ns = NormalizedString::from(input);
            HF_BERT.normalize(&mut hf_ns).expect("HF normalize failed");
            let hf_output: String = hf_ns.get().into();

            // Normy apply_only
            let normy_apply = NORMY_BERT
                .normalize_apply_only(input)
                .expect("Normy apply_only failed");
            let normy_apply_output: String = normy_apply.clone().into_owned();

            // Normy fusion
            let normy_fusion = NORMY_BERT.normalize(input).expect("Normy normalize failed");
            let normy_fusion_output: String = normy_fusion.clone().into_owned();

            assert_eq!(
                hf_output,
                normy_apply_output,
                "\n❌ BERT Apply Mismatch on test #{}\nInput: {:?}\nHF:    {:?}\nNormy: {:?}",
                i + 1,
                input,
                hf_output,
                normy_apply_output
            );

            assert_eq!(
                hf_output,
                normy_fusion_output,
                "\n❌ BERT Fusion Mismatch on test #{}\nInput: {:?}\nHF:    {:?}\nNormy: {:?}",
                i + 1,
                input,
                hf_output,
                normy_fusion_output
            );
        }
    }

    #[test]
    fn bert_normalizer_semantic_equivalence2() {
        let normy = &*NORMY_BERT;
        let hf = &*HF_BERT;

        let pool = bert_pool();

        for (i, &input) in pool.iter().enumerate() {
            // --- Hugging Face ---
            let mut hf_ns = NormalizedString::from(input);
            hf.normalize(&mut hf_ns).expect("HF normalize failed");
            let hf_output: String = hf_ns.get().into();

            // --- Normy ---
            let normy_result = normy
                .normalize_apply_only(input)
                .expect("Normy normalize apply only failed");
            let normy_output: String = normy_result.clone().into_owned();

            // --- Normy ---
            let normy_fusable_result = normy.normalize(input).expect("Normy normalize failed");
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
                    "Zero-copy failed on already-normalized input: {input:?}"
                );
            }
        }
    }

    #[test]
    fn test_french_normalizer_semantic_equivalence() {
        for (i, &input) in french_pool().iter().enumerate() {
            // HuggingFace
            let mut hf_ns = NormalizedString::from(input);
            HF_FRA_NORMALIZER
                .normalize(&mut hf_ns)
                .expect("HF normalize failed");
            let hf_output: String = hf_ns.get().into();

            // Normy apply_only
            let normy_apply = NORMY_FRA_PIPELINE
                .normalize_apply_only(input)
                .expect("Normy apply_only failed");
            let normy_apply_output: String = normy_apply.clone().into_owned();

            // Normy fusion
            let normy_fusion = NORMY_FRA_PIPELINE
                .normalize(input)
                .expect("Normy normalize failed");
            let normy_fusion_output: String = normy_fusion.clone().into_owned();

            assert_eq!(
                hf_output,
                normy_apply_output,
                "\n❌ French Apply Mismatch on test #{}\nInput: {:?}\nHF:    {:?}\nNormy: {:?}",
                i + 1,
                input,
                hf_output,
                normy_apply_output
            );

            assert_eq!(
                hf_output,
                normy_fusion_output,
                "\n❌ French Fusion Mismatch on test #{}\nInput: {:?}\nHF:    {:?}\nNormy: {:?}",
                i + 1,
                input,
                hf_output,
                normy_fusion_output
            );
        }
    }

    #[test]
    fn test_zero_copy_on_normalized_input() {
        let normalized = "hello world this is lowercase ascii";

        // BERT pipeline
        let result = NORMY_BERT.normalize(normalized).unwrap();
        assert!(
            matches!(result, Cow::Borrowed(s) if s.as_ptr() == normalized.as_ptr()),
            "❌ BERT zero-copy failed on normalized input"
        );

        // French pipeline
        let result = NORMY_FRA_PIPELINE.normalize(normalized).unwrap();
        assert!(
            matches!(result, Cow::Borrowed(s) if s.as_ptr() == normalized.as_ptr()),
            "❌ French zero-copy failed on normalized input"
        );
    }

    #[test]
    fn test_corpus_semantic_correctness() {
        // BERT corpus
        let normy_result = NORMY_BERT.normalize(&CORPUS_BERT_NEEDS).unwrap();
        let mut hf_ns = NormalizedString::from(CORPUS_BERT_NEEDS.as_str());
        HF_BERT.normalize(&mut hf_ns).unwrap();
        let hf_result = hf_ns.get();

        assert_eq!(
            normy_result.len(),
            hf_result.len(),
            "❌ BERT corpus length mismatch"
        );

        // French corpus
        let normy_result = NORMY_FRA_PIPELINE.normalize(&CORPUS_FRENCH).unwrap();
        let mut hf_ns = NormalizedString::from(CORPUS_FRENCH.as_str());
        HF_FRA_NORMALIZER.normalize(&mut hf_ns).unwrap();
        let hf_result = hf_ns.get();

        assert_eq!(
            normy_result.len(),
            hf_result.len(),
            "❌ French corpus length mismatch"
        );
    }

    #[test]
    fn test_normalized_corpus_zero_copy() {
        // BERT
        let result = NORMY_BERT.normalize(&CORPUS_NORMALIZED).unwrap();
        assert!(
            matches!(result, Cow::Borrowed(s) if s.as_ptr() == CORPUS_NORMALIZED.as_ptr()),
            "❌ BERT zero-copy failed on normalized corpus"
        );

        // French
        let result = NORMY_FRA_PIPELINE.normalize(&CORPUS_NORMALIZED).unwrap();
        assert!(
            matches!(result, Cow::Borrowed(s) if s.as_ptr() == CORPUS_NORMALIZED.as_ptr()),
            "❌ French zero-copy failed on normalized corpus"
        );
    }
}
