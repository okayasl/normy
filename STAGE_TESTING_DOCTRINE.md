# Normy Stage Testing Doctrine

*The Immutable Law of Correctness, Performance, and Debuggability

## Core Philosophy

> **The pipeline guarantees zero-copy. The stage guarantees correctness.**
>
> **Never duplicate `needs_apply()` checks inside `apply()`.**
>
> **Never sacrifice architectural purity for a test.**

---

## The Seven Universal Contracts (Never Break)

Every stage **must** pass these 6 universal tests via `StageTestConfig`.

| Contract                          | Meaning                                                                                   | Enforced By                          |
|----------------------------------|--------------------------------------------------------------------------------------------|---------------------------------------|
| `zero_copy_when_no_changes`      | If `needs_apply()` returns `false`, second pass **must not** reallocate (pointer equality) | **Pipeline simulation** (real behavior) |
| `fast_and_slow_paths_equivalent` | `CharMapper::bind()` must produce **identical** output to `apply()`                      | Only on `one_to_one_languages()`      |
| `stage_is_idempotent`            | Applying twice = applying once (unless explicitly skipped)                                | All languages                         |
| `needs_apply_is_accurate`        | Must detect real case changes, ignore whitespace/punctuation                              | English-only, focused samples         |
| `handles_empty_string_and_ascii` | Empty string and pure ASCII must survive round-trip                                       | Pipeline-aware                        |
| `no_panic_on_mixed_scripts`      | Must not panic on any valid UTF-8, any script combination                                 | Real-world robustness                 |

These are **not suggestions**. They are **law**.

---

## The One Language Per Test Rule (Non-Negotiable)

```rust
// YES — Surgical, debuggable, correct
#[test] fn case_fold_turkish_dotted_capital_i() { ... }
#[test] fn case_fold_dutch_ij_ligature() { ... }
#[test] fn case_fold_german_eszett_capital() { ... }

// NO — Vague, dangerous, forbidden
#[test] fn case_fold_all_languages() { ... }  // Will be rejected in review
