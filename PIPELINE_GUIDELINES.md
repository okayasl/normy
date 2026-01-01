# Normy Pipeline Guidelines

Normy is deliberately **composable** — you own the pipeline order and composition. There is no enforced default; design for your domain.

## General Ordering Advice

- **NFC first** for canonical composition (required for correctness in most multilingual cases).
- **Case/lowercase** before folding — locale-specific rules (e.g., Turkish dotted I) must apply first.
- **Lossy transformations** (`precomposed_to_base`, `transliterate`) in your preferred order.  
  Note: `transliterate` overrides `precomposed_to_base` on overlapping characters (see `LINGUISTIC_POLICY.md`).
- **Whitespace normalization** before segmentation.
- **Format-aware stages** (`strip_html`, `strip_markdown`) early if input may contain markup.

Detailed linguistic conflict resolution rules are in `LINGUISTIC_POLICY.md`.

## Static Fusion and Zero-Copy Performance

Normy's fastest path (`process_fused`) is only available when:

- All stages implement `StaticFusableStage` **and** `supports_static_fusion()` returns `true`.
- The chain is built with the **static** `NormyBuilder` (monomorphized `ChainedProcess`).

Currently, **only two stages are not statically fusable**:

- `StripHtml`
- `StripMarkdown`

These stages perform complex parsing/state machines that cannot be expressed as pure `char → char` adapters.

### Maximizing Fusion

To retain static fusion for the bulk of your pipeline:

1. Run format-stripping in a **separate pre-pass**:

```rust
   let cleaned = Normy::dynamic_builder()
       .add_stage(StripHtml)
       .add_stage(StripMarkdown)
       .build()
       .normalize(text)?;

   let normalized = Normy::builder()
       .add_stage(Nfc)
       .add_stage(LowerCase)
       .add_stage(CaseFold)
       // ... other fusable stages
       .build()
       .normalize(&cleaned)?;
