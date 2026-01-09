use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{CaseFold, DEU, ENG, NLD, TUR, context::Context, stage::StaticFusableStage};
use std::hint::black_box;
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════════════
// APPLY IMPLEMENTATION VARIANTS
// ═══════════════════════════════════════════════════════════════════════════

/// V1: Current implementation (for-loop in fallback)
fn apply_v1_current(text: &str, ctx: &Context) -> String {
    let mut out = String::with_capacity(text.len().saturating_mul(13).saturating_div(10));

    for c in text.chars() {
        if let Some(to) = ctx.lang_entry.find_fold_map(c) {
            out.push_str(to);
        } else if let Some(to) = ctx.lang_entry.find_case_map(c) {
            out.push(to);
        } else {
            for ch in c.to_lowercase() {
                out.push(ch);
            }
        }
    }
    out
}

/// V2: Using push(next().unwrap_or(c))
fn apply_v2_push_next(text: &str, ctx: &Context) -> String {
    let mut out = String::with_capacity(text.len().saturating_mul(13).saturating_div(10));

    for c in text.chars() {
        if let Some(to) = ctx.lang_entry.find_fold_map(c) {
            out.push_str(to);
        } else if let Some(to) = ctx.lang_entry.find_case_map(c) {
            out.push(to);
        } else {
            out.push(c.to_lowercase().next().unwrap_or(c));
        }
    }
    out
}

