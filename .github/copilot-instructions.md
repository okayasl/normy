# Copilot & AI Agent Instructions for Normy

## Project Overview
- **Normy** is a high-performance, linguistically principled text normalization library for Rust, designed for NLP pipelines, tokenizers, and multilingual search.
- Key features: zero-copy normalization, automatic iterator fusion, locale-aware processing, and format-aware cleaning (HTML/Markdown stripping).

## Architecture & Key Patterns
- **Pipeline-based design:** Users compose normalization pipelines using a fluent builder (`Normy::builder()`). Each pipeline consists of ordered `Stage`s (see `src/stage/`).
- **Stage contract:** Every stage must implement strict contracts for correctness, zero-copy, and idempotence. See `src/testing/stage_contract.rs` and `STAGE_TESTING_DOCTRINE.md`.
- **Language-aware:** Language codes and rules are defined in `src/lang/` and governed by `LINGUISTIC_POLICY.md`.
- **Fusion optimization:** When possible, compatible stages are fused for performance (see `PIPELINE_GUIDELINES.md`).
- **No unsafe code:** The crate is `#![forbid(unsafe_code)]`.

## Developer Workflows
- **Build:** `cargo build` (no special steps)
- **Test:** `cargo test` (unit, integration, and contract tests)
- **Benchmarks:** `cargo bench` (uses Criterion; see `benches/`)
- **Docs:** `cargo doc --open` (API docs)
- **Add dependencies:** `cargo add <crate>`

## Project-Specific Conventions
- **Stage ordering is critical:** Follow the recommended order in `PIPELINE_GUIDELINES.md` for correctness and performance. Example: `StripHtml` → `NFC` → `UnifyWidth` → `CaseFold` → `Transliterate` → `RemoveDiacritics` → whitespace/punctuation → segmentation.
- **Redundant stages are discouraged:** Avoid adding both `LowerCase` and `CaseFold`.
- **Language-specific logic:** Always use `.lang(...)` in pipelines; do not assume one-size-fits-all.
- **Non-destructive by default:** Aggressive/lossy transforms (e.g., diacritic stripping) are opt-in and never default.
- **Testing:** All new stages must pass the seven universal contracts (see `assert_stage_contract!` macro).

## Key Files & Directories
- `src/lib.rs`: Crate exports and stage registration
- `src/stage/`: All normalization stages (modular, flat)
- `src/lang/`: Language codes, rules, and data
- `PIPELINE_GUIDELINES.md`: Rationale and best practices for pipeline design
- `LINGUISTIC_POLICY.md`: Linguistic rules and language-specific behaviors
- `STAGE_TESTING_DOCTRINE.md`: Stage contract and testing philosophy
- `benches/`: Criterion benchmarks for performance
- `examples/`: Usage examples (see also README)

## Integration & Extensibility
- **Custom stages:** Implement the `Stage` trait and add to pipelines.
- **Language data:** Extend `src/lang/data.rs` for new languages or rules.
- **External use:** Normy is designed for easy integration into other Rust NLP projects.

## Example: Pipeline Construction
```rust
let pipeline = Normy::builder()
    .lang(DEU)
    .add_stage(StripHtml)
    .add_stage(CaseFold)
    .add_stage(Transliterate)
    .add_stage(RemoveDiacritics)
    .add_stage(CollapseWhitespaceUnicode)
    .build();
```

## References
- See `README.md` for quickstart and feature overview.
- See `PIPELINE_GUIDELINES.md` and `LINGUISTIC_POLICY.md` for in-depth rationale.
- All code must uphold the contracts and philosophy described in `STAGE_TESTING_DOCTRINE.md`.
