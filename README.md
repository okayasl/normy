<p align="center">
  <a href="https://crates.io/crates/normy">
    <img src="https://img.shields.io/crates/v/normy?style=flat-square" alt="Crates.io">
  </a>
  <a href="https://docs.rs/normy">
    <img src="https://img.shields.io/docsrs/normy?style=flat-square" alt="Docs.rs">
  </a>
  <a href="https://github.com/okayasl/normy/actions/workflows/ci.yml">
    <img src="https://github.com/okayasl/normy/actions/workflows/ci.yml/badge.svg?branch=main" alt="Build Status">
  </a>
  <a href="LICENSE">
    <img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=flat-square" alt="License: MIT OR Apache-2.0">
  </a>
</p>

# 📦 Normy

**Ultra-fast, zero-copy text normalization — built for Rust NLP pipelines & tokenizers**.
Flexible enough for any high-throughput multilingual text processing (search, logs, APIs, data pipelines, …)

Normy delivers **extreme performance** through automatic iterator fusion and precise early-exit checks, while respecting language-specific rules (e.g., Turkish dotted/dotless I, German ß folding).

- **Zero-copy** → Immediately returns without allocation when input needs no changes.
- **Automatic fusion** → Can fuse eligible stages (>1 fusable stage) into a single pass for better cache locality.
- **Locale-accurate** → Built-in rules for correctness across scripts.
- **Format-aware** → Clean HTML/Markdown while preserving content.

## Why Normy?

Traditional normalizers allocate on every call—even for clean text. Normy eliminates this overhead:

- On **already-normalized text** (common in production streams): **up to 51× higher throughput** than HuggingFace `tokenizers` normalizers due to true zero-copy.
- On **text requiring transformation**: **3.7–4.1× faster** through fusion and optimized stages.

## 🏆 Performance Comparison

Measured against HuggingFace `tokenizers` normalizers on 64 KiB inputs (200 samples each).

### Complex Pipeline Bert-like (Chinese + Strip + Whitespace + NFD + Diacritics + Lowercase)

Already Normalized Text

