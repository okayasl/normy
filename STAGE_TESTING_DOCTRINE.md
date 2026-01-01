# Normy Stage Testing Doctrine

The Immutable Law of Correctness, Performance, and Debuggability

## Core Philosophy

> **The pipeline guarantees zero-copy. The stage guarantees correctness.**
>
> **Never duplicate `needs_apply()` checks inside `apply()`.**
>
> **Never sacrifice architectural purity for a test.**

---

## The Six Universal Contracts (Never Break)

Every stage **must** pass these six universal tests via `StageTestConfig`.

| Contract                         | Meaning                                                                                                | Enforced By                               |
| -------------------------------- | ------------------------------------------------------------------------------------------------------ | ----------------------------------------- |
| `zero_copy_when_no_changes`      | If `needs_apply()` returns `false`, subsequent passes **must not** reallocate (pointer equality)       | Pipeline simulation (real behavior)       |
| `fused_path_equivalent_to_apply` | `StaticFusableStage::static_fused_adapter()` must produce **identical** output to `apply()`            | Only on `one_to_one_languages()`          |
| `stage_is_idempotent`            | Applying twice = applying once (unless explicitly skipped)                                             | All languages                             |
| `needs_apply_is_accurate`        | Must correctly predict whether `apply()` would change text; tested exhaustively on supported languages | `one_to_one_languages()` or `all_langs()` |
| `handles_empty_string_and_ascii` | Empty string and pure ASCII must survive round-trip unchanged                                          | Pipeline-aware                            |
| `no_panic_on_mixed_scripts`      | Must not panic on any valid UTF-8, any script combination                                              | Real-world robustness                     |

These are **not suggestions**. They are **law**.

The implicit seventh contract — `Send + Sync + 'static` — is enforced by the `Stage` trait bounds.

---

## Implementation Note

These contracts are mechanically enforced by the `assert_stage_contract!` macro and the test functions in `src/testing/stage_contract.rs`. The doctrine explains **why** they exist; the code shows **how** they are verified.

---
