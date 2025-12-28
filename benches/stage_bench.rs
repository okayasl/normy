use std::{borrow::Cow, hint::black_box, time::Duration};

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{
    ARA, CaseFold, DEU, ENG, FRA, HIN, JPN, KOR, LIT, LowerCase, NLD, Normy, RUS, RemoveDiacritics,
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

fn stage_paths_benches_auto<S, C>(c: &mut Criterion, stage_name: &str, constructor: C)
where
    S: Stage + StaticFusableStage + 'static,
    C: Fn() -> S + Copy,
{
    let mut group = c.benchmark_group(format!("{stage_name}_paths"));
    let mut auto_unchanged = Vec::new();

    for &(text, lang) in SAMPLES {
        let stage = constructor();
        let ctx = Context::new(lang);
        let supports_static_fusion = stage.supports_static_fusion();

        let normalized_cow = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        let normalized = normalized_cow.as_ref().to_string();
        auto_unchanged.push((normalized, lang));

        // Bench changed - apply
        group.bench_function(
            BenchmarkId::new("apply_changed", format!("{}-{}", lang.code(), text)),
            |b| {
                b.iter_batched(
                    constructor,
                    |stage| {
                        let ctx = normy::context::Context::new(lang);
                        let cow = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
                        let s = cow.into_owned();
                        black_box(s)
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        if supports_static_fusion {
            group.bench_function(
                BenchmarkId::new("static_fusion_changed", format!("{}-{}", lang.code(), text)),
                |b| {
                    b.iter_batched(
                        constructor,
                        |stage| {
                            let ctx = Context::new(lang);
                            let static_iter = stage.static_fused_adapter(text.chars(), &ctx);
                            let s = static_iter.collect::<String>();
                            black_box(s)
                        },
                        BatchSize::SmallInput,
                    )
                },
            );
        }
    }

    // Unchanged benches
    for (normalized, lang) in auto_unchanged {
        let stage = constructor();
        let ctx = Context::new(lang);
        let supports_static_fusion = stage.supports_static_fusion();

        // apply unchanged
        group.bench_function(
            BenchmarkId::new("apply_unchanged", format!("{}-{}", lang.code(), normalized)),
            |b| {
                b.iter_batched(
                    constructor,
                    |stage| {
                        let cow = stage.apply(Cow::Borrowed(&normalized), &ctx).unwrap();
                        let s = cow.into_owned();
                        black_box(s)
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        if supports_static_fusion {
            group.bench_function(
                BenchmarkId::new(
                    "static_fusion_unchanged",
                    format!("{}-{}", lang.code(), normalized),
                ),
                |b| {
                    b.iter_batched(
                        constructor,
                        |stage| {
                            let static_iter = stage.static_fused_adapter(normalized.chars(), &ctx);
                            let s = static_iter.collect::<String>();
                            black_box(s)
                        },
                        BatchSize::SmallInput,
                    )
                },
            );
        }
    }

    group.finish();
}

fn stage_benches_auto<S, C>(c: &mut Criterion, stage_name: &str, constructor: C)
where
    S: Stage + StaticFusableStage + 'static,
    C: Fn() -> S,
{
    let mut group = c.benchmark_group(stage_name);

    let mut auto_unchanged = Vec::new();

    for &(text, lang) in SAMPLES {
        // Prepare normalized (unchanged) sample outside measurements
        let stage = constructor();
        let normy = Normy::builder().lang(lang).add_stage(stage).build();
        let normalized = normy.normalize(text).unwrap().into_owned();
        auto_unchanged.push((normalized, lang));
        let mut zero_copy_hits = 0usize;
        let mut total = 0usize;

        // Benchmark changed input
        let id = format!("{} - Changed - {text}", lang.code());
        group.bench_function(BenchmarkId::new("", id), |b| {
            b.iter_batched(
                || text,
                |t| {
                    total += 1;
                    // fresh stage every iteration — same behavior as your original pattern
                    let stage = constructor();
                    let normy = Normy::builder().lang(lang).add_stage(stage).build();
                    let result = normy.normalize(t).unwrap();
                    if matches!(result, Cow::Borrowed(s) if s.as_ptr() == t.as_ptr() && s.len() == t.len()) {
                        zero_copy_hits += 1;
                    }
                },
                BatchSize::SmallInput,
            )
        });
        let pct = if total > 0 {
            (zero_copy_hits as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        println!("   ZERO-COPY {zero_copy_hits}/{total} ({pct:.2}%)");
    }

    // Benchmark auto-unchanged samples
    for (normalized, lang) in auto_unchanged {
        let mut zero_copy_hits = 0usize;
        let mut total = 0usize;
        let id = format!("{} - Unchanged (auto) - {normalized}", lang.code());
        group.bench_function(BenchmarkId::new("", id), |b| {
            b.iter_batched(
                || normalized.as_ref(),
                |t| {
                    total += 1;
                    let stage = constructor();
                    let normy = Normy::builder().lang(lang).add_stage(stage).build();
                    let result = normy.normalize(t).unwrap();
                    if matches!(result, Cow::Borrowed(s) if s.as_ptr() == t.as_ptr() && s.len() == t.len()) {
                        zero_copy_hits += 1;
                    }
                },
                BatchSize::SmallInput,
            )
        });
        let pct = if total > 0 {
            (zero_copy_hits as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        println!("   ZERO-COPY {zero_copy_hits}/{total} ({pct:.2}%)");
    }

    group.finish();
}

macro_rules! bench_stages {
    // This defines the macro syntax: takes a list of identifiers (the stages)
    ($c:expr, [ $( $stage:ident ),* ]) => {
        // The macro repeats the following code block for every identifier ($stage)
        $(
            // Convert the identifier to a string literal for the name
            let name = stringify!($stage);

            // Call the bench functions, passing a closure that constructs the stage
            stage_benches_auto($c, name, || $stage);
            stage_paths_benches_auto($c, name, || $stage);
        )*
    };
}

fn stage_matrix(c: &mut Criterion) {
    bench_stages!(
        c,
        [
            // UnifyWidth,
            // NFC,
            // NFD,
            // NFKC,
            // NFKD,
            // NormalizePunctuation,
            // StripControlChars
            // StripHtml,
            // NORMALIZE_WHITESPACE_FULL,
            // COLLAPSE_WHITESPACE,
            // COLLAPSE_WHITESPACE_UNICODE,
            // TRIM_WHITESPACE,
            // TRIM_WHITESPACE_UNICODE
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
    targets = stage_matrix
);
criterion_main!(benches);
