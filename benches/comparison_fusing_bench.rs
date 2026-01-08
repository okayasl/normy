use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{
    ARA, COLLAPSE_WHITESPACE_UNICODE, CaseFold, DEU, FRA, JPN, NormalizePunctuation, POL, RUS,
    RemoveDiacritics, SegmentWords, StripControlChars, Transliterate, UnifyWidth, VIE,
};
use std::{hint::black_box, time::Duration};

// ═══════════════════════════════════════════════════════════════════════════
// BASE TEXT SAMPLES (Short samples for pattern repetition)
// ═══════════════════════════════════════════════════════════════════════════

const GERMAN_BASE: &str = "GRÜßE SCHÖNẞE ÄPFEL müßen überäll verfügbär sein.";
const VIETNAMESE_BASE: &str = "TIẾNG VIỆT HÀ NỘI PHỞ rất ngon và đẹp. ";
const POLISH_BASE: &str = "ŁÓDŹ KRAKÓW GDAŃSK są piękne. Większość ludzi. ";
const RUSSIAN_BASE: &str = "МОСКВА РОССИЯ САНКТ-ПЕТЕРБУРГ очень красивые города. ";
const JAPANESE_BASE: &str = "ＨＥＬＬＯ　ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟ－－－日本語です。";
const ARABIC_BASE: &str = "اَلْعَرَبِيَّةُ\u{200B}\u{200C}اللغة جميلة جداً. ";
const FRENCH_BASE: &str = "ŒUVRE FRANÇAIS CAFÉ---ÉLÈVE très magnifique. ";

// ═══════════════════════════════════════════════════════════════════════════
// TEXT GENERATION HELPER
// ═══════════════════════════════════════════════════════════════════════════

fn generate_text(base: &str, target_len: usize) -> String {
    if target_len <= base.len() {
        return base.to_string();
    }

    let repetitions = (target_len / base.len()) + 1;
    let mut result = String::with_capacity(target_len);

    for _ in 0..repetitions {
        result.push_str(base);
        if result.len() >= target_len {
            break;
        }
    }

    if result.len() > target_len {
        let mut truncate_at = target_len;
        while truncate_at > 0 && !result.is_char_boundary(truncate_at) {
            truncate_at -= 1;
        }
        result.truncate(truncate_at);
    }

    result
}

// ═══════════════════════════════════════════════════════════════════════════
// LENGTH SCALING BENCHMARKS
// ═══════════════════════════════════════════════════════════════════════════