![Complex Normalized](https://quickchart.io/chart?c={type:%27horizontalBar%27,data:{labels:[%27Normy%27,%27HuggingFace%27],datasets:[{data:[19.3,1],backgroundColor:[%27rgba(75,192,75,0.8)%27,%27rgba(200,200,200,0.8)%27]}]},options:{legend:{display:false},scales:{xAxes:[{ticks:{beginAtZero:true,max:20,fontSize:9}}],yAxes:[{ticks:{fontSize:9}}]},title:{display:true,text:%2719.3x%20faster%20(100%25%20zero-copy)%27,fontSize:10}}}&width=300&height=80)

Needs Transform

![Complex Transform](https://quickchart.io/chart?c={type:%27horizontalBar%27,data:{labels:[%27Normy%27,%27HuggingFace%27],datasets:[{data:[3.7,1],backgroundColor:[%27rgba(75,192,75,0.8)%27,%27rgba(200,200,200,0.8)%27]}]},options:{legend:{display:false},scales:{xAxes:[{ticks:{beginAtZero:true,max:4,fontSize:9}}],yAxes:[{ticks:{fontSize:9}}]},title:{display:true,text:%273.7x%20faster%27,fontSize:10}}}&width=300&height=80)

### Simple Pipeline (French + Lowercase + Transliterate)

Already Normalized Text

![Simple Normalized](https://quickchart.io/chart?c={type:%27horizontalBar%27,data:{labels:[%27Normy%27,%27HuggingFace%27],datasets:[{data:[51.3,1],backgroundColor:[%27rgba(75,192,75,0.8)%27,%27rgba(200,200,200,0.8)%27]}]},options:{legend:{display:false},scales:{xAxes:[{ticks:{beginAtZero:true,max:55,fontSize:9}}],yAxes:[{ticks:{fontSize:9}}]},title:{display:true,text:%2751.3x%20faster%20(100%25%20zero-copy)%27,fontSize:10}}}&width=300&height=80)

With Accents/Diacritics

![Simple Accents](https://quickchart.io/chart?c={type:%27horizontalBar%27,data:{labels:[%27Normy%27,%27HuggingFace%27],datasets:[{data:[4.1,1],backgroundColor:[%27rgba(75,192,75,0.8)%27,%27rgba(200,200,200,0.8)%27]}]},options:{legend:{display:false},scales:{xAxes:[{ticks:{beginAtZero:true,max:5,fontSize:9}}],yAxes:[{ticks:{fontSize:9}}]},title:{display:true,text:%274.1x%20faster%27,fontSize:10}}}&width=300&height=80)

Numbers represent geometric mean over 200 samples. Hardware, OS, and input distribution can affect results.
See **/benches/comparison_tokenizers_bench.rs** for reproducible results.

## 💾 Installation

Add Normy to your project:

```bash
cargo add normy
```

## ⚡ Quickstart

Normy uses a **fluent builder** pattern with automatic fusion detection.

```rust
use std::error::Error;

use normy::{
    COLLAPSE_WHITESPACE_UNICODE, CaseFold, DEU, FRA, JPN, LowerCase, Normy, RemoveDiacritics, SegmentWords,
    TUR, Transliterate, UnifyWidth, ZHO,
};

fn main() -> Result<(), Box<dyn Error>> {
    // ────────────────────────────────────────────────────────────────
    // TURKISH (Turkic) – famous for its dotted/dotless I distinction
    // ────────────────────────────────────────────────────────────────
    let tur = Normy::builder()
        .lang(TUR)
        .add_stage(LowerCase) // Critical: İ → i, I → ı
        .build();

    println!(
        "Turkish : {}",
        tur.normalize("KIZILIRMAK NEHRİ TÜRKİYE'NİN EN UZUN NEHRİDİR.")?
    );
    // → kızılırmak nehri türkiye'nin en uzun nehridir.

    // ────────────────────────────────────────────────────────────────
    // GERMAN (Germany/Austria/Switzerland) – ß and umlaut handling
    // ────────────────────────────────────────────────────────────────
    let deu = Normy::builder()
        .lang(DEU)
        .add_stage(CaseFold) // ß → ss
        .add_stage(Transliterate) // Ä → ae, Ö → oe, Ü → ue
        .build();

    println!(
        "German  : {}",
        deu.normalize("Grüße aus München! Die Straße ist sehr schön.")?
    );
    // → gruesse aus muenchen! die strasse ist sehr schoen.

    // ────────────────────────────────────────────────────────────────
    // FRENCH (France/Belgium/Canada/etc.) – classic accented text
    // ────────────────────────────────────────────────────────────────
    let fra = Normy::builder()
        .lang(FRA)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics) // é → e, ç → c, etc.
        .build();

    println!(
        "French  : {}",
        fra.normalize("Bonjour ! J'adore le café et les croissants à Paris.")?
    );
    // → bonjour ! j'adore le cafe et les croissants a paris.

    // ────────────────────────────────────────────────────────────────
    // CHINESE (Simplified – China) – fullwidth & word segmentation
    // ────────────────────────────────────────────────────────────────
    let zho = Normy::builder()
        .lang(ZHO)
        .add_stage(UnifyWidth)
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .add_stage(SegmentWords) // unigram segmentation
        .build();

    println!(
        "Chinese : {}",
        zho.normalize("北京的秋天特别美丽，长城非常壮观！")?
    );
    // → 北京的秋天特别美丽 , 长城非常壮观 !

    // ────────────────────────────────────────────────────────────────
    // CHINESE (Simplified – China) – fullwidth & word segmentation & unigram cjk
    // ────────────────────────────────────────────────────────────────
    let zho = Normy::builder()
        .lang(ZHO)
        .modify_lang(|le| le.set_unigram_cjk(true))
        .add_stage(UnifyWidth)
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .add_stage(SegmentWords) // unigram segmentation
        .build();

    println!(
        "Chinese(unigram cjk) : {}",
        zho.normalize("北京的秋天特别美丽，长城非常壮观！")?
    );
    // → 北 京 的 秋 天 特 别 美 丽 , 长 城 非 常 壮 观 !

    // ────────────────────────────────────────────────────────────────
    // JAPANESE (Japan) – script transitions + width unification
    // ────────────────────────────────────────────────────────────────
    let jpn = Normy::builder()
        .lang(JPN)
        .add_stage(UnifyWidth)
        .add_stage(COLLAPSE_WHITESPACE_UNICODE)
        .add_stage(SegmentWords) // script boundary segmentation
        .build();

    println!(
        "Japanese: {}",
        jpn.normalize("東京は本当に素晴らしい街です！桜がとてもきれい。")?
    );
    // → 東京は本当に素晴らしい街です ! 桜がとてもきれい 。

    Ok(())
}
```

When text is already normalized, Normy returns `Cow::Borrowed` — **zero allocation**.

## ✨ Features

| Feature                  | Description                                                           |
| ------------------------ | --------------------------------------------------------------------- |
| **Zero-Copy**            | No allocation on clean input                                          |
| **Iterator Fusion**      | Automatic speedup on 2+ fusable stages (monomorphized char iterators) |
| **Locale-Accurate**      | Turkish İ/i, German ß→ss, Dutch Ĳ→ij, Arabic/Hebrew diacritics, etc.  |
| **Format-Aware**         | Safe HTML/Markdown stripping (preserves `<code>`, fences, attributes) |
| **Composable Pipelines** | Fluent builder + dynamic runtime stages                               |
| **Segmentation**         | Word boundaries for CJK, Indic, Thai, Khmer, etc. (ZWSP insertion)    |
| **Extensible**           | Implement custom transformation stage                                 |

## 💼 Available Normalization Stages

Normy provides a rich set of composable, high-performance normalization stages.  
Most stages support **static iterator fusion** for maximum speed (single-pass, zero-copy when possible).

| Stage                           | Description                                                                | Fusion Support |
| ------------------------------- | -------------------------------------------------------------------------- | -------------- |
| `CaseFold`                      | Locale-aware case folding (German ß→ss, etc.)                              | Yes            |
| `LowerCase`                     | Locale-aware lowercasing (Turkish İ→i)                                     | Yes            |
| `RemoveDiacritics`              | Removes combining/spacing diacritics (accents, tone marks, etc.)           | Yes            |
| `Transliterate`                 | Language-specific character substitutions (Ä→ae, Ю→ju, etc.)               | Yes            |
| `NormalizePunctuation`          | Normalizes dashes, quotes, ellipsis, bullets, etc. to standard forms       | Yes            |
| `UnifyWidth`                    | Converts fullwidth → halfwidth (critical for CJK compatibility)            | Yes            |
| `SegmentWords`                  | Inserts spaces at word/script boundaries (CJK unigram, Indic virama, etc.) | Yes            |
| `StripControlChars`             | Removes all control characters (Unicode Cc category)                       | Yes            |
| `StripFormatControls`           | Removes directional marks, joiners, ZWSP, invisible operators, etc.        | Yes            |
| **Whitespace Variants**         |                                                                            |                |
| • `COLLAPSE_WHITESPACE`         | Collapse consecutive ASCII whitespace → single space                       | Yes            |
| • `COLLAPSE_WHITESPACE_UNICODE` | Collapse all Unicode whitespace → single space                             | Yes            |
| • `NORMALIZE_WHITESPACE_FULL`   | Normalize + collapse + trim all Unicode whitespace                         | Yes            |
| • `TRIM_WHITESPACE`             | Trim leading/trailing ASCII whitespace only                                | Yes            |
| • `TRIM_WHITESPACE_UNICODE`     | Trim leading/trailing Unicode whitespace                                   | Yes            |
| **Normalization Forms**         |                                                                            |                |
| • `NFC`                         | Unicode canonical composed form (most compact, W3C recommended)            | **No**         |
| • `NFD`                         | Unicode canonical decomposed form                                          | **No**         |
| • `NFKC`                        | Unicode compatibility composed (lossy, e.g. ﬁ→fi, ℃→°C)                    | **No**         |
| • `NFKD`                        | Unicode compatibility decomposed                                           | **No**         |
| `StripHtml`                     | Strips HTML tags and decodes entities (format-aware)                       | **No**         |
| `StripMarkdown`                 | Removes Markdown formatting while preserving content                       | **No**         |

Key notes

- **Fusion** = static single-pass iterator fusion (zero-copy + minimal allocation when conditions met)
- Non-fusable stages (`NFC`/`NFD`/`NFKC`/`NFKD`, `StripHtml`, `StripMarkdown`) use optimized batch processing and should usually be placed early in the pipeline

## 🆎 Supported Languages

| Language           | Code  | Special Features                        |
| ------------------ | ----- | --------------------------------------- |
| **European**       |       |                                         |
| Turkish            | `TUR` | Custom case rules (İ/i, I/ı)            |
| German             | `DEU` | ß folding, umlauts transliteration      |
| Dutch              | `NLD` | IJ digraph folding                      |
| Danish             | `DAN` | Å/Æ/Ø transliteration                   |
| Norwegian          | `NOR` | Å/Æ/Ø transliteration                   |
| Swedish            | `SWE` | Å/Ä/Ö transliteration                   |
| Icelandic          | `ISL` | Þ/Ð/Æ transliteration                   |
| French             | `FRA` | Œ/Æ ligatures, accent handling          |
| Spanish            | `SPA` | Accent normalization                    |
| Portuguese         | `POR` | Comprehensive diacritics                |
| Italian            | `ITA` | Grave/acute accents                     |
| Catalan            | `CAT` | Ç transliteration                       |
| Czech              | `CES` | Háček preservation, selective stripping |
| Slovak             | `SLK` | Caron handling                          |
| Polish             | `POL` | Ogonek & acute accents                  |
| Croatian           | `HRV` | Digraph normalization                   |
| Serbian            | `SRP` | Cyrillic diacritics                     |
| Lithuanian         | `LIT` | Dot-above vowels                        |
| Greek              | `ELL` | Polytonic diacritics (6 types)          |
| Russian            | `RUS` | Cyrillic→Latin transliteration          |
| **Middle Eastern** |       |                                         |
| Arabic             | `ARA` | 15 diacritic types (tashkeel)           |
| Hebrew             | `HEB` | 20 vowel points (nikud)                 |
| **Asian**          |       |                                         |
| Vietnamese         | `VIE` | Tone marks (5 tones × vowels)           |
| Chinese            | `ZHO` | Word segmentation(Optional CJK unigram) |
| Japanese           | `JPN` | Word segmentation                       |
| Korean             | `KOR` | Word segmentation                       |
| Thai               | `THA` | Tone marks, word segmentation           |
| Lao                | `LAO` | 15 combining marks, segmentation        |
| Khmer              | `KHM` | 30+ combining marks, segmentation       |
| Myanmar            | `MYA` | 17 combining marks, segmentation        |
| **South Asian**    |       |                                         |
| Hindi              | `HIN` | Devanagari diacritics, segmentation     |
| Bengali            | `BEN` | Bengali diacritics, segmentation        |
| Tamil              | `TAM` | Tamil diacritics, segmentation          |
| **Other**          |       |                                         |
| English            | `ENG` | Default/baseline                        |

**Features Key:**

- **Word Segmentation**: Automatic boundary detection for non-space-delimited scripts
- **CJK Unigram**: Optinal character-level tokenization for Chinese ideographs
- **Transliteration**: Script→Latin conversion (e.g., Cyrillic, ligatures)
- **Diacritics**: Intelligent spacing/combining mark handling

## 📖 Documentation

- Full API docs: [docs.rs/normy](https://docs.rs/normy)
- Linguistic rules: `LINGUISTIC_POLICY.md`
- Pipeline guidelines: `PIPELINE_GUIDELINES.md`
- Examples are in the `examples/` directory
- Generate local docs:

```bash
cargo doc --open
```

---

## 🤝 Contributing

Contributions are very welcome! See `CONTRIBUTING.md`

---

## 📜 License

Dual-licensed under **MIT** or **Apache-2.0**, at your option.

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

---

**Normy** — Ultra-fast, linguistically correct normalization — the next-generation layer for Rust NLP & tokenizers
