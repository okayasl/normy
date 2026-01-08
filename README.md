# Normy

**Next-Generation Text Normalization for Rust**

`Normy` is a **fast**, **accurate**, and **production-ready** normalization layer library for Rust, specifically designed for NLP pipelines, tokenizers, search indexing, and multilingual text processing.

It excels at **zero-copy** operations, **automatic iterator fusion** for 30-40% speedups on multi-stage pipelines, **locale-specific linguistic accuracy**, and **format-aware** cleaning (HTML/Markdown stripping without corrupting code).

---

## Table of Contents

- [Normy](#normy)
  - [Table of Contents](#table-of-contents)
  - [Installation](#installation)
  - [Quickstart](#quickstart)
  - [Features](#features)
  - [Pipeline Design](#pipeline-design)
  - [Examples](#examples)
    - [Search Normalization (Multilingual)](#search-normalization-multilingual)
    - [CJK Pipeline](#cjk-pipeline)
  - [Benchmarks](#benchmarks)
  - [Documentation](#documentation)
  - [Contributing](#contributing)
  - [License](#license)

---

## Installation

Add Normy to your project:

```bash
cargo add normy
```

## Quickstart

Normy uses a **fluent builder** pattern with automatic fusion detection.

```rust
use normy::{Normy, stage::*};
use normy::lang::{TUR, DEU, ENG};

fn main() -> Result<(), normy::NormyError> {
    // Turkish: locale-specific case folding (İ → i, I → ı)
    let turkish = Normy::builder()
        .lang(TUR)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .build();

    assert_eq!(turkish.normalize("İstanbul")?, "istanbul");

    // German: folding + transliteration + diacritics
    let german = Normy::builder()
        .lang(DEU)
        .add_stage(CaseFold)
        .add_stage(Transliterate)
        .add_stage(RemoveDiacritics)
        .build();

    assert_eq!(german.normalize("Größe")?, "groesse");

    // Web content: strip HTML + normalize
    let web = Normy::builder()
        .lang(ENG)
        .add_stage(StripHtml)
        .add_stage(CaseFold)
        .add_stage(CollapseWhitespaceUnicode)
        .build();

    let html = "<p>Hello <b>World</b> café!</p>";
    assert_eq!(web.normalize(html)?, "hello world cafe!");

    Ok(())
}
```

When text is already normalized, Normy returns `Cow::Borrowed` — **zero allocation**.

---

## Features

| Feature                        | Description                                                                 |
| ------------------------------ | --------------------------------------------------------------------------- |
| **Zero-Copy**                  | Early-exit via `needs_apply()` → no allocation on clean input               |
| **Iterator Fusion**            | Automatic 30-40% speedup on 3+ fusable stages (monomorphized char iterators)|
| **Locale-Accurate**            | Turkish İ/i, German ß→ss, Dutch Ĳ→ij, Arabic/Hebrew diacritics, etc.        |
| **Format-Aware**               | Safe HTML/Markdown stripping (preserves `<code>`, fences, attributes)       |
| **Composable Pipelines**       | Fluent builder + dynamic runtime stages                                     |
| **Linguistically Conservative**| Non-destructive by default; aggressive ASCII in optional crates only        |
| **Unicode NFC First**          | Always canonical composed form (W3C standard)                               |
| **Segmentation**               | Word boundaries for CJK, Indic, Thai, Khmer, etc. (ZWSP insertion)          |
| **Extensible**                 | Implement `Stage` trait for custom transformations                           |

---

## Pipeline Design

Correct stage order is crucial for both **correctness** and **performance**.

Recommended priority:

1. Format stripping (`StripHtml`, `StripMarkdown`)
2. Unicode normalization (`NFC`)
3. Width unification (`UnifyWidth`)
4. Case operations (`CaseFold`)
5. Transliteration (`Transliterate`)
6. Diacritic removal (`RemoveDiacritics`)
7. Punctuation/whitespace normalization
8. Control/format control removal
9. Segmentation (`SegmentWords`) — always last

Fusion works automatically when all stages implement `StaticFusableStage`.

See `PIPELINE_GUIDELINES.md` for detailed rationale.

---

## Examples

### Search Normalization (Multilingual)

```rust
let search = Normy::builder()
    .lang(detected_lang)
    .add_stage(CaseFold)
    .add_stage(Transliterate)
    .add_stage(RemoveDiacritics)
    .add_stage(CollapseWhitespaceUnicode)
    .build();
```

### CJK Pipeline

```rust
let cjk = Normy::builder()
    .lang(JPN)  // or ZHO, KOR
    .add_stage(UnifyWidth)
    .add_stage(NormalizePunctuation)
    .add_stage(CollapseWhitespaceUnicode)
    .add_stage(SegmentWords)
    .build();
```

More examples in the `examples/` directory (coming soon).

---

## Benchmarks

Normy achieves **30-40% faster** multi-stage processing via fusion compared to naïve per-stage allocation.

- Already-normalized text: **zero-copy** regardless of pipeline length
- 4-stage pipelines: ~35% speedup with fusion
- Heavy multilingual corpus: competitive with optimized C++ equivalents

Run benchmarks locally:

```bash
cargo bench
```

See `BENCHMARKS.md` for detailed results.

---

## Documentation

* Full API docs: [docs.rs/normy](https://docs.rs/normy) (once published)
* Linguistic rules: `LINGUISTIC_POLICY.md`
* Pipeline guidelines: `PIPELINE_GUIDELINES.md`
* Generate local docs:

```bash
cargo doc --open
```

---

## Contributing

Contributions are very welcome! See `CONTRIBUTING.md` for:

* Code style (`rustfmt`, `clippy`)
* Stage contract tests (`assert_stage_contract!`)
* Adding new languages/stages

---

## License

Dual-licensed under **MIT** or **Apache-2.0**, at your option.

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

---

**Normy** — Linguistically correct, blazingly fast text normalization for modern Rust NLP.