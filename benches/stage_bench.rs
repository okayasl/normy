use std::{borrow::Cow, time::Duration};

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{
    ARA, COLLAPSE_WHITESPACE_ONLY, CaseFold, DEU, ELL, ENG, FRA, HEB, HIN, JPN, KOR, LowerCase,
    NFC, NFD, NFKC, NFKD, NLD, NORMALIZE_WHITESPACE_FULL, NormalizePunctuation, Normy, POL, RUS,
    RemoveDiacritics, SPA, SegmentWords, StripControlChars, StripHtml, THA, TRIM_WHITESPACE_ONLY,
    TUR, Transliterate, UnifyWidth, VIE, ZHO, lang::Lang,
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
    // // 11. Russian  – Ё/ё + combining accents
    // ("ЁЛКИ-ПАЛКИ А́ННА", RUS),
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
];

// S is the concrete stage type (e.g. LowerCase, CaseFold, ...)
fn stage_benches_auto<S, C>(c: &mut Criterion, stage_name: &str, constructor: C)
where
    S: normy::stage::Stage + 'static, // ← correct bound
    C: Fn() -> S,
{
    let mut group = c.benchmark_group(stage_name);

    let mut auto_unchanged = Vec::new();

    for &(text, lang) in SAMPLES {
        // Prepare normalized (unchanged) sample outside measurements
        let stage = constructor();
        let normy = Normy::builder().lang(lang).add_stage(stage).build();
        let normalized = normy.normalize(text).unwrap();
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

fn stage_matrix(c: &mut Criterion) {
    //stage_benches_auto(c, "LowerCase", || LowerCase);
    stage_benches_auto(c, "CaseFold", || CaseFold);
    // stage_benches_auto(c, "RemoveDiacritics", || RemoveDiacritics);
    // stage_benches_auto(c, "Transliterate", || Transliterate);
    // stage_benches_auto(c, "SegmentWords", || SegmentWords);
    // stage_benches_auto(c, "UnifyWidth", || UnifyWidth);
    // stage_benches_auto(c, "NFC", || NFC);
    // stage_benches_auto(c, "NFD", || NFD);
    // stage_benches_auto(c, "NFKC", || NFKC);
    // stage_benches_auto(c, "NFKD", || NFKD);
    // stage_benches_auto(c, "NormalizePunctuation", || NormalizePunctuation);
    // stage_benches_auto(c, "StripControlChars", || StripControlChars);
    // stage_benches_auto(c, "StripHtml", || StripHtml);
    // stage_benches_auto(c, "NormalizeWhitespaceFull", || NORMALIZE_WHITESPACE_FULL);
    // stage_benches_auto(c, "CollapseWhitespaceOnly", || COLLAPSE_WHITESPACE_ONLY);
    // stage_benches_auto(c, "TrimWhitespaceOnly", || TRIM_WHITESPACE_ONLY);
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(2))
        .warm_up_time(Duration::from_secs(2))
        .sample_size(1000)
        .noise_threshold(0.015)
        .significance_level(0.05);
    targets = stage_matrix
);
criterion_main!(benches);
