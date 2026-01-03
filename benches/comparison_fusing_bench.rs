use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{
    ARA, COLLAPSE_WHITESPACE_UNICODE, CaseFold, DEU, FRA, JPN, NormalizePunctuation, POL, RUS,
    RemoveDiacritics, StripControlChars, TRIM_WHITESPACE, Transliterate, UnifyWidth, VIE,
};
use std::{hint::black_box, time::Duration};

// Test samples where EVERY stage transforms the text

// German: ß folding + umlaut transliteration + diacritic removal
const GERMAN_TEXT: &str = "  GRÜßE SCHÖNE ÄPFEL   ";

// Vietnamese: Case fold + heavy diacritics
const VIETNAMESE_TEXT: &str = "TIẾNG VIỆT HÀ NỘI PHỞ";

// Polish: Case fold + Polish diacritics
const POLISH_TEXT: &str = "ŁÓDŹ KRAKÓW GDAŃSK";

// Russian: Case fold + Cyrillic transliteration
const RUSSIAN_TEXT: &str = "МОСКВА РОССИЯ САНКТ-ПЕТЕРБУРГ";

// Japanese: Fullwidth + halfwidth katakana + punctuation
const JAPANESE_TEXT: &str = "ＨＥＬＬＯ　ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟ－－－";

// Arabic: Text with control chars and diacritics
const ARABIC_TEXT: &str = "اَلْعَرَبِيَّةُ\u{200B}\u{200C}اللغة";

// French: Case + ligatures + accents
const FRENCH_TEXT: &str = "ŒUVRE FRANÇAIS CAFÉ---ÉLÈVE";

