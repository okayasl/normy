// benches/lang_entry_bench.rs
//
// Micro-benchmark that isolates the two hot LangEntry methods
//   • needs_case_fold
//   • needs_lowercase
//
// Compares the current implementation (iter().any()) with the proposed
// slice-lookup version (contains(&c)).
//
// Run with `cargo bench --bench lang_entry_bench`

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{context::Context, lang::Lang};

// ---------------------------------------------------------------------------
// Test corpus – exactly the same 17 samples used in the white paper
// ---------------------------------------------------------------------------
const SAMPLES: &[(&str, Lang)] = &[
    ("İSTANBUL İĞNE İĞDE", normy::TUR),
    ("GRÜNE STRAßE", normy::DEU),
    ("SŒUR NAÏVE À L’ŒUF", normy::FRA),
    ("الْكِتَابُ مُحَمَّدٌ ـــــ", normy::ARA),
    ("Việt Nam Phỏ̉", normy::VIE),
    ("हिन्दी ज़िंदगी", normy::HIN),
    ("ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟ ーー", normy::JPN),
    ("ＨＴＭＬ　＜ｔａｇ＞　１２３", normy::ZHO),
    ("한글 ＫＯＲＥＡ", normy::KOR),
    ("<b>IJssEL und Ĳssel</b>\t\r\n", normy::NLD),
    ("<b>Hello naïve World!</b>\t\r\n résumé", normy::ENG),
    ("ἈΡΧΙΜΉΔΗΣ ἙΛΛΆΣ", normy::ELL),
    ("ЁЛКИ-ПАЛКИ А́ННА", normy::RUS),
    ("ภาษาไทย ๓๔๕", normy::THA),
    ("ספר עִבְרִית", normy::HEB),
    ("¡España mañana!", normy::SPA),
    ("Łódź Żółć", normy::POL),
];

// ---------------------------------------------------------------------------
// Two versions of LangEntry – we compile both into the same binary
// ---------------------------------------------------------------------------
mod current {
    use normy::lang::LangEntry;

    #[inline(always)]
    pub fn needs_case_fold(entry: &LangEntry, c: char) -> bool {
        entry.fold_char_slice().contains(&c)
            || entry.case_map().iter().any(|m| m.from == c)
            || c.to_lowercase().next() != Some(c)
    }

    #[inline(always)]
    pub fn needs_lowercase(entry: &LangEntry, c: char) -> bool {
        entry.case_map().iter().any(|m| m.from == c) || c.to_lowercase().next() != Some(c)
    }
}

mod fast_slice {
    use normy::lang::LangEntry;

    #[inline(always)]
    pub fn needs_case_fold(entry: &LangEntry, c: char) -> bool {
        entry.fold_char_slice().contains(&c)
            || entry.case_char_slice().contains(&c)
            || c.to_lowercase().next() != Some(c)
    }

    #[inline(always)]
    pub fn needs_lowercase(entry: &LangEntry, c: char) -> bool {
        entry.case_char_slice().contains(&c) || c.to_lowercase().next() != Some(c)
    }
}

// ---------------------------------------------------------------------------
// Benchmark harness
// ---------------------------------------------------------------------------
fn bench_lang_entry(c: &mut Criterion) {
    let mut group = c.benchmark_group("LangEntry methods");

    for &(text, lang) in SAMPLES {
        let ctx = Context::new(lang);
        let entry = &ctx.lang_entry;

        // Warm-up + make sure the compiler can’t eliminate the work
        let chars: Vec<char> = text.chars().cycle().take(10_000).collect();

        // ---- needs_case_fold ----
        group.bench_function(
            BenchmarkId::new("needs_case_fold/current", lang.code()),
            |b| {
                b.iter_batched(
                    || chars.clone(),
                    |data| {
                        for &c in &data {
                            black_box(current::needs_case_fold(entry, c));
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.bench_function(
            BenchmarkId::new("needs_case_fold/fast_slice", lang.code()),
            |b| {
                b.iter_batched(
                    || chars.clone(),
                    |data| {
                        for &c in &data {
                            black_box(fast_slice::needs_case_fold(entry, c));
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        // ---- needs_lowercase ----
        group.bench_function(
            BenchmarkId::new("needs_lowercase/current", lang.code()),
            |b| {
                b.iter_batched(
                    || chars.clone(),
                    |data| {
                        for &c in &data {
                            black_box(current::needs_lowercase(entry, c));
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.bench_function(
            BenchmarkId::new("needs_lowercase/fast_slice", lang.code()),
            |b| {
                b.iter_batched(
                    || chars.clone(),
                    |data| {
                        for &c in &data {
                            black_box(fast_slice::needs_lowercase(entry, c));
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(std::time::Duration::from_secs(4))
        .warm_up_time(std::time::Duration::from_secs(2))
        .sample_size(200)
        .noise_threshold(0.02);
    targets = bench_lang_entry
);
criterion_main!(benches);
