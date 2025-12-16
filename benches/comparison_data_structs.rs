use criterion::{Criterion, criterion_group, criterion_main};
use phf::{Set, phf_set};
use std::{collections::HashSet, hint::black_box};

// ============================================================================
// Test Data Sets - Representing different language property sizes
// ============================================================================

// Small: 2-5 elements (e.g., Turkish case map, Bengali diacritics)
const SMALL_SLICE: &[char] = &['İ', 'I', 'ı', 'i', 'Ė'];
lazy_static::lazy_static! {
    static ref SMALL_HASHSET: HashSet<char> = SMALL_SLICE.iter().copied().collect();
}
static SMALL_PHF: Set<char> = phf_set! {
    'İ', 'I', 'ı', 'i', 'Ė'
};

// Medium: 10-20 elements (e.g., Polish precomposed, Czech precomposed, Hebrew diacritics)
const MEDIUM_SLICE: &[char] = &[
    'Ą', 'ą', 'Ć', 'ć', 'Ę', 'ę', 'Ł', 'ł', 'Ń', 'ń', 'Ó', 'ó', 'Ś', 'ś', 'Ź', 'ź', 'Ż', 'ż',
];
lazy_static::lazy_static! {
    static ref MEDIUM_HASHSET: HashSet<char> = MEDIUM_SLICE.iter().copied().collect();
}
static MEDIUM_PHF: Set<char> = phf_set! {
    'Ą', 'ą', 'Ć', 'ć', 'Ę', 'ę', 'Ł', 'ł', 'Ń', 'ń',
    'Ó', 'ó', 'Ś', 'ś', 'Ź', 'ź', 'Ż', 'ż'
};

// Large: 30-66 elements (e.g., Russian transliterate, Khmer diacritics)
const LARGE_SLICE: &[char] = &[
    'А', 'а', 'Б', 'б', 'В', 'в', 'Г', 'г', 'Д', 'д', 'Е', 'е', 'Ё', 'ё', 'Ж', 'ж', 'З', 'з', 'И',
    'и', 'Й', 'й', 'К', 'к', 'Л', 'л', 'М', 'м', 'Н', 'н', 'О', 'о', 'П', 'п', 'Р', 'р', 'С', 'с',
    'Т', 'т', 'У', 'у', 'Ф', 'ф', 'Х', 'х', 'Ц', 'ц', 'Ч', 'ч', 'Ш', 'ш', 'Щ', 'щ', 'Ъ', 'ъ', 'Ы',
    'ы', 'Ь', 'ь', 'Э', 'э', 'Ю', 'ю', 'Я', 'я',
];
lazy_static::lazy_static! {
    static ref LARGE_HASHSET: HashSet<char> = LARGE_SLICE.iter().copied().collect();
}
static LARGE_PHF: Set<char> = phf_set! {
    'А', 'а', 'Б', 'б', 'В', 'в', 'Г', 'г', 'Д', 'д',
    'Е', 'е', 'Ё', 'ё', 'Ж', 'ж', 'З', 'з', 'И', 'и',
    'Й', 'й', 'К', 'к', 'Л', 'л', 'М', 'м', 'Н', 'н',
    'О', 'о', 'П', 'п', 'Р', 'р', 'С', 'с', 'Т', 'т',
    'У', 'у', 'Ф', 'ф', 'Х', 'х', 'Ц', 'ц', 'Ч', 'ч',
    'Ш', 'ш', 'Щ', 'щ', 'Ъ', 'ъ', 'Ы', 'ы', 'Ь', 'ь',
    'Э', 'э', 'Ю', 'ю', 'Я', 'я'
};

