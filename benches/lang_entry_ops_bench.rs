use std::{hint::black_box, time::Duration};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::lang::get_lang_entry_by_code;

fn bench_char_lookups(c: &mut Criterion) {
    let mut group = c.benchmark_group("char_operations");

    let test_cases = vec![
        // ---------------------------
        // Transliterate
        // ---------------------------

        // DEU (6)
        ("DEU", 'ß', "German ß - transliterate miss - 0/6"),
        ("DEU", 'Ä', "German Ä - transliterate first element - 1/6"),
        ("DEU", 'ü', "German ü - transliterate last element - 6/6"),
        // DAN (6)
        ("DAN", 'A', "Danish A - transliterate miss - 0/6"),
        ("DAN", 'Å', "Danish Å - transliterate first element - 1/6"),
        ("DAN", 'ø', "Danish ø - transliterate last element - 6/6"),
        // NOR (6)
        ("NOR", 'B', "Norwegian B - transliterate miss - 0/6"),
        (
            "NOR",
            'Æ',
            "Norwegian Æ - transliterate first element - 1/6",
        ),
        ("NOR", 'å', "Norwegian å - transliterate last element - 6/6"),
        // SWE (6)
        ("SWE", 'C', "Swedish C - transliterate miss - 0/6"),
        ("SWE", 'Å', "Swedish Å - transliterate first element - 1/6"),
        ("SWE", 'ö', "Swedish ö - transliterate last element - 6/6"),
        // ISL (6)
        ("ISL", 'A', "Icelandic A - transliterate miss - 0/6"),
        (
            "ISL",
            'Þ',
            "Icelandic Þ - transliterate first element - 1/6",
        ),
        ("ISL", 'æ', "Icelandic æ - transliterate last element - 6/6"),
        // FRA (6)
        ("FRA", 'B', "French B - transliterate miss - 0/6"),
        ("FRA", 'Œ', "French Œ - transliterate first element - 1/6"),
        ("FRA", 'ç', "French ç - transliterate last element - 6/6"),
        // CAT (2)
        ("CAT", 'A', "Catalan A - transliterate miss - 0/2"),
        ("CAT", 'Ç', "Catalan Ç - transliterate first element - 1/2"),
        ("CAT", 'ç', "Catalan ç - transliterate last element - 2/2"),
        // RUS (66)
        ("RUS", 'A', "Russian A - transliterate miss - 0/66"),
        ("RUS", 'А', "Russian А - transliterate first element - 1/66"),
        ("RUS", 'я', "Russian я - transliterate last element - 66/66"),
        // ---------------------------
        // Precomposed → Base
        // ---------------------------

        // CAT (16)
        ("CAT", 'B', "Catalan B - precomposed miss - 0/16"),
        ("CAT", 'À', "Catalan À - precomposed first element - 1/16"),
        ("CAT", 'ü', "Catalan ü - precomposed last element - 16/16"),
        // CES (18)
        ("CES", 'A', "Czech A - precomposed miss - 0/18"),
        ("CES", 'Č', "Czech Č - precomposed first element - 1/18"),
        ("CES", 'ů', "Czech ů - precomposed last element - 18/18"),
        // FRA (26)
        ("FRA", 'B', "French B - precomposed miss - 0/26"),
        ("FRA", 'À', "French À - precomposed first element - 1/26"),
        ("FRA", 'ÿ', "French ÿ - precomposed last element - 26/26"),
        // HRV (14)
        ("HRV", 'A', "Croatian A - precomposed miss - 0/14"),
        ("HRV", 'Č', "Croatian Č - precomposed first element - 1/14"),
        ("HRV", 'ǌ', "Croatian ǌ - precomposed last element - 14/14"),
        // ITA (12)
        ("ITA", 'B', "Italian B - precomposed miss - 0/12"),
        ("ITA", 'À', "Italian À - precomposed first element - 1/12"),
        ("ITA", 'ù', "Italian ù - precomposed last element - 12/12"),
        // POL (18)
        ("POL", 'A', "Polish A - precomposed miss - 0/18"),
        ("POL", 'Ą', "Polish Ą - precomposed first element - 1/18"),
        ("POL", 'ż', "Polish ż - precomposed last element - 18/18"),
        // POR (26)
        ("POR", 'B', "Portuguese B - precomposed miss - 0/26"),
        (
            "POR",
            'À',
            "Portuguese À - precomposed first element - 1/26",
        ),
        (
            "POR",
            'ü',
            "Portuguese ü - precomposed last element - 26/26",
        ),
        // SLK (20)
        ("SLK", 'A', "Slovak A - precomposed miss - 0/20"),
        ("SLK", 'Č', "Slovak Č - precomposed first element - 1/20"),
        ("SLK", 'ô', "Slovak ô - precomposed last element - 20/20"),
        // SPA (12)
        ("SPA", 'B', "Spanish B - precomposed miss - 0/12"),
        ("SPA", 'Á', "Spanish Á - precomposed first element - 1/12"),
        ("SPA", 'ü', "Spanish ü - precomposed last element - 12/12"),
        // SRP (14)
        ("SRP", 'A', "Serbian A - precomposed miss - 0/14"),
        ("SRP", 'Ђ', "Serbian Ђ - precomposed first element - 1/14"),
        ("SRP", 'ž', "Serbian ž - precomposed last element - 14/14"),
        // VIE (134)
        ("VIE", 'B', "Vietnamese B - precomposed miss - 0/134"),
        (
            "VIE",
            'À',
            "Vietnamese À - precomposed first element - 1/134",
        ),
        (
            "VIE",
            'đ',
            "Vietnamese đ - precomposed last element - 134/134",
        ),
        // ---------------------------
        // Spacing Diacritics
        // ---------------------------

        // ARA (14)
        ("ARA", 'A', "Arabic A - spacing miss - 0/14"),
        (
            "ARA",
            '\u{064B}',
            "Arabic FATHATAN - spacing first element - 1/14",
        ),
        (
            "ARA",
            '\u{0670}',
            "Arabic SUPERSCRIPT ALEF - spacing last element - 14/14",
        ),
        // BEN (5)
        ("BEN", 'A', "Bengali A - spacing miss - 0/5"),
        (
            "BEN",
            '\u{09BC}',
            "Bengali Nukta - spacing first element - 1/5",
        ),
        (
            "BEN",
            '\u{09CD}',
            "Bengali Virama - spacing last element - 5/5",
        ),
        // ELL (6)
        ("ELL", 'A', "Greek A - spacing miss - 0/6"),
        (
            "ELL",
            '\u{0301}',
            "Greek Oxia - spacing first element - 1/6",
        ),
        (
            "ELL",
            '\u{0345}',
            "Greek Ypogegrammeni - spacing last element - 6/6",
        ),
        // HEB (20)
        ("HEB", 'A', "Hebrew A - spacing miss - 0/20"),
        (
            "HEB",
            '\u{05B0}',
            "Hebrew Sheva - spacing first element - 1/20",
        ),
        (
            "HEB",
            '\u{05C7}',
            "Hebrew Qamats Qatan - spacing last element - 20/20",
        ),
        // HIN (5)
        ("HIN", 'A', "Hindi A - spacing miss - 0/5"),
        (
            "HIN",
            '\u{093C}',
            "Hindi Nukta - spacing first element - 1/5",
        ),
        (
            "HIN",
            '\u{094D}',
            "Hindi Virama - spacing last element - 5/5",
        ),
        // TAM (1)
        ("TAM", 'A', "Tamil A - spacing miss - 0/1"),
        (
            "TAM",
            '\u{0BCD}',
            "Tamil Pulli - spacing first element - 1/1",
        ),
        (
            "TAM",
            '\u{0BCD}',
            "Tamil Pulli - spacing last element - 1/1",
        ),
        // THA (16)
        ("THA", 'A', "Thai A - spacing miss - 0/16"),
        (
            "THA",
            '\u{0E31}',
            "Thai MAI HAN-AKAT - spacing first element - 1/16",
        ),
        (
            "THA",
            '\u{0E4E}',
            "Thai YAMAKKAN - spacing last element - 16/16",
        ),
        // LAO (15)
        ("LAO", 'A', "Lao A - spacing miss - 0/15"),
        (
            "LAO",
            '\u{0EB1}',
            "Lao MAI KAN - spacing first element - 1/15",
        ),
        (
            "LAO",
            '\u{0ECD}',
            "Lao NIGGAHITA - spacing last element - 15/15",
        ),
        // MYA (17)
        ("MYA", 'A', "Myanmar A - spacing miss - 0/17"),
        (
            "MYA",
            '\u{102B}',
            "Myanmar TALL AA - spacing first element - 1/17",
        ),
        (
            "MYA",
            '\u{103E}',
            "Myanmar MEDIAL HA - spacing last element - 17/17",
        ),
        // KHM (31)
        ("KHM", 'A', "Khmer A - spacing miss - 0/31"),
        ("KHM", '\u{17B6}', "Khmer AA - spacing first element - 1/31"),
        (
            "KHM",
            '\u{17DD}',
            "Khmer ATTHACAN - spacing last element - 31/31",
        ),
        // VIE (5)
        ("VIE", 'A', "Vietnamese A - spacing miss - 0/5"),
        (
            "VIE",
            '\u{0300}',
            "Vietnamese Grave - spacing first element - 1/5",
        ),
        (
            "VIE",
            '\u{0323}',
            "Vietnamese Dot Below - spacing last element - 5/5",
        ),
    ];

    for (lang, ch, desc) in test_cases {
        let entry = get_lang_entry_by_code(lang).unwrap();

        group.bench_with_input(
            BenchmarkId::new("is_diacritic", desc),
            &(entry, ch),
            |b, (entry, ch)| {
                b.iter(|| black_box(entry.spacing_diacritics().contains(black_box(ch))));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("is_diacritic_via_any", desc),
            &(entry, ch),
            |b, (entry, ch)| {
                b.iter(|| {
                    black_box(
                        entry
                            .spacing_diacritics()
                            .iter()
                            .any(|y| *y == black_box(*ch)),
                    )
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("is_transliterable", desc),
            &(entry, ch),
            |b, (entry, ch)| {
                b.iter(|| black_box(entry.transliterate_char_slice().contains(black_box(ch))));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("is_transliterable_via_any", desc),
            &(entry, ch),
            |b, (entry, ch)| {
                b.iter(|| {
                    black_box(
                        entry
                            .transliterate_char_slice()
                            .iter()
                            .any(|y| *y == black_box(*ch)),
                    )
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("is_pre_composed_to_base_char", desc),
            &(entry, ch),
            |b, (entry, ch)| {
                b.iter(|| black_box(entry.is_pre_composed_to_base_char(black_box(*ch))));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("is_pre_composed_to_base_char_via_any", desc),
            &(entry, ch),
            |b, (entry, ch)| {
                b.iter(|| {
                    black_box(
                        entry
                            .pre_composed_to_base_char_slice()
                            .iter()
                            .any(|y| *y == black_box(*ch)),
                    )
                });
            },
        );
    }

    group.finish();
}

fn bench_transliterate_lookups(c: &mut Criterion) {
    let mut group = c.benchmark_group("char_transliterate");

    let test_cases = vec![
        // DEU (6)
        ("DEU", 'ß', "German ß - miss - 0/6"),
        ("DEU", 'Ä', "German Ä - first - 1/6"),
        ("DEU", 'ü', "German ü - last - 6/6"),
        // DAN (6)
        ("DAN", 'A', "Danish A - miss - 0/6"),
        ("DAN", 'Å', "Danish Å - first - 1/6"),
        ("DAN", 'ø', "Danish ø - last - 6/6"),
        // NOR (6)
        ("NOR", 'B', "Norwegian B - miss - 0/6"),
        ("NOR", 'Æ', "Norwegian Æ - first - 1/6"),
        ("NOR", 'å', "Norwegian å - last - 6/6"),
        // SWE (6)
        ("SWE", 'C', "Swedish C - miss - 0/6"),
        ("SWE", 'Å', "Swedish Å - first - 1/6"),
        ("SWE", 'ö', "Swedish ö - last - 6/6"),
        // ISL (6)
        ("ISL", 'A', "Icelandic A - miss - 0/6"),
        ("ISL", 'Þ', "Icelandic Þ - first - 1/6"),
        ("ISL", 'æ', "Icelandic æ - last - 6/6"),
        // FRA (6)
        ("FRA", 'B', "French B - miss - 0/6"),
        ("FRA", 'Œ', "French Œ - first - 1/6"),
        ("FRA", 'ç', "French ç - last - 6/6"),
        // CAT (2)
        ("CAT", 'A', "Catalan A - miss - 0/2"),
        ("CAT", 'Ç', "Catalan Ç - first - 1/2"),
        ("CAT", 'ç', "Catalan ç - last - 2/2"),
        // RUS (66)
        ("RUS", 'A', "Russian A - miss - 0/66"),
        ("RUS", 'А', "Russian А - first - 1/66"),
        ("RUS", 'я', "Russian я - last - 66/66"),
    ];

    for (lang, ch, desc) in test_cases {
        let entry = get_lang_entry_by_code(lang).unwrap();

        group.bench_with_input(BenchmarkId::new("is_transliterable", desc), &ch, |b, ch| {
            b.iter(|| black_box(entry.transliterate_char_slice().contains(black_box(ch))));
        });

        group.bench_with_input(BenchmarkId::new("via_any", desc), &ch, |b, ch| {
            b.iter(|| {
                black_box(
                    entry
                        .transliterate_char_slice()
                        .iter()
                        .any(|y| *y == black_box(*ch)),
                )
            });
        });
    }

    group.finish();
}

fn bench_precomposed_to_base_lookups(c: &mut Criterion) {
    let mut group = c.benchmark_group("char_precomposed");

    let test_cases = vec![
        ("CAT", 'B', "CAT miss - 0/16"),
        ("CAT", 'À', "CAT first - 1/16"),
        ("CAT", 'ü', "CAT last - 16/16"),
        ("CES", 'A', "CES miss - 0/18"),
        ("CES", 'Č', "CES first - 1/18"),
        ("CES", 'ů', "CES last - 18/18"),
        ("FRA", 'B', "FRA miss - 0/26"),
        ("FRA", 'À', "FRA first - 1/26"),
        ("FRA", 'ÿ', "FRA last - 26/26"),
        ("HRV", 'A', "HRV miss - 0/14"),
        ("HRV", 'Č', "HRV first - 1/14"),
        ("HRV", 'ǌ', "HRV last - 14/14"),
        ("ITA", 'B', "ITA miss - 0/12"),
        ("ITA", 'À', "ITA first - 1/12"),
        ("ITA", 'ù', "ITA last - 12/12"),
        ("POL", 'A', "POL miss - 0/18"),
        ("POL", 'Ą', "POL first - 1/18"),
        ("POL", 'ż', "POL last - 18/18"),
        ("POR", 'B', "POR miss - 0/26"),
        ("POR", 'À', "POR first - 1/26"),
        ("POR", 'ü', "POR last - 26/26"),
        ("SLK", 'A', "SLK miss - 0/20"),
        ("SLK", 'Č', "SLK first - 1/20"),
        ("SLK", 'ô', "SLK last - 20/20"),
        ("SPA", 'B', "SPA miss - 0/12"),
        ("SPA", 'Á', "SPA first - 1/12"),
        ("SPA", 'ü', "SPA last - 12/12"),
        ("SRP", 'A', "SRP miss - 0/14"),
        ("SRP", 'Ђ', "SRP first - 1/14"),
        ("SRP", 'ž', "SRP last - 14/14"),
        ("VIE", 'B', "VIE miss - 0/134"),
        ("VIE", 'À', "VIE first - 1/134"),
        ("VIE", 'đ', "VIE last - 134/134"),
    ];

    for (lang, ch, desc) in test_cases {
        let entry = get_lang_entry_by_code(lang).unwrap();

        group.bench_with_input(BenchmarkId::new("is_precomposed", desc), &ch, |b, ch| {
            b.iter(|| {
                black_box(
                    entry
                        .pre_composed_to_base_char_slice()
                        .contains(black_box(ch)),
                )
            });
        });

        group.bench_with_input(BenchmarkId::new("via_any", desc), &ch, |b, ch| {
            b.iter(|| {
                black_box(
                    entry
                        .pre_composed_to_base_char_slice()
                        .iter()
                        .any(|y| *y == black_box(*ch)),
                )
            });
        });
    }

    group.finish();
}

fn bench_spacing_diacritic_lookups(c: &mut Criterion) {
    let mut group = c.benchmark_group("char_spacing_diacritic");

    let test_cases = vec![
        ("ARA", 'A', "ARA miss - 0/14"),
        ("ARA", '\u{064B}', "ARA first - 1/14"),
        ("ARA", '\u{0670}', "ARA last - 14/14"),
        ("BEN", 'A', "BEN miss - 0/5"),
        ("BEN", '\u{09BC}', "BEN first - 1/5"),
        ("BEN", '\u{09CD}', "BEN last - 5/5"),
        ("ELL", 'A', "ELL miss - 0/6"),
        ("ELL", '\u{0301}', "ELL first - 1/6"),
        ("ELL", '\u{0345}', "ELL last - 6/6"),
        ("HEB", 'A', "HEB miss - 0/20"),
        ("HEB", '\u{05B0}', "HEB first - 1/20"),
        ("HEB", '\u{05C7}', "HEB last - 20/20"),
        ("HIN", 'A', "HIN miss - 0/5"),
        ("HIN", '\u{093C}', "HIN first - 1/5"),
        ("HIN", '\u{094D}', "HIN last - 5/5"),
        ("TAM", 'A', "TAM miss - 0/1"),
        ("TAM", '\u{0BCD}', "TAM only - 1/1"),
        ("THA", 'A', "THA miss - 0/16"),
        ("THA", '\u{0E31}', "THA first - 1/16"),
        ("THA", '\u{0E4E}', "THA last - 16/16"),
        ("LAO", 'A', "LAO miss - 0/15"),
        ("LAO", '\u{0EB1}', "LAO first - 1/15"),
        ("LAO", '\u{0ECD}', "LAO last - 15/15"),
        ("MYA", 'A', "MYA miss - 0/17"),
        ("MYA", '\u{102B}', "MYA first - 1/17"),
        ("MYA", '\u{103E}', "MYA last - 17/17"),
        ("KHM", 'A', "KHM miss - 0/31"),
        ("KHM", '\u{17B6}', "KHM first - 1/31"),
        ("KHM", '\u{17DD}', "KHM last - 31/31"),
        ("VIE", 'A', "VIE miss - 0/5"),
        ("VIE", '\u{0300}', "VIE first - 1/5"),
        ("VIE", '\u{0323}', "VIE last - 5/5"),
    ];

    for (lang, ch, desc) in test_cases {
        let entry = get_lang_entry_by_code(lang).unwrap();

        group.bench_with_input(BenchmarkId::new("is_spacing", desc), &ch, |b, ch| {
            b.iter(|| black_box(entry.spacing_diacritics().contains(black_box(ch))));
        });

        group.bench_with_input(BenchmarkId::new("via_any", desc), &ch, |b, ch| {
            b.iter(|| {
                black_box(
                    entry
                        .spacing_diacritics()
                        .iter()
                        .any(|y| *y == black_box(*ch)),
                )
            });
        });
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
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(2))
        .warm_up_time(Duration::from_secs(2))
        .sample_size(500)
        .noise_threshold(0.015)
        .significance_level(0.05);
    targets = bench_spacing_diacritic_lookups, bench_precomposed_to_base_lookups, bench_transliterate_lookups, bench_text_operations, bench_char_lookups, bench_hot_loop_simulation
);
criterion_main!(benches);
