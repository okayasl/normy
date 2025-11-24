# NORMY RECOMMENDED DEFAULT PIPELINE ORDER

## Zero-Copy Optimized • Linguistically Pure by Default • Format-Aware

This is the **official recommended default** — used by all preset profiles and proven to maximize zero-copy success.

### Official Recommended Pipeline Order

```rust
1. NFC                  → Canonical precomposed form
2. case_map             → Locale-correct lowercase
3. fold                 → Linguistic folding (ß→"ss", Dutch Ĳ→"ij")
4. precomposed_to_base (opt-in)       → Lossy accent removal — disabled by default
5. transliterate (opt-in) → Historical ASCII expansion — disabled by default
   → Overrides strip when both enabled
6. spacing_diacritics removal         → Remove standalone combining marks
7. normalize_whitespace → Must precede segment
8. segment              → Insert word boundaries
