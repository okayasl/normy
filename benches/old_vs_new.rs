use std::hint::black_box;

// benches/old_vs_new.rs
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::lang::get_lang_entry_by_code;

fn bench_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("old_vs_new");

    let test_cases = vec![
        ("TUR", 'I', "Turkish I"),
        ("TUR", 'İ', "Turkish İ"),
        ("DEU", 'ß', "German ß"),
        ("ARA", '\u{064B}', "Arabic diacritic"),
        ("VIE", 'Á', "Vietnamese Á"),
        ("ENG", 'A', "ASCII A"),
    ];

    for (lang, ch, desc) in &test_cases {
        let entry = get_lang_entry_by_code(lang).unwrap();

        // Compare apply_case_fold
        group.bench_with_input(
            BenchmarkId::new("apply_case_fold_NEW", desc),
            &(entry, ch),
            |b, (entry, ch)| {
                b.iter(|| black_box(entry.apply_case_fold(black_box(**ch))));
            },
        );

        // Compare is_diacritic
        group.bench_with_input(
            BenchmarkId::new("is_diacritic_NEW", desc),
            &(entry, ch),
            |b, (entry, ch)| {
                b.iter(|| black_box(entry.is_diacritic(black_box(**ch))));
            },
        );

        // Compare needs_lowercase
        group.bench_with_input(
            BenchmarkId::new("needs_lowercase_NEW", desc),
            &(entry, ch),
            |b, (entry, ch)| {
                b.iter(|| black_box(entry.needs_lowercase(black_box(**ch))));
            },
        );
    }

    // Text-based operations
    let text_cases = vec![
        ("TUR", "İSTANBUL", "Turkish uppercase"),
        ("DEU", "GROẞE STRAẞE", "German with ß"),
        ("ARA", "مَرْحَبًا", "Arabic with diacritics"),
    ];

    for (lang, text, desc) in &text_cases {
        let entry = get_lang_entry_by_code(lang).unwrap();

        group.bench_with_input(
            BenchmarkId::new("hint_capacity_fold_NEW", desc),
            &(entry, text),
            |b, (entry, text)| {
                b.iter(|| black_box(entry.hint_capacity_fold(black_box(*text))));
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_comparison);
criterion_main!(benches);
