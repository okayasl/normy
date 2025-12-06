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
    ("IÌ Í Ĩ IĮ ĖĖ ŲŲ – Lithuanian edge cases", normy::LIT),
];

// ---------------------------------------------------------------------------
// Two versions of LangEntry – we compile both into the same binary
// ---------------------------------------------------------------------------
mod current {
    use normy::lang::LangEntry;

    #[inline(always)]
    pub fn apply_case_fold(entry: &LangEntry, c: char) -> Option<char> {
        // if !self.has_case_map && !self.has_fold_map {
        //     return c.to_lowercase().next();
        // }
        if let Some(m) = entry.fold_map().iter().find(|m| m.from == c) {
            if entry.has_one_to_one_folds() {
                Some(m.to.chars().next().unwrap_or(c)) // Safe: we know it's 1 char
            } else {
                None
            }
        } else if let Some(m) = entry.case_map().iter().find(|m| m.from == c) {
            Some(m.to)
        } else {
            c.to_lowercase().next()
        }
    }
}

mod new {
    use normy::lang::LangEntry;

    #[inline(always)]
    pub fn apply_case_fold(entry: &LangEntry, c: char) -> Option<char> {
        if !entry.has_case_map() && !entry.has_fold_map() {
            return c.to_lowercase().next();
        }
        if let Some(m) = entry.fold_map().iter().find(|m| m.from == c) {
            if entry.has_one_to_one_folds() {
                Some(m.to.chars().next().unwrap_or(c)) // Safe: we know it's 1 char
            } else {
                None
            }
        } else if let Some(m) = entry.case_map().iter().find(|m| m.from == c) {
            Some(m.to)
        } else {
            c.to_lowercase().next()
        }
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
        // group.bench_function(
        //     BenchmarkId::new("needs_case_fold/current", lang.code()),
        //     |b| {
        //         b.iter_batched(
        //             || chars.clone(),
        //             |data| {
        //                 for &c in &data {
        //                     black_box(current::needs_case_fold(entry, c));
        //                 }
        //             },
        //             BatchSize::SmallInput,
        //         )
        //     },
        // );

        // group.bench_function(
        //     BenchmarkId::new("needs_case_fold/new", lang.code()),
        //     |b| {
        //         b.iter_batched(
        //             || chars.clone(),
        //             |data| {
        //                 for &c in &data {
        //                     black_box(new::needs_case_fold(entry, c));
        //                 }
        //             },
        //             BatchSize::SmallInput,
        //         )
        //     },
        // );

        group.bench_function(
            BenchmarkId::new("apply_case_fold/current", lang.code()),
            |b| {
                b.iter_batched(
                    || chars.clone(),
                    |data| {
                        for &c in &data {
                            black_box(current::apply_case_fold(entry, c));
                        }
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        group.bench_function(BenchmarkId::new("apply_case_fold/new", lang.code()), |b| {
            b.iter_batched(
                || chars.clone(),
                |data| {
                    for &c in &data {
                        black_box(new::apply_case_fold(entry, c));
                    }
                },
                BatchSize::SmallInput,
            )
        });
    }

    group.finish();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(std::time::Duration::from_secs(1))
        .warm_up_time(std::time::Duration::from_secs(1))
        .sample_size(200)
        .noise_threshold(0.02);
    targets = bench_lang_entry
);
criterion_main!(benches);