/// V3: Using extend(to_lowercase())
fn apply_v3_extend(text: &str, ctx: &Context) -> String {
    let mut out = String::with_capacity(text.len().saturating_mul(13).saturating_div(10));

    for c in text.chars() {
        if let Some(to) = ctx.lang_entry.find_fold_map(c) {
            out.push_str(to);
        } else if let Some(to) = ctx.lang_entry.find_case_map(c) {
            out.push(to);
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// V4: Smart allocation (exact size for languages without multi-char folds)
fn apply_v4_smart_alloc(text: &str, ctx: &Context) -> String {
    let capacity = if ctx.lang_entry.has_fold_map() {
        text.len().saturating_mul(13).saturating_div(10)
    } else {
        text.len()
    };

    let mut out = String::with_capacity(capacity);

    for c in text.chars() {
        if let Some(to) = ctx.lang_entry.find_fold_map(c) {
            out.push_str(to);
        } else if let Some(to) = ctx.lang_entry.find_case_map(c) {
            out.push(to);
        } else {
            for ch in c.to_lowercase() {
                out.push(ch);
            }
        }
    }
    out
}

/// V5: Branch-once optimization (check map existence once per string)
fn apply_v5_branch_once(text: &str, ctx: &Context) -> String {
    let capacity = if ctx.lang_entry.has_fold_map() {
        text.len().saturating_mul(13).saturating_div(10)
    } else {
        text.len()
    };

    let mut out = String::with_capacity(capacity);

    if ctx.lang_entry.has_fold_map() || ctx.lang_entry.has_case_map() {
        // Path with lookups
        for c in text.chars() {
            if let Some(to) = ctx.lang_entry.find_fold_map(c) {
                out.push_str(to);
            } else if let Some(to) = ctx.lang_entry.find_case_map(c) {
                out.push(to);
            } else {
                for ch in c.to_lowercase() {
                    out.push(ch);
                }
            }
        }
    } else {
        // Fast path: no lookups
        for c in text.chars() {
            for ch in c.to_lowercase() {
                out.push(ch);
            }
        }
    }
    out
}

/// V6: Combined optimizations (smart alloc + branch once + push_next)
fn apply_v6_combined(text: &str, ctx: &Context) -> String {
    let capacity = if ctx.lang_entry.has_fold_map() {
        text.len().saturating_mul(13).saturating_div(10)
    } else {
        text.len()
    };

    let mut out = String::with_capacity(capacity);

    if ctx.lang_entry.has_fold_map() || ctx.lang_entry.has_case_map() {
        for c in text.chars() {
            if let Some(to) = ctx.lang_entry.find_fold_map(c) {
                out.push_str(to);
            } else if let Some(to) = ctx.lang_entry.find_case_map(c) {
                out.push(to);
            } else {
                out.push(c.to_lowercase().next().unwrap_or(c));
            }
        }
    } else {
        for c in text.chars() {
            out.push(c.to_lowercase().next().unwrap_or(c));
        }
    }
    out
}

/// V7: Delegate to fusion (apply calls fusion internally)
fn apply_v7_use_fusion(text: &str, ctx: &Context) -> String {
    let stage = CaseFold;
    stage.static_fused_adapter(text.chars(), ctx).collect()
}

/// FUSION: Reference implementation
fn fusion_reference(text: &str, ctx: &Context) -> String {
    let stage = CaseFold;
    stage.static_fused_adapter(text.chars(), ctx).collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST DATA
// ═══════════════════════════════════════════════════════════════════════════

struct TestCase {
    lang_name: &'static str,
    ctx: Context,
    // Text that triggers transformation
    changed_base: &'static str,
    // Properties
    has_fold_map: bool,
    has_case_map: bool,
}

const LENGTH_CONFIGS: &[(&str, usize)] = &[("short", 500), ("medium", 2000), ("long", 10000)];

fn generate_text(base: &str, target_len: usize) -> String {
    let repetitions = (target_len / base.len()).max(1);
    let mut result = String::with_capacity(target_len);

    for _ in 0..repetitions {
        result.push_str(base);
        if result.len() >= target_len {
            break;
        }
    }

    result.truncate(target_len.min(result.len()));
    result
}

// ═══════════════════════════════════════════════════════════════════════════
// BENCHMARK FUNCTION
// ═══════════════════════════════════════════════════════════════════════════

fn bench_casefold_implementations(c: &mut Criterion) {
    let mut group = c.benchmark_group("casefold_apply_variants");

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║              CaseFold::apply IMPLEMENTATION COMPARISON               ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝\n");

    let test_cases = vec![
        TestCase {
            lang_name: "English",
            ctx: Context::new(ENG),
            changed_base: "HELLO WORLD TEST ",
            has_fold_map: false,
            has_case_map: false,
        },
        TestCase {
            lang_name: "German",
            ctx: Context::new(DEU),
            changed_base: "GRÜẞE STRAẞE ",
            has_fold_map: true, // ß => ss
            has_case_map: false,
        },
        TestCase {
            lang_name: "Dutch",
            ctx: Context::new(NLD),
            changed_base: "ĲSSEL IJMUIDEN ",
            has_fold_map: true, // IJ => ij
            has_case_map: false,
        },
        TestCase {
            lang_name: "Turkish",
            ctx: Context::new(TUR),
            changed_base: "İSTANBUL İĞNE ",
            has_fold_map: false,
            has_case_map: true, // İ => i, I => ı
        },
    ];

    // ═══════════════════════════════════════════════════════════════════════
    // TEST EACH LANGUAGE
    // ═══════════════════════════════════════════════════════════════════════

    for test_case in &test_cases {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "🌍 {} (fold_map: {}, case_map: {})",
            test_case.lang_name, test_case.has_fold_map, test_case.has_case_map
        );
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

        // Test both changed and unchanged text
        let scenarios = vec![
            ("changed", test_case.changed_base),
            //    ("unchanged", test_case.unchanged_base),
        ];

        for (scenario_name, base_text) in &scenarios {
            println!("  📝 Scenario: {}", scenario_name);

            for &(size_name, target_bytes) in LENGTH_CONFIGS {
                let text = generate_text(base_text, target_bytes);

                println!("    📏 {} ({} bytes)", size_name, text.len());

                let bench_id = format!("{}/{}/{}", test_case.lang_name, scenario_name, size_name);

                // Verify all implementations produce same output
                if text.len() < 1000 {
                    // Only verify for smaller texts
                    let results = [
                        apply_v1_current(&text, &test_case.ctx),
                        apply_v2_push_next(&text, &test_case.ctx),
                        apply_v3_extend(&text, &test_case.ctx),
                        apply_v4_smart_alloc(&text, &test_case.ctx),
                        apply_v5_branch_once(&text, &test_case.ctx),
                        apply_v6_combined(&text, &test_case.ctx),
                        apply_v7_use_fusion(&text, &test_case.ctx),
                        fusion_reference(&text, &test_case.ctx),
                    ];

                    for i in 1..results.len() {
                        assert_eq!(
                            results[0], results[i],
                            "Implementation mismatch at index {}",
                            i
                        );
                    }
                }

                // Benchmark each implementation
                group.bench_function(BenchmarkId::new("v1_current", &bench_id), |b| {
                    b.iter(|| apply_v1_current(black_box(&text), black_box(&test_case.ctx)))
                });

                group.bench_function(BenchmarkId::new("v2_push_next", &bench_id), |b| {
                    b.iter(|| apply_v2_push_next(black_box(&text), black_box(&test_case.ctx)))
                });

                group.bench_function(BenchmarkId::new("v3_extend", &bench_id), |b| {
                    b.iter(|| apply_v3_extend(black_box(&text), black_box(&test_case.ctx)))
                });

                group.bench_function(BenchmarkId::new("v4_smart_alloc", &bench_id), |b| {
                    b.iter(|| apply_v4_smart_alloc(black_box(&text), black_box(&test_case.ctx)))
                });

                group.bench_function(BenchmarkId::new("v5_branch_once", &bench_id), |b| {
                    b.iter(|| apply_v5_branch_once(black_box(&text), black_box(&test_case.ctx)))
                });

                group.bench_function(BenchmarkId::new("v6_combined", &bench_id), |b| {
                    b.iter(|| apply_v6_combined(black_box(&text), black_box(&test_case.ctx)))
                });

                group.bench_function(BenchmarkId::new("v7_use_fusion", &bench_id), |b| {
                    b.iter(|| apply_v7_use_fusion(black_box(&text), black_box(&test_case.ctx)))
                });

                group.bench_function(BenchmarkId::new("fusion_ref", &bench_id), |b| {
                    b.iter(|| fusion_reference(black_box(&text), black_box(&test_case.ctx)))
                });
            }
            println!();
        }
    }

    group.finish();

    // ═══════════════════════════════════════════════════════════════════════
    // ANALYSIS GUIDE
    // ═══════════════════════════════════════════════════════════════════════

    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                         ANALYSIS GUIDE                               ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("🔍 WHAT TO LOOK FOR:");
    println!();
    println!("   1. ENGLISH (no maps):");
    println!("      • V5 (branch_once) should dominate - skips all lookups");
    println!("      • V6 (combined) should be close second");
    println!("      • V4 (smart_alloc) should beat V1 (exact allocation)");
    println!();
    println!("   2. GERMAN/DUTCH (fold_map only):");
    println!("      • V4/V6 should beat V1 (better allocation)");
    println!("      • V2 vs V1 vs V3: micro-differences in fallback");
    println!();
    println!("   3. TURKISH (case_map only):");
    println!("      • V5/V6 won't help much (still need lookups)");
    println!("      • V4 (smart_alloc) should help");
    println!();
    println!("   4. FUSION vs APPLY:");
    println!("      • Fusion should be ~10-20% slower for changed text");
    println!("      • If fusion is faster, indicates apply bugs");
    println!();
    println!("   5. TEXT SIZE EFFECTS:");
    println!("      • Small (50-200 bytes): overhead dominates");
    println!("      • Medium (1000 bytes): allocation matters");
    println!("      • Large (5000+ bytes): algorithm efficiency dominates");
    println!();
    println!("📊 EXPECTED WINNERS:");
    println!();
    println!("   English/medium/changed:");
    println!("     1st: V5 or V6 (branch elimination)");
    println!("     2nd: V4 (smart allocation)");
    println!("     3rd: V1-V3 (current approaches)");
    println!();
    println!("   German/medium/changed:");
    println!("     1st: V6 (combined optimizations)");
    println!("     2nd: V4 (smart allocation helps)");
    println!("     3rd: V2 (push_next slightly better)");
    println!();
    println!("   All/huge/changed:");
    println!("     • Allocation differences matter less");
    println!("     • Per-char efficiency dominates");
    println!("     • V5/V6 should maintain lead for English");
    println!();
    println!("✅ DECISION CRITERIA:");
    println!();
    println!("   If V6 wins across all languages/sizes:");
    println!("     → Implement combined optimizations");
    println!();
    println!("   If V4 or V5 wins but not V6:");
    println!("     → Investigate why combination underperforms");
    println!();
    println!("   If current (V1) is competitive:");
    println!("     → Code complexity may not be worth optimization");
    println!();
    println!("   If fusion_ref beats all apply variants:");
    println!("     → Critical bug in apply logic, use V7 (delegate to fusion)");
    println!();
    println!("🎯 RECOMMENDED APPROACH:");
    println!();
    println!("   Run: cargo bench --bench casefold_variants -- --output-format bencher");
    println!("   Then: Compare relative performance across implementations");
    println!("   Focus on: 'changed' scenarios at 'medium' and 'long' sizes");
    println!();
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(3))
        .warm_up_time(Duration::from_secs(1))
        .sample_size(200);
    targets = bench_casefold_implementations
);

criterion_main!(benches);
