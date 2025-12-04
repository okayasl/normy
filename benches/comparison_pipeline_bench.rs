#![deny(unsafe_code)]
#![warn(clippy::all)]
#![allow(clippy::must_use_candidate, clippy::missing_errors_doc)]

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use icu_normalizer::{
    ComposingNormalizer, ComposingNormalizerBorrowed, DecomposingNormalizer,
    DecomposingNormalizerBorrowed,
};
use normy::{
    process::{ChainedProcess, EmptyProcess},
    stage::normalization::{NfcStage, NfdStage, NfkcStage, NfkdStage},
};
use rand::{Rng, SeedableRng, random, rngs::StdRng};
use std::sync::LazyLock;
use std::{borrow::Cow, hint::black_box};

use tokenizers::{
    NormalizedString, Normalizer,
    normalizers::{
        Sequence, unicode::NFC as tokenizerNFC, unicode::NFD as tokenizerNFD,
        unicode::NFKC as tokenizerNFKC, unicode::NFKD as tokenizerNFKD,
    },
};

use normy::{NFC, NFD, NFKC, NFKD, Normy, NormyBuilder};
use unicode_normalization::UnicodeNormalization;
// ──────────────────────────────────────────────────────────────
// 20+ Language-Specific Stress Samples (Injected into all corpora)
// ──────────────────────────────────────────────────────────────
static STRESS_POOL_NFC_NFD: &[&str] = &[
    // 1. Vietnamese – stacked diacritics (worst-case NFD explosion)
    "Tiếng Việt Quốc ngữ Phở Hà Nội",
    // 2. French – precomposed + ligatures
    "Sœur naïve à l’œuf ŒUF déjà-vu",
    // 3. German – ß and ligatures
    "Fußball Straße Maßstab GRÜNE STRAẞE",
    // 4. Turkish – dotted/dotless I
    "İSTANBUL İĞNE İĞDE ıiIİ",
    // 5. Spanish – ñ + inverted punctuation
    "¡España mañana José Peña!",
    // 6. Polish – ogonek + kreska
    "Łódź żółć ŻÓŁĆ Żubrówka",
    // 7. Lithuanian – preserves i with ogonek
    "Žemaitija Šiauliai Jurgis",
    // 8. Icelandic – eth and thorn
    "Þetta er íslenska ÐðÞþ",
    // 9. Romanian – ș and ț (comma below)
    "Ștefan Țară România",
    // 10. Croatian – đ and lj/nj digraphs
    "Đuro Đaković Ljiljana Njiva",
    // 11. Greek – final sigma + tonos
    "Ἀρχιμήδης Ἑλλάς σοφός",
    // 12. Russian – yo + soft sign
    "Ёлки-палки всё А́нна",
    // 13. Arabic – shadda + harakat
    "الْكِتَابُ مُحَمَّدٌ ـــ",
    // 14. Hebrew – niqqud + final forms
    "סֵפֶר עִבְרִית שׂ",
    // 15. Hindi – conjuncts + nukta
    "हिन्दी ज़िंदगी क़िला",
    // 16. Thai – no spaces + tone marks
    "ภาษาไทย สวัสดีครับ ๑๒๓",
    // 17. Korean – jamo + full-width
    "한글 ＫＯＲＥＡ 한국어",
    // 18. Japanese – half-width kana + prolonged sound
    "ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟ ーー こんにちは",
    // 19. Chinese – full-width punctuation + letters
    "ＨＴＭＬ　＜ｔａｇ＞　你好世界",
    // 20. Emoji + skin tone + ZWJ
    "👨‍👩‍👧‍👦 👍🏼 ✨ 🚀",
    // Bonus: Ligature soup
    "ﬁﬂﬃﬄﬆﬀﬁﬃﬃﬃ",
];

static STRESS_POOL_NFKC_NFKD: &[&str] = &[
    "ﬀ ﬁ ﬂ ﬃ ﬄ ﬆ ﬁﬀﬃﬃ",                 // Latin ligatures
    "½ ⅓ ¼ ⅕ ⅙ ⅛ ⅔ ¾",                  // Fractions
    "①②③④⑤ ⑩ ⑴⑵⑶ ⒈⒉⒊",                  // Circled/enclosed numbers
    "Ｈｅｌｌｏ　Ｗｏｒｌｄ　＆　＜＞", // Full-width Latin + punctuation
    "㈱ ㈲ ㎏ ㎞ ㎡",                   // CJK compatibility (company, kg, km²)
    "№ ℡ ™ © ®",                        // Symbols
    "ﬃﬃﬃﬃ ﬃﬃﬃﬃ",                        // Triple ligatures
    "ﬀﬃ ﬃﬃ ﬄﬃ",                         // Mixed ligatures
    "stﬀ stﬂ stﬃ",                      // st ligature variants
];

/// Enhanced realistic corpus with guaranteed transformation triggers
fn realistic_corpus(seed: u64, size_kb: usize) -> String {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut out = String::with_capacity(size_kb * 1024);

    let pools = if rng.random_bool(0.5) {
        &[STRESS_POOL_NFC_NFD, STRESS_POOL_NFKC_NFKD]
    } else {
        &[STRESS_POOL_NFKC_NFKD, STRESS_POOL_NFC_NFD]
    };

    while out.len() < size_kb * 1024 {
        let pool = pools[rng.random_range(0..pools.len())];
        let text = pool[rng.random_range(0..pool.len())];
        let repeat = rng.random_range(1..=5);
        for _ in 0..repeat {
            out.push_str(text);
            out.push(' ');
        }
        // Random ASCII filler
        if rng.random_bool(0.1) {
            let word: String = (0..rng.random_range(5..20))
                .map(|_| (b'a' + (random::<u8>() % 26)) as char)
                .collect();
            out.push_str(&word);
            out.push(' ');
        }
    }

    truncate_to_char_boundary(&mut out, size_kb * 1024);
    out
}

fn truncate_to_char_boundary(s: &mut String, max_len: usize) {
    if s.len() > max_len {
        while !s.is_char_boundary(max_len) && !s.is_empty() {
            s.pop();
        }
        s.truncate(max_len);
    }
}

// ── Zero-Copy Tracker ──
#[derive(Default)]
struct ZeroCopyTracker {
    name: String,
    hits: usize,
    total: usize,
}

impl ZeroCopyTracker {
    fn new(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    #[allow(clippy::ptr_arg)]
    fn record(&mut self, input: &str, output: &Cow<'_, str>) {
        self.total += 1;
        if matches!(output, Cow::Borrowed(s) if s.as_ptr() == input.as_ptr() && s.len() == input.len())
        {
            self.hits += 1;
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn hit_rate_pct(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.hits as f64 / self.total as f64) * 100.0
        }
    }

    fn print(&self) {
        println!(
            "Case: {} → ZERO-COPY: {:.2}% ({}/{})",
            self.name,
            self.hit_rate_pct(),
            self.hits,
            self.total
        );
    }
}
