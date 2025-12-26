use std::{hint::black_box, time::Duration};

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{
    ARA, CaseFold, DEU, ENG, FRA, HIN, JPN, KOR, LIT, LowerCase, NLD, RUS, RemoveDiacritics,
    SegmentWords, TUR, Transliterate, VIE, ZHO,
    context::Context,
    lang::Lang,
    stage::{Stage, StaticFusableStage},
};

// 16 languages — the exact set that will appear in the Normy white paper
const SAMPLES: &[(&str, Lang)] = &[
    //  1. Turkish  – dotted/dotless I + aggressive case rules
    ("İSTANBUL İĞNE İĞDE", TUR),
    //  2. German   – sharp-s + Eszett
    ("GRÜNE STRAßE", DEU),
    //  3. French   – œ/Œ ligatures + heavy accents
    ("SŒUR NAÏVE À L’ŒUF", FRA),
    //  4. Arabic   – lam-alef, shadda, harakat, tatweel
    ("ٱلْكِتَابُ مُحَمَّدٌ ـــــ", ARA),
    //  5. Vietnamese – stacked diacritics (worst-case NFD explosion)
    ("Việt Nam Phỏ̉", VIE),
    //  6. Hindi    – nukta, ZWNJ/ZWJ, conjuncts
    ("हिन्दी ज़िंदगी", HIN),
    //  7. Japanese – half-width kana + prolonged sound mark
    ("ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟ ーー", JPN),
    //  8. Chinese  – full-width ASCII + full-width punctuation
    ("ＨＴＭＬ　＜ｔａｇ＞　１２３", ZHO),
    //  9. Korean   – jamo + full-width Latin
    ("한글 ＫＯＲＥＡ", KOR),
    // 10. Greek    – final sigma + dialytika + tonos
    // ("ἈΡΧΙΜΉΔΗΣ ἙΛΛΆΣ", ELL),
    // 11. Russian  – Ё/ё + combining accents
    ("ЁЛКИ-ПАЛКИ А́ННА", RUS),
    // // 12. Thai     – no spaces, tone marks, saraswati
    // ("ภาษาไทย ๓๔๕", THA),
    // // 13. Hebrew   – niqqud + final forms
    // ("ספר עִבְרִית", HEB),
    // // 14. Spanish  – ñ + inverted punctuation
    // ("¡España mañana!", SPA),
    // // 15. Polish   – Polish ogonek + kreska
    // ("Łódź Żółć", POL),
    // 16. Dutch  – HTML + emoji + punctuation + control chars
    ("<b>IJssEL und Ĳssel</b>\t\r\n", NLD),
    // 17. English  – HTML + emoji + punctuation + control chars
    ("<b>Hello naïve World!</b>\t\r\n  résumé 🇫🇷", ENG),
    ("IÌ Í Ĩ IĮ ĖĖ ŲŲ – Lithuanian edge cases", LIT),
];

fn collection_methods_benches_auto<S, C>(c: &mut Criterion, stage_name: &str, constructor: C)
where
    S: Stage + StaticFusableStage + 'static,
    C: Fn() -> S + Copy,
{
    let mut group = c.benchmark_group(format!("{stage_name}_collection_methods"));

    for &(text, lang) in SAMPLES {
        let stage = constructor();
        let ctx = Context::new(lang);

        if !stage.needs_apply(text, &ctx).unwrap() {
            continue; // Skip unchanged; collection only happens on changed paths
        }

        if stage.supports_static_fusion() {
            // Bench extend (option 1)
            group.bench_function(
                BenchmarkId::new("extend_changed_static", format!("{}-{}", lang.code(), text)),
                |b| {
                    b.iter_batched(
                        constructor,
                        |stage| {
                            let iter = stage.static_fused_adapter(text.chars(), &ctx);
                            let mut out = String::with_capacity(text.len());
                            out.extend(iter);
                            black_box(out)
                        },
                        BatchSize::SmallInput,
                    )
                },
            );

            // Bench collect (option 2)
            group.bench_function(
                BenchmarkId::new(
                    "collect_changed_static",
                    format!("{}-{}", lang.code(), text),
                ),
                |b| {
                    b.iter_batched(
                        constructor,
                        |stage| {
                            let iter = stage.static_fused_adapter(text.chars(), &ctx);
                            let out: String = iter.collect();
                            black_box(out)
                        },
                        BatchSize::SmallInput,
                    )
                },
            );

            // Bench loop with push (option 3)
            group.bench_function(
                BenchmarkId::new("loop_changed_static", format!("{}-{}", lang.code(), text)),
                |b| {
                    b.iter_batched(
                        constructor,
                        |stage| {
                            let iter = stage.static_fused_adapter(text.chars(), &ctx);
                            let mut out = String::with_capacity(text.len());
                            for c in iter {
                                out.push(c);
                            }
                            black_box(out)
                        },
                        BatchSize::SmallInput,
                    )
                },
            );
        }

        if let Some(dynamic_fused_stage) = stage.as_fusable() {
            // Similar benches for dynamic iter
            // Bench extend dynamic
            group.bench_function(
                BenchmarkId::new(
                    "extend_changed_dynamic",
                    format!("{}-{}", lang.code(), text),
                ),
                |b| {
                    b.iter_batched(
                        constructor,
                        |_| {
                            let iter =
                                dynamic_fused_stage.dyn_fused_adapter(Box::new(text.chars()), &ctx);
                            let mut out = String::with_capacity(text.len());
                            out.extend(iter);
                            black_box(out)
                        },
                        BatchSize::SmallInput,
                    )
                },
            );

            // Bench collect dynamic
            group.bench_function(
                BenchmarkId::new(
                    "collect_changed_dynamic",
                    format!("{}-{}", lang.code(), text),
                ),
                |b| {
                    b.iter_batched(
                        constructor,
                        |_| {
                            let iter =
                                dynamic_fused_stage.dyn_fused_adapter(Box::new(text.chars()), &ctx);
                            let out: String = iter.collect();
                            black_box(out)
                        },
                        BatchSize::SmallInput,
                    )
                },
            );

            // Bench loop dynamic
            group.bench_function(
                BenchmarkId::new("loop_changed_dynamic", format!("{}-{}", lang.code(), text)),
                |b| {
                    b.iter_batched(
                        constructor,
                        |_| {
                            let iter =
                                dynamic_fused_stage.dyn_fused_adapter(Box::new(text.chars()), &ctx);
                            let mut out = String::with_capacity(text.len());
                            for c in iter {
                                out.push(c);
                            }
                            black_box(out)
                        },
                        BatchSize::SmallInput,
                    )
                },
            );
        }
    }

    group.finish();
}

macro_rules! bench_processes {
    // This defines the macro syntax: takes a list of identifiers (the stages)
    ($c:expr, [ $( $stage:ident ),* ]) => {
        // The macro repeats the following code block for every identifier ($stage)
        $(
            // Convert the identifier to a string literal for the name
            let name = stringify!($stage);

            // Call the bench functions, passing a closure that constructs the stage
            collection_methods_benches_auto($c, name, || $stage);
        )*
    };
}

fn process_matrix(c: &mut Criterion) {
    bench_processes!(
        c,
        [
            LowerCase,
            CaseFold,
            RemoveDiacritics,
            Transliterate,
            SegmentWords
        ]
    );
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(2))
        .warm_up_time(Duration::from_secs(2))
        .sample_size(500)
        .noise_threshold(0.015)
        .significance_level(0.05);
    targets = process_matrix
);
criterion_main!(benches);