// Very Large: 134 elements (Vietnamese precomposed)
const VERY_LARGE_SLICE: &[char] = &[
    'À', 'à', 'Á', 'á', 'Ả', 'ả', 'Ã', 'ã', 'Ạ', 'ạ', 'Ă', 'ă', 'Ằ', 'ằ', 'Ắ', 'ắ', 'Ẳ', 'ẳ', 'Ẵ',
    'ẵ', 'Ặ', 'ặ', 'Â', 'â', 'Ầ', 'ầ', 'Ấ', 'ấ', 'Ẩ', 'ẩ', 'Ẫ', 'ẫ', 'Ậ', 'ậ', 'È', 'è', 'É', 'é',
    'Ẻ', 'ẻ', 'Ẽ', 'ẽ', 'Ẹ', 'ẹ', 'Ê', 'ê', 'Ề', 'ề', 'Ế', 'ế', 'Ể', 'ể', 'Ễ', 'ễ', 'Ệ', 'ệ', 'Ì',
    'ì', 'Í', 'í', 'Ỉ', 'ỉ', 'Ĩ', 'ĩ', 'Ị', 'ị', 'Ò', 'ò', 'Ó', 'ó', 'Ỏ', 'ỏ', 'Õ', 'õ', 'Ọ', 'ọ',
    'Ô', 'ô', 'Ồ', 'ồ', 'Ố', 'ố', 'Ổ', 'ổ', 'Ỗ', 'ỗ', 'Ộ', 'ộ', 'Ơ', 'ơ', 'Ờ', 'ờ', 'Ớ', 'ớ', 'Ở',
    'ở', 'Ỡ', 'ỡ', 'Ợ', 'ợ', 'Ù', 'ù', 'Ú', 'ú', 'Ủ', 'ủ', 'Ũ', 'ũ', 'Ụ', 'ụ', 'Ư', 'ư', 'Ừ', 'ừ',
    'Ứ', 'ứ', 'Ử', 'ử', 'Ữ', 'ữ', 'Ự', 'ự', 'Ỳ', 'ỳ', 'Ý', 'ý', 'Ỷ', 'ỷ', 'Ỹ', 'ỹ', 'Ỵ', 'ỵ', 'Đ',
    'đ',
];
lazy_static::lazy_static! {
    static ref VERY_LARGE_HASHSET: HashSet<char> = VERY_LARGE_SLICE.iter().copied().collect();
}
static VERY_LARGE_PHF: Set<char> = phf_set! {
    'À', 'à', 'Á', 'á', 'Ả', 'ả', 'Ã', 'ã', 'Ạ', 'ạ',
    'Ă', 'ă', 'Ằ', 'ằ', 'Ắ', 'ắ', 'Ẳ', 'ẳ', 'Ẵ', 'ẵ',
    'Ặ', 'ặ', 'Â', 'â', 'Ầ', 'ầ', 'Ấ', 'ấ', 'Ẩ', 'ẩ',
    'Ẫ', 'ẫ', 'Ậ', 'ậ', 'È', 'è', 'É', 'é', 'Ẻ', 'ẻ',
    'Ẽ', 'ẽ', 'Ẹ', 'ẹ', 'Ê', 'ê', 'Ề', 'ề', 'Ế', 'ế',
    'Ể', 'ể', 'Ễ', 'ễ', 'Ệ', 'ệ', 'Ì', 'ì', 'Í', 'í',
    'Ỉ', 'ỉ', 'Ĩ', 'ĩ', 'Ị', 'ị', 'Ò', 'ò', 'Ó', 'ó',
    'Ỏ', 'ỏ', 'Õ', 'õ', 'Ọ', 'ọ', 'Ô', 'ô', 'Ồ', 'ồ',
    'Ố', 'ố', 'Ổ', 'ổ', 'Ỗ', 'ỗ', 'Ộ', 'ộ', 'Ơ', 'ơ',
    'Ờ', 'ờ', 'Ớ', 'ớ', 'Ở', 'ở', 'Ỡ', 'ỡ', 'Ợ', 'ợ',
    'Ù', 'ù', 'Ú', 'ú', 'Ủ', 'ủ', 'Ũ', 'ũ', 'Ụ', 'ụ',
    'Ư', 'ư', 'Ừ', 'ừ', 'Ứ', 'ứ', 'Ử', 'ử', 'Ữ', 'ữ',
    'Ự', 'ự', 'Ỳ', 'ỳ', 'Ý', 'ý', 'Ỷ', 'ỷ', 'Ỹ', 'ỹ',
    'Ỵ', 'ỵ', 'Đ', 'đ'
};