fn fusion_length_scaling_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("fusion_length_scaling");

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║           FUSION LENGTH SCALING: WHERE DOES FUSION WIN?             ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");

    let length_configs = vec![
        ("tiny", 25, "Original short samples"),
        ("short", 100, "Single sentence"),
        ("medium", 500, "Paragraph"),
        ("long", 2000, "Multi-paragraph"),
        ("huge", 10000, "Document"),
        ("massive", 50000, "Large document"),
    ];

    // ═══════════════════════════════════════════════════════════════════════
    // GERMAN: 2-STAGE PIPELINE (Fold + Transliterate)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🇩🇪 GERMAN: CaseFold + Transliterate");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let german_pipeline = normy::Normy::builder()
        .lang(DEU)
        .add_stage(CaseFold)
        .add_stage(Transliterate)
        .build();

    for (size_name, target_len, description) in &length_configs {
        let text = generate_text(GERMAN_BASE, *target_len);
        let actual_len = text.len();

        println!(
            "  📏 {} ({} bytes - {})",
            size_name, actual_len, description
        );
        println!(
            "     Sample: \"{}...\"",
            &text.chars().take(50).collect::<String>()
        );

        let bench_name = format!("german_{}", size_name);

        // OPTIMIZED: Use bench_function instead of bench_with_input
        // The text is already in scope, no need to pass it as input
        group.bench_function(BenchmarkId::new("fusion", &bench_name), |b| {
            b.iter(|| black_box(german_pipeline.normalize(&text).unwrap()))
        });

        group.bench_function(BenchmarkId::new("no_fusion", &bench_name), |b| {
            b.iter(|| black_box(german_pipeline.normalize_no_fusion(&text).unwrap()))
        });
    }
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // VIETNAMESE: 2-STAGE PIPELINE (Fold + RemoveDiacritics)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🇻🇳 VIETNAMESE: CaseFold + RemoveDiacritics");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let vietnamese_pipeline = normy::Normy::builder()
        .lang(VIE)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .build();

    for (size_name, target_len, description) in &length_configs {
        let text = generate_text(VIETNAMESE_BASE, *target_len);
        let actual_len = text.len();

        println!(
            "  📏 {} ({} bytes - {})",
            size_name, actual_len, description
        );

        let bench_name = format!("vietnamese_{}", size_name);

        group.bench_function(BenchmarkId::new("fusion", &bench_name), |b| {
            b.iter(|| black_box(vietnamese_pipeline.normalize(&text).unwrap()))
        });

        group.bench_function(BenchmarkId::new("no_fusion", &bench_name), |b| {
            b.iter(|| black_box(vietnamese_pipeline.normalize_no_fusion(&text).unwrap()))
        });
    }
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // RUSSIAN: 2-STAGE PIPELINE (Fold + Transliterate)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🇷🇺 RUSSIAN: CaseFold + Transliterate");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let russian_pipeline = normy::Normy::builder()
        .lang(RUS)
        .add_stage(CaseFold)
        .add_stage(Transliterate)
        .build();

    for (size_name, target_len, description) in &length_configs {
        let text = generate_text(RUSSIAN_BASE, *target_len);
        let actual_len = text.len();

        println!(
            "  📏 {} ({} bytes - {})",
            size_name, actual_len, description
        );

        let bench_name = format!("russian_{}", size_name);

        group.bench_function(BenchmarkId::new("fusion", &bench_name), |b| {
            b.iter(|| black_box(russian_pipeline.normalize(&text).unwrap()))
        });

        group.bench_function(BenchmarkId::new("no_fusion", &bench_name), |b| {
            b.iter(|| black_box(russian_pipeline.normalize_no_fusion(&text).unwrap()))
        });
    }
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // FRENCH: 4-STAGE PIPELINE (Fold + Transliterate + Punct + Strip)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🇫🇷 FRENCH: CaseFold + Transliterate + NormPunct + RemoveDiacritics");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let french_pipeline = normy::Normy::builder()
        .lang(FRA)
        .add_stage(CaseFold)
        .add_stage(Transliterate)
        .add_stage(NormalizePunctuation)
        .add_stage(RemoveDiacritics)
        .build();

    for (size_name, target_len, description) in &length_configs {
        let text = generate_text(FRENCH_BASE, *target_len);
        let actual_len = text.len();

        println!(
            "  📏 {} ({} bytes - {})",
            size_name, actual_len, description
        );

        let bench_name = format!("french_{}", size_name);

        group.bench_function(BenchmarkId::new("fusion", &bench_name), |b| {
            b.iter(|| black_box(french_pipeline.normalize(&text).unwrap()))
        });

        group.bench_function(BenchmarkId::new("no_fusion", &bench_name), |b| {
            b.iter(|| black_box(french_pipeline.normalize_no_fusion(&text).unwrap()))
        });
    }
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // POLISH: 2-STAGE PIPELINE (Fold + RemoveDiacritics)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🇵🇱 POLISH: CaseFold + RemoveDiacritics");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let polish_pipeline = normy::Normy::builder()
        .lang(POL)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .build();

    for (size_name, target_len, description) in &length_configs {
        let text = generate_text(POLISH_BASE, *target_len);
        let actual_len = text.len();

        println!(
            "  📏 {} ({} bytes - {})",
            size_name, actual_len, description
        );

        let bench_name = format!("polish_{}", size_name);

        group.bench_function(BenchmarkId::new("fusion", &bench_name), |b| {
            b.iter(|| black_box(polish_pipeline.normalize(&text).unwrap()))
        });

        group.bench_function(BenchmarkId::new("no_fusion", &bench_name), |b| {
            b.iter(|| black_box(polish_pipeline.normalize_no_fusion(&text).unwrap()))
        });
    }
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // JAPANESE: 4-STAGE PIPELINE (SegmentWords + UnifyWidth + Punct + Whitespace)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🇯🇵 JAPANESE: SegmentWords + UnifyWidth + NormPunct + CollapseWhitespace");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let japanese_pipeline = normy::Normy::builder()
        .lang(JPN)
        .add_stage(SegmentWords)
        .add_stage(UnifyWidth)
        .add_stage(NormalizePunctuation)
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .build();

    for (size_name, target_len, description) in &length_configs {
        let text = generate_text(JAPANESE_BASE, *target_len);
        let actual_len = text.len();

        println!(
            "  📏 {} ({} bytes - {})",
            size_name, actual_len, description
        );

        let bench_name = format!("japanese_{}", size_name);

        group.bench_function(BenchmarkId::new("fusion", &bench_name), |b| {
            b.iter(|| black_box(japanese_pipeline.normalize(&text).unwrap()))
        });

        group.bench_function(BenchmarkId::new("no_fusion", &bench_name), |b| {
            b.iter(|| black_box(japanese_pipeline.normalize_no_fusion(&text).unwrap()))
        });
    }
    println!();

    // ═══════════════════════════════════════════════════════════════════════
    // ARABIC: 2-STAGE PIPELINE (RemoveDiacritics + StripControlChars)
    // ═══════════════════════════════════════════════════════════════════════
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🇸🇦 ARABIC: RemoveDiacritics + StripControlChars");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let arabic_pipeline = normy::Normy::builder()
        .lang(ARA)
        .add_stage(RemoveDiacritics)
        .add_stage(StripControlChars)
        .build();

    for (size_name, target_len, description) in &length_configs {
        let text = generate_text(ARABIC_BASE, *target_len);
        let actual_len = text.len();

        println!(
            "  📏 {} ({} bytes - {})",
            size_name, actual_len, description
        );

        let bench_name = format!("arabic_{}", size_name);

        group.bench_function(BenchmarkId::new("fusion", &bench_name), |b| {
            b.iter(|| black_box(arabic_pipeline.normalize(&text).unwrap()))
        });

        group.bench_function(BenchmarkId::new("no_fusion", &bench_name), |b| {
            b.iter(|| black_box(arabic_pipeline.normalize_no_fusion(&text).unwrap()))
        });
    }
    println!();

    group.finish();

    // ═══════════════════════════════════════════════════════════════════════
    // ANALYSIS GUIDE
    // ═══════════════════════════════════════════════════════════════════════
    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                         ANALYSIS GUIDE                               ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("📊 EXPECTED PERFORMANCE PATTERN:");
    println!();
    println!("   tiny (25 bytes):     fusion MAY BE SLOWER  (overhead > benefit)");
    println!("   short (100 bytes):   fusion BREAKS EVEN    (transition point)");
    println!("   medium (500 bytes):  fusion FASTER 10-20%  (benefit emerges)");
    println!("   long (2KB):          fusion FASTER 20-30%  (clear advantage)");
    println!("   huge (10KB):         fusion FASTER 30-50%  (dominant win)");
    println!("   massive (50KB):      fusion FASTER 40-60%  (maximum benefit)");
    println!();
    println!("🔍 WHAT TO LOOK FOR:");
    println!();
    println!("   1. BREAK-EVEN POINT:");
    println!("      At what text length does fusion start winning?");
    println!();
    println!("   2. SCALING BEHAVIOR:");
    println!("      Does fusion advantage grow with text length?");
    println!();
    println!("   3. STAGE COUNT EFFECT:");
    println!("      • 2-stage: Smaller fusion benefit");
    println!("      • 3-stage: Medium fusion benefit");
    println!("      • 4-stage: Largest fusion benefit");
    println!();
    println!("✅ SUCCESS CRITERIA:");
    println!("   • Fusion wins at medium+ sizes (500+ bytes)");
    println!("   • Advantage scales with text length");
    println!("   • Advantage scales with stage count");
    println!();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(5))
        .warm_up_time(Duration::from_secs(2))
        .sample_size(200);  // REDUCED: 200 is sufficient for large texts
    targets = fusion_length_scaling_benchmark
);
criterion_main!(benches);
