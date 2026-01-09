# Normy Pipeline Guidelines

Normy is deliberately **composable** — you own the pipeline order and composition. There is no enforced default; design for your domain.

**⚠️ IMPORTANT**: Proper pipeline design is critical for both correctness and performance. Well-designed pipelines can be 6-42% faster through automatic fusion optimization, while poorly designed ones may produce incorrect results or degrade performance.

## Pipeline Design Principles

### 1. Avoid Redundant Stages

❌ BAD: Redundant operations

```rust
.add_stage(LowerCase)    // Redundant!
.add_stage(CaseFold)     // CaseFold already includes lowercase
```

✅ GOOD: Non-redundant pipeline

```rust
.add_stage(CaseFold)           // Handles case normalization
.add_stage(RemoveDiacritics)   // Then strip diacritics
```

**Why it matters**:

- Redundant stages waste CPU cycles
- Prevents fusion optimization when it could help
- Example: Vietnamese text with redundant stages is ~20% slower

### 2. Order Stages by Dependency

❌ BAD: Wrong order

```rust
.add_stage(RemoveDiacritics)  // Tries to remove é, à, etc.
.add_stage(Transliterate)     // Then transliterates œ -> oe
// Problem: Some transliterations produce characters with diacritics!
```

✅ GOOD: Correct order

```rust
.add_stage(CaseFold)           // Normalize case first
.add_stage(Transliterate)      // Then transliterate (may produce diacritics)
.add_stage(RemoveDiacritics)   // Finally remove all diacritics
```

**Why it matters**:

- Incorrect ordering produces wrong output
- May miss transformations that later stages would create
- Can create unintended character sequences

### 3. Use Language-Appropriate Stages

❌ BAD: Generic approach

```rust
// Same pipeline for all languages
Normy::builder()
    .lang(lang)  // Language ignored in design
    .add_stage(CaseFold)
    .add_stage(RemoveDiacritics)
```

✅ GOOD: Language-aware design

```rust
// Turkish: Special İ/I handling
Normy::builder()
    .lang(TUR)
    .add_stage(CaseFold)         // İ->i, I->ı (Turkish rules)
    .add_stage(RemoveDiacritics)

// Russian: Cyrillic transliteration
Normy::builder()
    .lang(RUS)
    .add_stage(CaseFold)
    .add_stage(Transliterate)    // Cyrillic -> Latin
    
// Vietnamese: Heavy diacritics
Normy::builder()
    .lang(VIE)
    .add_stage(CaseFold)
    .add_stage(RemoveDiacritics) // 80+ diacritic mappings
```

**Why it matters**:

- Language-specific rules affect correctness
- Some optimizations only work with language context
- Example: Turkish İ->i vs standard I->i

## General Ordering Advice

### Stage Priority Order

1. **Format stripping** (`StripHtml`, `StripMarkdown`) — if input may contain markup
2. **Unicode normalization** (`NFC`, `NFD`, `NFKC`, `NFKD`) — for canonical representation
3. **Width normalization** (`UnifyWidth`) — for CJK text
4. **Case operations** (`CaseFold`) — applies locale-specific rules
5. **Transliteration** (`Transliterate`) — language-specific character substitutions
6. **Diacritic removal** (`RemoveDiacritics`) — should come after transliteration
7. **Punctuation normalization** (`NormalizePunctuation`)
8. **Control character removal** (`StripControlChars`)
9. **Whitespace normalization** (`COLLAPSE_WHITESPACE_UNICODE`, etc.)
10. **Segmentation** (`SegmentWords`) — always last

### Why This Specific Order? (Linguistic Conflict Resolution)

The ordering above is not arbitrary—it resolves potential conflicts between transformations in a linguistically principled way:

- **NFC first**: Ensures canonical composed form (W3C/WHATWG standard); most compact and enables efficient comparison.
- **Case operations before folding**: Locale-specific rules (e.g., Turkish dotted/dotless I) must be applied before any search-equivalence folding.
- **Folding before diacritic stripping**: Preserves more semantic information (e.g., German `ß → "ss"` is better than `ß → s`).
- **Transliteration before diacritic removal**: Transliteration is the most lossy letter-level transform and takes highest priority (e.g., German `ö → "oe"` overrides a hypothetical `ö → o` from accent stripping).
- **Diacritic/spacing mark removal before whitespace/segmentation**: Operates on final letter forms; whitespace and word boundaries are computed last.

This order guarantees predictable, conservative results while maximizing zero-copy opportunities.

### Detailed Stage Guidance

**Unicode Normalization (NFC/NFD/NFKC/NFKD):**