// ============================================================================
// Test Characters
// ============================================================================

struct TestChars {
    hit_first: char,  // First element (best case for linear search)
    hit_middle: char, // Middle element
    hit_last: char,   // Last element (worst case for linear search)
    miss: char,       // Not in set
}

const SMALL_TEST: TestChars = TestChars {
    hit_first: 'İ',
    hit_middle: 'ı',
    hit_last: 'Ė',
    miss: 'X',
};

const MEDIUM_TEST: TestChars = TestChars {
    hit_first: 'Ą',
    hit_middle: 'Ó',
    hit_last: 'ż',
    miss: 'X',
};

const LARGE_TEST: TestChars = TestChars {
    hit_first: 'А',
    hit_middle: 'О',
    hit_last: 'я',
    miss: 'X',
};

const VERY_LARGE_TEST: TestChars = TestChars {
    hit_first: 'À',
    hit_middle: 'Ò',
    hit_last: 'đ',
    miss: 'X',
};

// ============================================================================
// Benchmark Functions
// ============================================================================

fn bench_small(c: &mut Criterion) {
    let mut group = c.benchmark_group("small_dataset_5_chars");

    // Slice contains
    group.bench_function("slice_contains_hit_first", |b| {
        b.iter(|| SMALL_SLICE.contains(&black_box(SMALL_TEST.hit_first)))
    });
    group.bench_function("slice_contains_hit_last", |b| {
        b.iter(|| SMALL_SLICE.contains(&black_box(SMALL_TEST.hit_last)))
    });
    group.bench_function("slice_contains_miss", |b| {
        b.iter(|| SMALL_SLICE.contains(&black_box(SMALL_TEST.miss)))
    });

    // Iter any
    group.bench_function("iter_any_hit_first", |b| {
        b.iter(|| {
            SMALL_SLICE
                .iter()
                .any(|&x| x == black_box(SMALL_TEST.hit_first))
        })
    });
    group.bench_function("iter_any_hit_last", |b| {
        b.iter(|| {
            SMALL_SLICE
                .iter()
                .any(|&x| x == black_box(SMALL_TEST.hit_last))
        })
    });
    group.bench_function("iter_any_miss", |b| {
        b.iter(|| SMALL_SLICE.iter().any(|&x| x == black_box(SMALL_TEST.miss)))
    });

    // Match expression (exhaustive)
    group.bench_function("match_hit", |b| {
        b.iter(|| {
            matches!(
                black_box(SMALL_TEST.hit_middle),
                'İ' | 'I' | 'ı' | 'i' | 'Ė'
            )
        })
    });
    group.bench_function("match_miss", |b| {
        b.iter(|| matches!(black_box(SMALL_TEST.miss), 'İ' | 'I' | 'ı' | 'i' | 'Ė'))
    });

    // HashSet
    group.bench_function("hashset_hit", |b| {
        b.iter(|| SMALL_HASHSET.contains(&black_box(SMALL_TEST.hit_middle)))
    });
    group.bench_function("hashset_miss", |b| {
        b.iter(|| SMALL_HASHSET.contains(&black_box(SMALL_TEST.miss)))
    });

    // PHF Set
    group.bench_function("phf_hit", |b| {
        b.iter(|| SMALL_PHF.contains(&black_box(SMALL_TEST.hit_middle)))
    });
    group.bench_function("phf_miss", |b| {
        b.iter(|| SMALL_PHF.contains(&black_box(SMALL_TEST.miss)))
    });

    group.finish();
}