fn fusion_real_work_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("fusion_real_work");

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║       FUSION: ALL STAGES DO WORK (NON-REDUNDANT PIPELINES)          ║");
    println!("║                                                                      ║");
    println!("║  Testing realistic pipelines where each stage adds value            ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");

    // ═══════════════════════════════════════════════════════════════════════
    // 2-STAGE: CaseFold + RemoveDiacritics
    // ═══════════════════════════════════════════════════════════════════════
    {
        let name = "vietnamese_2stage_fold_strip";
        let text = VIETNAMESE_TEXT;

        let pipeline = normy::Normy::builder()
            .lang(VIE)
            .add_stage(CaseFold) // TIẾNG -> tiếng (includes lowercase)
            .add_stage(RemoveDiacritics) // tiếng -> tieng (strip 80+ diacritics)
            .build();

        let fusion_enabled = pipeline.uses_fusion();
        let result = pipeline.normalize(text).unwrap();

        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "🇻🇳 VIETNAMESE | {} | Fusion: {}",
            name,
            if fusion_enabled { "✅ YES" } else { "❌ NO" }
        );
        println!("   Input:  \"{}\"", text);
        println!("   Output: \"{}\"", result);
        println!("   Transform: CaseFold + strip 80+ diacritics");

        group.bench_with_input(BenchmarkId::new("normalize", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize(black_box(text)).unwrap()));
        });

        group.bench_with_input(BenchmarkId::new("no_fusion", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize_no_fusion(black_box(text)).unwrap()));
        });
        println!();
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 2-STAGE: CaseFold + Transliterate
    // ═══════════════════════════════════════════════════════════════════════
    {
        let name = "russian_2stage_fold_translit";
        let text = RUSSIAN_TEXT;

        let pipeline = normy::Normy::builder()
            .lang(RUS)
            .add_stage(CaseFold) // МОСКВА -> москва
            .add_stage(Transliterate) // москва -> moskva (Cyrillic->Latin)
            .build();

        let fusion_enabled = pipeline.uses_fusion();
        let result = pipeline.normalize(text).unwrap();

        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "🇷🇺 RUSSIAN | {} | Fusion: {}",
            name,
            if fusion_enabled { "✅ YES" } else { "❌ NO" }
        );
        println!("   Input:  \"{}\"", text);
        println!("   Output: \"{}\"", result);
        println!("   Transform: Lowercase + Cyrillic->Latin");

        group.bench_with_input(BenchmarkId::new("normalize", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize(black_box(text)).unwrap()));
        });

        group.bench_with_input(BenchmarkId::new("no_fusion", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize_no_fusion(black_box(text)).unwrap()));
        });
        println!();
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 3-STAGE: CaseFold + Transliterate + Trim Whitespace
    // ═══════════════════════════════════════════════════════════════════════
    {
        let name = "german_3stage_fold_translit_trim";
        let text = GERMAN_TEXT;

        let pipeline = normy::Normy::builder()
            .lang(DEU)
            .add_stage(CaseFold) // GRÜßE -> grüße (ß->ss)
            .add_stage(Transliterate) // ü->ue, ö->oe, ä->ae
            .add_stage(TRIM_WHITESPACE) // Trim whitespace
            .build();

        let fusion_enabled = pipeline.uses_fusion();
        let result = pipeline.normalize(text).unwrap();

        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "🇩🇪 GERMAN | {} | Fusion: {}",
            name,
            if fusion_enabled { "✅ YES" } else { "❌ NO" }
        );
        println!("   Input:  \"{}\"", text);
        println!("   Output: \"{}\"", result);
        println!("   Transform: ß->ss + ü->ue + trim");

        group.bench_with_input(BenchmarkId::new("normalize", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize(black_box(text)).unwrap()));
        });

        group.bench_with_input(BenchmarkId::new("no_fusion", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize_no_fusion(black_box(text)).unwrap()));
        });
        println!();
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 3-STAGE: Polish heavy transformation
    // ═══════════════════════════════════════════════════════════════════════
    {
        let name = "polish_3stage_fold_translit_strip";
        let text = POLISH_TEXT;

        let pipeline = normy::Normy::builder()
            .lang(POL)
            .add_stage(CaseFold) // ŁÓDŹ -> łódź
            .add_stage(Transliterate) // If any transliteration rules exist
            .add_stage(RemoveDiacritics) // ł->l, ó->o, ź->z
            .build();

        let fusion_enabled = pipeline.uses_fusion();
        let result = pipeline.normalize(text).unwrap();

        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "🇵🇱 POLISH | {} | Fusion: {}",
            name,
            if fusion_enabled { "✅ YES" } else { "❌ NO" }
        );
        println!("   Input:  \"{}\"", text);
        println!("   Output: \"{}\"", result);
        println!("   Transform: Lowercase + strip Polish characters");

        group.bench_with_input(BenchmarkId::new("normalize", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize(black_box(text)).unwrap()));
        });

        group.bench_with_input(BenchmarkId::new("no_fusion", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize_no_fusion(black_box(text)).unwrap()));
        });
        println!();
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 3-STAGE: Japanese width/punctuation normalization
    // ═══════════════════════════════════════════════════════════════════════
    {
        let name = "japanese_3stage_width_punct_ws";
        let text = JAPANESE_TEXT;

        let pipeline = normy::Normy::builder()
            .lang(JPN)
            .add_stage(UnifyWidth) // Fullwidth->halfwidth, ﾊﾟ->パ
            .add_stage(NormalizePunctuation) // ----> -
            .add_stage(COLLAPSE_WHITESPACE_UNICODE) // 　-> space
            .build();

        let fusion_enabled = pipeline.uses_fusion();
        let result = pipeline.normalize(text).unwrap();

        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "🇯🇵 JAPANESE | {} | Fusion: {}",
            name,
            if fusion_enabled { "✅ YES" } else { "❌ NO" }
        );
        println!("   Input:  \"{}\"", text);
        println!("   Output: \"{}\"", result);
        println!("   Transform: Width + punctuation + whitespace");

        group.bench_with_input(BenchmarkId::new("normalize", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize(black_box(text)).unwrap()));
        });

        group.bench_with_input(BenchmarkId::new("no_fusion", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize_no_fusion(black_box(text)).unwrap()));
        });
        println!();
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 2-STAGE: Arabic diacritics + control chars
    // ═══════════════════════════════════════════════════════════════════════
    {
        let name = "arabic_2stage_strip_ctrl";
        let text = ARABIC_TEXT;

        let pipeline = normy::Normy::builder()
            .lang(ARA)
            .add_stage(RemoveDiacritics) // Strip tashkeel
            .add_stage(StripControlChars) // Remove zero-width chars
            .build();

        let fusion_enabled = pipeline.uses_fusion();
        let result = pipeline.normalize(text).unwrap();

        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "🇸🇦 ARABIC | {} | Fusion: {}",
            name,
            if fusion_enabled { "✅ YES" } else { "❌ NO" }
        );
        println!("   Input:  \"{}\" (with diacritics+ZWSP)", text);
        println!("   Output: \"{}\"", result);
        println!("   Transform: Strip tashkeel + control chars");

        group.bench_with_input(BenchmarkId::new("normalize", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize(black_box(text)).unwrap()));
        });

        group.bench_with_input(BenchmarkId::new("no_fusion", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize_no_fusion(black_box(text)).unwrap()));
        });
        println!();
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 4-STAGE: French comprehensive normalization
    // ═══════════════════════════════════════════════════════════════════════
    {
        let name = "french_4stage_fold_translit_punct_strip";
        let text = FRENCH_TEXT;

        let pipeline = normy::Normy::builder()
            .lang(FRA)
            .add_stage(CaseFold) // ŒUVRE -> œuvre
            .add_stage(Transliterate) // œ->oe, ç->c
            .add_stage(NormalizePunctuation) // --- -> -
            .add_stage(RemoveDiacritics) // é->e, à->a
            .build();

        let fusion_enabled = pipeline.uses_fusion();
        let result = pipeline.normalize(text).unwrap();

        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "🇫🇷 FRENCH | {} | Fusion: {}",
            name,
            if fusion_enabled { "✅ YES" } else { "❌ NO" }
        );
        println!("   Input:  \"{}\"", text);
        println!("   Output: \"{}\"", result);
        println!("   Transform: 4-stage heavy normalization");

        group.bench_with_input(BenchmarkId::new("normalize", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize(black_box(text)).unwrap()));
        });

        group.bench_with_input(BenchmarkId::new("no_fusion", name), &text, |b, &text| {
            b.iter(|| black_box(pipeline.normalize_no_fusion(black_box(text)).unwrap()));
        });
        println!();
    }

    group.finish();

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                         ANALYSIS GUIDE                               ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("🎯 CRITICAL TEST: Every stage transforms text, no redundancy");
    println!();
    println!("📊 What we're measuring:");
    println!("   • 2-stage: Does fusion beat 2 sequential passes?");
    println!("   • 3-stage: Does fusion beat 3 sequential passes?");
    println!("   • 4-stage: Does fusion beat 4 sequential passes?");
    println!();
    println!("✅ If fusion WINS:");
    println!("   • Single-pass iteration < multi-pass overhead");
    println!("   • Keep fusion for 2+ stage pipelines");
    println!("   • Validates the design");
    println!();
    println!("❌ If fusion LOSES:");
    println!("   • Iterator overhead > saved iterations");
    println!("   • Remove fusion entirely");
    println!("   • Fundamental flaw in implementation");
    println!();
    println!("💡 Watch for:");
    println!("   • Break-even point: At what stage count does fusion win?");
    println!("   • Language variance: Does complexity affect fusion benefit?");
    println!("   • Magnitude: Small differences (<10%) vs large (>20%)");
    println!();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(3))
        .warm_up_time(Duration::from_secs(1))
        .sample_size(500);
    targets = fusion_real_work_benchmark
);
criterion_main!(benches);
