use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{DAN, DEU, FRA, RUS, VIE, context::Context};
use std::hint::black_box;
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════════════
// ALGORITHM IMPLEMENTATIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Approach A: Current - Iterating over text characters (Short-circuits on first match)
/// Best for: Text with target chars early, small maps
/// Complexity: O(n * m) worst case, O(k * m) average where k = position of first match
fn needs_apply_text_iter(text: &str, ctx: &Context) -> bool {
    let entry = ctx.lang_entry;
    if !entry.has_transliterate_map() || text.is_ascii() {
        return false;
    }
    text.chars().any(|c| entry.is_transliterable(c))
}

/// Approach B: Alternative - Iterating over map slice (Uses optimized str::contains)
/// Best for: Large maps, text without target chars
/// Complexity: O(m * n) but contains() is optimized (Boyer-Moore-like)
fn needs_apply_map_iter(text: &str, ctx: &Context) -> bool {
    let entry = ctx.lang_entry;
    if !entry.has_transliterate_map() || text.is_ascii() {
        return false;
    }
    entry
        .transliterate_char_slice()
        .iter()
        .any(|&c| text.contains(c))
}

/// Approach C: Hybrid - Use map_iter for large maps, text_iter for small maps
/// Threshold determined empirically
fn needs_apply_hybrid(text: &str, ctx: &Context) -> bool {
    let entry = ctx.lang_entry;
    if !entry.has_transliterate_map() || text.is_ascii() {
        return false;
    }

    let map_size = entry.transliterate_char_slice().len();

    // Threshold: If map has >20 chars, use map iteration
    if map_size > 20 {
        entry
            .transliterate_char_slice()
            .iter()
            .any(|&c| text.contains(c))
    } else {
        text.chars().any(|c| entry.is_transliterable(c))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TEXT GENERATION
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
// TEST DATA STRUCTURES
// ═══════════════════════════════════════════════════════════════════════════

struct TestCase {
    lang_name: &'static str,
    ctx: Context,
    map_size: usize,

    // Different text patterns
    has_target_early: String, // Target char in first 10 chars
    has_target_late: String,  // Target char after 80% of text
    no_target: String,        // Pure ASCII/Latin (no target chars)
}

struct LengthConfig {
    name: &'static str,
    target_bytes: usize,
}

const LENGTH_CONFIGS: &[LengthConfig] = &[
    LengthConfig {
        name: "tiny",
        target_bytes: 50,
    },
    LengthConfig {
        name: "short",
        target_bytes: 200,
    },
    LengthConfig {
        name: "medium",
        target_bytes: 1000,
    },
    LengthConfig {
        name: "long",
        target_bytes: 5000,
    },
    LengthConfig {
        name: "huge",
        target_bytes: 20000,
    },
];

// ═══════════════════════════════════════════════════════════════════════════
// BENCHMARK FUNCTION
// ═══════════════════════════════════════════════════════════════════════════

fn bench_transliterate_logic(c: &mut Criterion) {
    let mut group = c.benchmark_group("transliterate_needs_apply");

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                 needs_apply ALGORITHM COMPARISON                     ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");

    // ═══════════════════════════════════════════════════════════════════════
    // SETUP TEST CASES
    // ═══════════════════════════════════════════════════════════════════════

    let test_cases = vec![
        TestCase {
            lang_name: "German",
            ctx: Context::new(DEU),
            map_size: 6, // Ä, ä, Ö, ö, Ü, ü
            has_target_early: "Äpfel und Birnen sind gesund. ".to_string(),
            has_target_late: "The quick brown fox jumps over the lazy Äpfel.".to_string(),
            no_target: "The quick brown fox jumps over the lazy dog.".to_string(),
        },
        TestCase {
            lang_name: "Danish",
            ctx: Context::new(DAN),
            map_size: 6, // Å, å, Æ, æ, Ø, ø
            has_target_early: "Åse spiser æbler ved Øresund.".to_string(),
            has_target_late: "The quick brown fox jumps over the lazy Øresund.".to_string(),
            no_target: "The quick brown fox jumps over the lazy dog.".to_string(),
        },
        TestCase {
            lang_name: "French",
            ctx: Context::new(FRA),
            map_size: 6, // Œ, œ, Æ, æ, Ç, ç
            has_target_early: "Œuvre française avec des çédilles.".to_string(),
            has_target_late: "The quick brown fox jumps over the lazy Œuvre.".to_string(),
            no_target: "The quick brown fox jumps over the lazy dog.".to_string(),
        },
        TestCase {
            lang_name: "Russian",
            ctx: Context::new(RUS),
            map_size: 66, // Full Cyrillic alphabet (both cases)
            has_target_early: "Привет мир! Hello world.".to_string(),
            has_target_late: "The quick brown fox jumps over the lazy Москва.".to_string(),
            no_target: "The quick brown fox jumps over the lazy dog.".to_string(),
        },
        TestCase {
            lang_name: "Vietnamese",
            ctx: Context::new(VIE),
            map_size: 0, // No transliterate map (uses RemoveDiacritics instead)
            has_target_early: "Tiếng Việt rất đẹp.".to_string(),
            has_target_late: "The quick brown fox jumps over the lazy Việt.".to_string(),
            no_target: "The quick brown fox jumps over the lazy dog.".to_string(),
        },
    ];

    // ═══════════════════════════════════════════════════════════════════════
    // RUN BENCHMARKS
    // ═══════════════════════════════════════════════════════════════════════

    for test_case in &test_cases {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "🌍 {} (map_size: {})",
            test_case.lang_name, test_case.map_size
        );
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        // Skip if no transliterate map
        if test_case.map_size == 0 {
            println!("  ⏭️  No transliterate map - skipping\n");
            continue;
        }

        // Test each pattern at each length
        let patterns = vec![
            ("early", &test_case.has_target_early),
            ("late", &test_case.has_target_late),
            ("none", &test_case.no_target),
        ];

        for (pattern_name, base_text) in &patterns {
            println!("  📝 Pattern: target_{}", pattern_name);

            for config in LENGTH_CONFIGS {
                let text = generate_text(base_text, config.target_bytes);

                println!("    📏 {} ({} bytes)", config.name, text.len());

                let bench_id = format!("{}/{}/{}", test_case.lang_name, pattern_name, config.name);

                // Benchmark: text_iter approach
                group.bench_function(BenchmarkId::new("text_iter", &bench_id), |b| {
                    b.iter(|| needs_apply_text_iter(black_box(&text), black_box(&test_case.ctx)))
                });

                // Benchmark: map_iter approach
                group.bench_function(BenchmarkId::new("map_iter", &bench_id), |b| {
                    b.iter(|| needs_apply_map_iter(black_box(&text), black_box(&test_case.ctx)))
                });

                // Benchmark: hybrid approach
                group.bench_function(BenchmarkId::new("hybrid", &bench_id), |b| {
                    b.iter(|| needs_apply_hybrid(black_box(&text), black_box(&test_case.ctx)))
                });
            }
            println!();
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // FAST-PATH TEST (ASCII early exit)
    // ═══════════════════════════════════════════════════════════════════════

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🚀 FAST-PATH TEST: Pure ASCII (should be ~0ns)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    let ctx_rus = Context::new(RUS);
    let ascii_text = "The quick brown fox jumps over the lazy dog. ".repeat(100);

    group.bench_function("fast_path/text_iter", |b| {
        b.iter(|| needs_apply_text_iter(black_box(&ascii_text), black_box(&ctx_rus)))
    });

    group.bench_function("fast_path/map_iter", |b| {
        b.iter(|| needs_apply_map_iter(black_box(&ascii_text), black_box(&ctx_rus)))
    });

    group.finish();

    // ═══════════════════════════════════════════════════════════════════════
    // ANALYSIS GUIDE
    // ═══════════════════════════════════════════════════════════════════════

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                         ANALYSIS GUIDE                               ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("📊 EXPECTED PATTERNS:");
    println!();
    println!("   SMALL MAPS (German, Danish, French - 6 chars):");
    println!("   • text_iter WINS for early/late patterns (short-circuits quickly)");
    println!("   • map_iter WINS for 'none' pattern (6 optimized contains() calls)");
    println!();
    println!("   LARGE MAPS (Russian - 66 chars):");
    println!("   • text_iter WINS for early pattern (finds match in first chars)");
    println!(
        "   • map_iter WINS for late/none patterns (66 char-by-char checks vs 66 optimized contains)"
    );
    println!();
    println!("   HYBRID:");
    println!("   • Should match best of both: text_iter for small maps, map_iter for large");
    println!();
    println!("🔍 KEY METRICS:");
    println!();
    println!("   1. CROSSOVER POINT:");
    println!("      At what map size does map_iter become better?");
    println!();
    println!("   2. PATTERN SENSITIVITY:");
    println!("      How much does early vs late vs none affect performance?");
    println!();
    println!("   3. LENGTH SCALING:");
    println!("      Does algorithm choice matter more for longer texts?");
    println!();
    println!("   4. FAST-PATH:");
    println!("      Is ASCII check effective? Should be <5ns regardless of approach.");
    println!();
    println!("✅ DECISION CRITERIA:");
    println!();
    println!("   If Russian late/none is >2x faster with map_iter:");
    println!("     → Switch to hybrid approach with threshold ~20 chars");
    println!();
    println!("   If text_iter consistently faster across all cases:");
    println!("     → Keep current implementation");
    println!();
    println!("   If results are mixed/inconsistent:");
    println!("     → Need more profiling or accept current trade-offs");
    println!();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(3))
        .warm_up_time(Duration::from_secs(1))
        .sample_size(200);
    targets = bench_transliterate_logic
);

criterion_main!(benches);