- Place early in pipeline for canonical representation
- Required for correctness in multilingual text processing
- NFC (composition) is most common for typical text normalization
- NFD (decomposition) useful before diacritic removal or transliteration
- **Performance note**: These stages use optimized batch processing (do not fuse)

**Case Operations:**

- Use `CaseFold` for search/comparison (includes lowercase + folding)
- Use `LowerCase` only when you specifically need just lowercase
- Must come before folding for locale-specific rules (e.g., Turkish dotted İ/I)

**Lossy Transformations:**

- Order matters: `Transliterate` can produce characters with diacritics
- Always place `RemoveDiacritics` after `Transliterate` if using both
- Note: `Transliterate` overrides `precomposed_to_base` on overlapping characters
- See `LINGUISTIC_POLICY.md` for conflict resolution rules

**Whitespace Normalization:**

- Should come before segmentation
- Choose appropriate variant:
  - `NORMALIZE_WHITESPACE_FULL` — comprehensive (collapse + trim + normalize Unicode)
  - `COLLAPSE_WHITESPACE_UNICODE` — collapse only (preserves edges)
  - `TRIM_WHITESPACE` — trim edges only

**Format-Aware Stages:**

- `StripHtml`, `StripMarkdown` should be early if input may contain markup
- These stages do not support fusion (use complex state machines)

## Performance: Fusion Optimization

Normy can automatically optimize multi-stage pipelines through **iterator fusion**, which eliminates intermediate allocations by processing text in a single pass.

### How Fusion Works

Normy offers two execution paths:

#### 1. Fusion Path (Automatic Optimization)

When conditions are met, all stages merge into a single character-by-character iterator:

```rust
// What you write:
.add_stage(CaseFold)
.add_stage(Transliterate)
.add_stage(RemoveDiacritics)

// What Normy executes (conceptually):
text.chars()
    .case_fold()
    .transliterate()
    .remove_diacritics()
    .collect()
// Single pass through string ✅
```

**Benefits:**

- ✅ Single iteration (vs N separate iterations)
- ✅ Better CPU cache locality
- ✅ Fewer intermediate allocations
- ✅ **6-42% faster** depending on pipeline complexity

#### 2. Apply Path (Sequential Processing)

Each stage processes the entire string, then passes result to next stage:

```rust
// What Normy executes when fusion unavailable:
let text = case_fold(text);        // Pass 1
let text = transliterate(text);    // Pass 2
let text = remove_diacritics(text);// Pass 3
// Multiple passes + intermediate allocations
```

### Controlling Fusion

Normy provides two execution methods:

```rust
// Automatic optimization (default) - uses fusion when supported
let result = pipeline.normalize(text)?;

// Force sequential processing
let result = pipeline.normalize_no_fusion(text)?;
```

**When to use `.normalize_no_fusion()`**:

- You've benchmarked and found it faster for your specific workload
- You need consistent behavior across library updates
- Debugging performance regressions

### Fusion Requirements

Fusion automatically activates when **all** conditions are met:

1. ✅ All stages return `supports_static_fusion() = true`
2. ✅ Pipeline has 2+ stages
3. ✅ Built with static `NormyBuilder` (not `dynamic_builder()`)

**Non-fusable stages** (use optimized batch processing instead):

- `StripHtml`, `StripMarkdown` - complex state machines
- `NFC`, `NFD`, `NFKC`, `NFKD` - batch processing is 2-6x faster

**Examples:**

```rust
// ✅ FUSION ENABLED (2 fusable stages)
Normy::builder()
    .add_stage(CaseFold)
    .add_stage(RemoveDiacritics)
    .build()

// ❌ FUSION DISABLED (NFC is non-fusable)
Normy::builder()
    .add_stage(NFC)              // ← Non-fusable
    .add_stage(CaseFold)
    .add_stage(RemoveDiacritics)
    .build()
// Result: Uses fast batch processing for NFC,
//         then sequential for remaining stages

// ❌ FUSION DISABLED (only 1 stage)
Normy::builder()
    .add_stage(CaseFold)
    .build()
// Result: Direct apply (no fusion overhead)
```

### Design for Performance

✅ DO: Build non-redundant pipelines

```rust
// Good: Each stage adds value, fusion optimizes
Normy::builder()
    .lang(RUS)
    .add_stage(CaseFold)        // 1. Lowercase
    .add_stage(Transliterate)   // 2. Cyrillic->Latin
    .build()
// Result: Faster with fusion ✅
```

❌ DON'T: Add redundant stages

```rust
// Bad: LowerCase + CaseFold is redundant
Normy::builder()
    .add_stage(LowerCase)    // ← Redundant
    .add_stage(CaseFold)     // Already does lowercase
    .build()
// Result: Wasted work, no fusion benefit
```

