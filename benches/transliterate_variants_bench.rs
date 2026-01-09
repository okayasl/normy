use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use normy::{CAT, DAN, FRA, ISL, NOR, SWE};
use normy::{DEU, RUS, Transliterate, context::Context, stage::StaticFusableStage};
use std::hint::black_box;
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════════════
// APPLY IMPLEMENTATION VARIANTS
// ═══════════════════════════════════════════════════════════════════════════

/// Current implementation (manual push_str/push loop)
fn apply_current(text: &str, ctx: &Context) -> String {
    let mut out = String::with_capacity(text.len() + (text.len() >> 3));
    for c in text.chars() {
        if let Some(replacement) = ctx.lang_entry.find_transliterate_map(c) {
            out.push_str(replacement);
        } else {
            out.push(c);
        }
    }
    out
}

/// Delegate to fusion (apply calls fusion internally)
fn apply_use_fusion(text: &str, ctx: &Context) -> String {
    let stage = Transliterate;
    stage.static_fused_adapter(text.chars(), ctx).collect()
}

fn apply_optmized(text: &str, ctx: &Context) -> String {
    let entry = &ctx.lang_entry;

    let capacity = if entry.has_one_to_one_transliterate() {
        // Guaranteed to be close to or exactly text.len()
        text.len()
    } else {
        // Growth required (e.g., German Ä -> ae)
        text.len() + (text.len() >> 3)
    };

    let mut out = String::with_capacity(capacity);
    for c in text.chars() {
        if let Some(replacement) = ctx.lang_entry.find_transliterate_map(c) {
            out.push_str(replacement);
        } else {
            out.push(c);
        }
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST DATA & BENCHMARK CORE
// ═══════════════════════════════════════════════════════════════════════════

struct TestCase {
    lang_name: &'static str,
    ctx: Context,
    changed_base: &'static str,
}

const LENGTH_CONFIGS: &[(&str, usize)] = &[("short", 500), ("medium", 2000), ("long", 10000)];

fn generate_text(base: &str, target_bytes: usize) -> String {
    let mut result = String::with_capacity(target_bytes + base.len());
    while result.len() < target_bytes {
        result.push_str(base);
    }

    // Safety: Ensure we don't truncate in the middle of a multi-byte char
    let mut limit = target_bytes;
    while limit > 0 && !result.is_char_boundary(limit) {
        limit -= 1;
    }
    result.truncate(limit);
    result
}

fn bench_transliterate_implementations(c: &mut Criterion) {
    let mut group = c.benchmark_group("transliterate_apply_variants");

    let test_cases = vec![
        TestCase {
            lang_name: "Russian",
            ctx: Context::new(RUS),
            changed_base: "Привет мир ADFDVERER ", // Triggers Cyrillic transliteration
        },
        TestCase {
            lang_name: "German",
            ctx: Context::new(DEU),
            changed_base: "Äöü ÄÖÜ ADFDVERER ", // Triggers Ä -> ae, etc.
        },
        TestCase {
            lang_name: "Danish",
            ctx: Context::new(DAN),
            changed_base: "ÅåÆæØø ADFDVERER ", // Triggers Å -> aa, etc.
        },
        TestCase {
            lang_name: "Norwegian",
            ctx: Context::new(NOR),
            changed_base: "ÆæØøÅå ADFDVERER ", // Triggers Æ -> ae, etc.
        },
        TestCase {
            lang_name: "Swedish",
            ctx: Context::new(SWE),
            changed_base: "ÅåÄäÖö ADFDVERER ", // Triggers Å -> aa, etc.
        },
        TestCase {
            lang_name: "Icelandic",
            ctx: Context::new(ISL),
            changed_base: "ÞþÐðÆæ ADFDVERER ", // Triggers Þ -> th, etc.
        },
        TestCase {
            lang_name: "French",
            ctx: Context::new(FRA),
            changed_base: "ŒœÆæÇç ADFDVERER ", // Triggers Œ -> oe, etc.
        },
        TestCase {
            lang_name: "Catalan",
            ctx: Context::new(CAT),
            changed_base: "Çç ADFDVERER ", // Triggers Ç -> c
        },
    ];

    for test_case in &test_cases {
        for &(size_name, target_bytes) in LENGTH_CONFIGS {
            let text = generate_text(test_case.changed_base, target_bytes);
            let bench_id = format!("{}/{}", test_case.lang_name, size_name);

            // Verify logic consistency
            let expected = apply_current(&text, &test_case.ctx);
            assert_eq!(
                expected,
                apply_optmized(&text, &test_case.ctx),
                "V9 mismatch in {}",
                bench_id
            );
            assert_eq!(
                expected,
                apply_use_fusion(&text, &test_case.ctx),
                "V7 mismatch in {}",
                bench_id
            );

            group.bench_function(BenchmarkId::new("apply_current", &bench_id), |b| {
                b.iter(|| apply_current(black_box(&text), black_box(&test_case.ctx)))
            });

            group.bench_function(BenchmarkId::new("apply_fusion", &bench_id), |b| {
                b.iter(|| apply_use_fusion(black_box(&text), black_box(&test_case.ctx)))
            });

            group.bench_function(BenchmarkId::new("apply_optimized", &bench_id), |b| {
                b.iter(|| apply_optmized(black_box(&text), black_box(&test_case.ctx)))
            });
        }
    }
    group.finish();
}

// ═══════════════════════════════════════════════════════════════════════════
// NEW ANALYSIS FOCUS
// ═══════════════════════════════════════════════════════════════════════════
/*
🔍 WHAT TO LOOK FOR IN V9 (LAZY BUILDER):

1. CLEAN PREFIXES:
   If the first 1000 chars are ASCII and only the last char is 'Ä',
   V9 should crush V1 and V7 because it uses a single `memcpy`
   for the first 1000 chars instead of a character loop.

2. ENGLISH/NO-CHANGE CASE:
   V9 should perform similarly to V6 (very fast) because it never
   enters the allocation logic.

3. RUSSIAN (MANY CHANGES):
   If changes are frequent, V9 might be slightly slower than V7
   because V7's `collect()` is extremely optimized for continuous
   character streams.



🎯 EXPECTED WINNER:
- V9 for mostly-clean text or large strings with few changes.
- V7 (Fusion) for text that is 100% non-ASCII (like Russian) where the
  bulk-copy optimization never gets to run.
*/

criterion_group!(
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(3))
        .warm_up_time(Duration::from_secs(1))
        .sample_size(200);
    targets = bench_transliterate_implementations
);
criterion_main!(benches);
