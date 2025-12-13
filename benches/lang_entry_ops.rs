use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::lang::get_lang_entry_by_code;

fn bench_char_lookups(c: &mut Criterion) {
    let mut group = c.benchmark_group("char_operations");

    // Test data: chars that need transformation
    let test_cases = vec![
        ("TUR", 'I', "Turkish capital I"),
        ("TUR", 'İ', "Turkish capital İ"),
        ("DEU", 'ß', "German sharp s"),
        ("ARA", '\u{064B}', "Arabic diacritic"),
        ("VIE", 'Á', "Vietnamese accent"),
        ("ENG", 'A', "ASCII (no-op)"),
    ];

    for (lang, ch, desc) in test_cases {
        let entry = get_lang_entry_by_code(lang).unwrap();

        group.bench_with_input(
            BenchmarkId::new("apply_case_fold", desc),
            &(entry, ch),
            |b, (entry, ch)| {
                b.iter(|| black_box(entry.apply_case_fold(black_box(*ch))));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("is_diacritic", desc),
            &(entry, ch),
            |b, (entry, ch)| {
                b.iter(|| black_box(entry.is_spacing_diacritic(black_box(*ch))));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("apply_strip", desc),
            &(entry, ch),
            |b, (entry, ch)| {
                b.iter(|| black_box(entry.find_pre_composed_to_base_map(black_box(*ch))));
            },
        );
    }

    group.finish();
}

fn bench_text_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_operations");

    let test_texts = vec![
        ("TUR", "İSTANBUL", "uppercase Turkish"),
        ("TUR", "istanbul", "lowercase Turkish (no-op)"),
        ("DEU", "GROẞE STRAẞE", "German with ß"),
        ("DEU", "kleine strasse", "lowercase German (no-op)"),
        ("ARA", "مَرْحَبًا", "Arabic with diacritics"),
        ("ARA", "مرحبا", "Arabic without diacritics (no-op)"),
        ("VIE", "TIẾNG VIỆT", "Vietnamese with tones"),
        ("ENG", "HELLO WORLD", "ASCII uppercase"),
        ("ENG", "hello world", "ASCII lowercase (no-op)"),
    ];

    for (lang, text, desc) in test_texts {
        let entry = get_lang_entry_by_code(lang).unwrap();

        // Test needs_lowercase (hot detection path)
        group.bench_with_input(
            BenchmarkId::new("needs_lowercase", desc),
            &(entry, text),
            |b, (entry, text)| {
                b.iter(|| black_box(text.chars().any(|c| entry.needs_lowercase(c))));
            },
        );

        // Test needs_diacritic_removal
        group.bench_with_input(
            BenchmarkId::new("needs_diacritic_removal", desc),
            &(entry, text),
            |b, (entry, text)| {
                b.iter(|| {
                    black_box(
                        entry.needs_pre_composed_to_base_map_or_spacing_diacritics_removal(text),
                    )
                });
            },
        );

        // Test hint_capacity_fold
        group.bench_with_input(
            BenchmarkId::new("hint_capacity_fold", desc),
            &(entry, text),
            |b, (entry, text)| {
                b.iter(|| black_box(entry.hint_capacity_fold(text)));
            },
        );
    }

    group.finish();
}

fn bench_hot_loop_simulation(c: &mut Criterion) {
    let mut group = c.benchmark_group("hot_loop");

    // Simulate the actual hot loop: check every char in a string
    let text = "İstanbul'da Büyük Çarşı'da Ümit'le Öğle Yemeği Yedik";
    let entry = get_lang_entry_by_code("TUR").unwrap();

    group.bench_function("turkish_case_fold_detection", |b| {
        b.iter(|| {
            let mut count = 0;
            for c in text.chars() {
                if entry.needs_case_fold(black_box(c)) {
                    count += 1;
                }
            }
            black_box(count)
        });
    });

    group.bench_function("turkish_case_fold_transform", |b| {
        b.iter(|| {
            let result: String = text
                .chars()
                .filter_map(|c| entry.apply_case_fold(black_box(c)))
                .collect();
            black_box(result)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_char_lookups,
    bench_text_operations,
    bench_hot_loop_simulation
);
criterion_main!(benches);