✅ DO: Combine transformations when possible

```rust
// Good: 4-stage pipeline gets maximum fusion benefit
Normy::builder()
    .lang(FRA)
    .add_stage(CaseFold)
    .add_stage(Transliterate)     // œ->oe
    .add_stage(NormalizePunctuation)
    .add_stage(RemoveDiacritics)
    .build()
// Result: Faster with fusion✅
```

✅ DO: Use language-specific text

```rust
// Good: Stages actually transform the text
let text = "РУССКАЯ ПИСЬМЕННОСТЬ";  // Russian Cyrillic
pipeline.normalize(text)
// Result: Every stage does work, fusion provides value
```

❌ DON'T: Use generic text for language-specific pipelines

```rust
// Bad: English text with Russian pipeline
let text = "HELLO";  // No Cyrillic to transliterate!
pipeline.normalize(text)
// Result: Stages do no work, fusion overhead wasted
```

## Zero-Copy Optimization

Both execution paths support zero-copy when text is already normalized:

```rust
let pipeline = Normy::builder()
    .add_stage(CaseFold)
    .add_stage(RemoveDiacritics)
    .build();

// Text needs changes: allocates
let result = pipeline.normalize("HELLO CAFÉ")?;  // → "hello cafe"

// Text already normalized: zero-copy!
let result = pipeline.normalize("hello cafe")?;  // Returns Cow::Borrowed
```

The `needs_apply()` check in each stage enables early-exit, making normalized input fast regardless of pipeline size.

## Example Pipelines

### Search Index Normalization

```rust
// Typical search/comparison pipeline
// Performance: faster with fusion
let pipeline = Normy::builder()
    .lang(user_lang)
    .add_stage(CaseFold)          // Normalize case (includes lowercase)
    .add_stage(Transliterate)     // Language-specific substitutions
    .add_stage(RemoveDiacritics)  // Strip accents for fuzzy matching
    .build();
```

### Web Content Normalization

```rust
// Content pipeline with format stripping
// Note: StripHtml disables fusion, but that's optimal
let pipeline = Normy::builder()
    .lang(ENG)
    .add_stage(StripHtml)                    // ← Non-fusable (correct)
    .add_stage(CaseFold)
    .add_stage(RemoveDiacritics)
    .add_stage(COLLAPSE_WHITESPACE_UNICODE)
    .build();
```

### CJK Text Normalization

```rust
// Japanese/Chinese/Korean pipeline
let pipeline = Normy::builder()
    .lang(JPN)
    .add_stage(UnifyWidth)              // Fullwidth -> halfwidth
    .add_stage(NormalizePunctuation)    // －－－ -> ---
    .add_stage(COLLAPSE_WHITESPACE_UNICODE)
    .build();
```

### Multilingual Content

```rust
// Heavy normalization for multilingual search
let pipeline = Normy::builder()
    .lang(detected_lang)
    .add_stage(NFC)                     // ← Non-fusable (optimal)
    .add_stage(CaseFold)                // ← Fusion starts here
    .add_stage(Transliterate)
    .add_stage(RemoveDiacritics)
    .add_stage(StripControlChars)
    .build();
// NFC uses fast batch processing
```

## Summary

### Design Checklist

✅ **Avoid redundant stages** (e.g., don't use LowerCase + CaseFold)  
✅ **Order stages by dependency** (Transliterate before RemoveDiacritics)  
✅ **Use language-appropriate stages** (Turkish rules for Turkish text)  
✅ **Place Unicode normalization early** (if needed for correctness)  
✅ **Test with representative text** (language-specific examples)

### Performance Characteristics

**When fusion helps (typical: 5-25% faster):**

- ✅ Multiple simple stages (2-4 stages)
- ✅ Balanced stage costs (no single stage dominates)
- ✅ Medium to large input sizes (500+ bytes)

**When fusion may not help:**

- ⚠️ Single dominant expensive stage (>80% of runtime)
- ⚠️ Very small inputs (<100 bytes)
- ⚠️ Stages with heavy per-character overhead

**Note**: Performance varies significantly by language, text characteristics,
and pipeline composition. Always benchmark with your actual workload.

### Trust the Optimizer (But Verify)

- ✅ **Do** focus on correctness and proper stage ordering
- ✅ **Do** let Normy handle fusion automatically by default
- ✅ **Do** benchmark your specific workload if performance is critical
- ⚠️ **Don't** assume fusion is always faster
- ⚠️ **Don't** avoid non-fusable stages (they use optimal algorithms)

Normy's fusion is designed to "just work" when you build sensible pipelines. Focus on correctness and stage ordering; let Normy handle the optimization.