fn bench_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("medium_dataset_18_chars");

    group.bench_function("slice_contains_hit_first", |b| {
        b.iter(|| MEDIUM_SLICE.contains(&black_box(MEDIUM_TEST.hit_first)))
    });
    group.bench_function("slice_contains_hit_last", |b| {
        b.iter(|| MEDIUM_SLICE.contains(&black_box(MEDIUM_TEST.hit_last)))
    });
    group.bench_function("slice_contains_miss", |b| {
        b.iter(|| MEDIUM_SLICE.contains(&black_box(MEDIUM_TEST.miss)))
    });

    group.bench_function("iter_any_hit_first", |b| {
        b.iter(|| {
            MEDIUM_SLICE
                .iter()
                .any(|&x| x == black_box(MEDIUM_TEST.hit_first))
        })
    });
    group.bench_function("iter_any_hit_last", |b| {
        b.iter(|| {
            MEDIUM_SLICE
                .iter()
                .any(|&x| x == black_box(MEDIUM_TEST.hit_last))
        })
    });

    group.bench_function("hashset_hit", |b| {
        b.iter(|| MEDIUM_HASHSET.contains(&black_box(MEDIUM_TEST.hit_middle)))
    });
    group.bench_function("hashset_miss", |b| {
        b.iter(|| MEDIUM_HASHSET.contains(&black_box(MEDIUM_TEST.miss)))
    });

    group.bench_function("phf_hit", |b| {
        b.iter(|| MEDIUM_PHF.contains(&black_box(MEDIUM_TEST.hit_middle)))
    });
    group.bench_function("phf_miss", |b| {
        b.iter(|| MEDIUM_PHF.contains(&black_box(MEDIUM_TEST.miss)))
    });

    group.finish();
}

fn bench_large(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_dataset_66_chars");

    group.bench_function("slice_contains_hit_first", |b| {
        b.iter(|| LARGE_SLICE.contains(&black_box(LARGE_TEST.hit_first)))
    });
    group.bench_function("slice_contains_hit_last", |b| {
        b.iter(|| LARGE_SLICE.contains(&black_box(LARGE_TEST.hit_last)))
    });
    group.bench_function("slice_contains_miss", |b| {
        b.iter(|| LARGE_SLICE.contains(&black_box(LARGE_TEST.miss)))
    });

    group.bench_function("hashset_hit", |b| {
        b.iter(|| LARGE_HASHSET.contains(&black_box(LARGE_TEST.hit_middle)))
    });
    group.bench_function("hashset_miss", |b| {
        b.iter(|| LARGE_HASHSET.contains(&black_box(LARGE_TEST.miss)))
    });

    group.bench_function("phf_hit", |b| {
        b.iter(|| LARGE_PHF.contains(&black_box(LARGE_TEST.hit_middle)))
    });
    group.bench_function("phf_miss", |b| {
        b.iter(|| LARGE_PHF.contains(&black_box(LARGE_TEST.miss)))
    });

    group.finish();
}

fn bench_very_large(c: &mut Criterion) {
    let mut group = c.benchmark_group("very_large_dataset_134_chars");

    group.bench_function("slice_contains_hit_first", |b| {
        b.iter(|| VERY_LARGE_SLICE.contains(&black_box(VERY_LARGE_TEST.hit_first)))
    });
    group.bench_function("slice_contains_hit_last", |b| {
        b.iter(|| VERY_LARGE_SLICE.contains(&black_box(VERY_LARGE_TEST.hit_last)))
    });
    group.bench_function("slice_contains_miss", |b| {
        b.iter(|| VERY_LARGE_SLICE.contains(&black_box(VERY_LARGE_TEST.miss)))
    });

    group.bench_function("hashset_hit", |b| {
        b.iter(|| VERY_LARGE_HASHSET.contains(&black_box(VERY_LARGE_TEST.hit_middle)))
    });
    group.bench_function("hashset_miss", |b| {
        b.iter(|| VERY_LARGE_HASHSET.contains(&black_box(VERY_LARGE_TEST.miss)))
    });

    group.bench_function("phf_hit", |b| {
        b.iter(|| VERY_LARGE_PHF.contains(&black_box(VERY_LARGE_TEST.hit_middle)))
    });
    group.bench_function("phf_miss", |b| {
        b.iter(|| VERY_LARGE_PHF.contains(&black_box(VERY_LARGE_TEST.miss)))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_small,
    bench_medium,
    bench_large,
    bench_very_large
);
criterion_main!(benches);

// ============================================================================
// Cargo.toml dependencies needed:
// ============================================================================
// [dev-dependencies]
// criterion = "0.5"
// phf = { version = "0.11", features = ["macros"] }
// lazy_static = "1.4"
//
// [[bench]]
// name = "lang_lookup"
// harness = false
