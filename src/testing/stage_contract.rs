use crate::{lang::Lang, stage::Stage};

/// Trait that stages implement to opt into the universal test suite.
pub trait StageTestConfig: Stage + Sized {
    /// Languages where this stage has a **pure 1:1, context-free** mapping.
    fn one_to_one_languages() -> &'static [Lang];

    /// Optional: skip idempotency test (e.g. SegmentWords)
    fn skip_idempotency() -> &'static [Lang] {
        &[]
    }

    /// Optional: custom samples per language
    fn samples(_lang: Lang) -> &'static [&'static str] {
        &["Hello World 123", " déjà-vu ", "TEST", ""]
    }

    // Add this default
    fn skip_needs_apply_test() -> bool {
        false
    }
}

/// Assert that a stage satisfies **all seven universal contracts** defined in the Normy white paper.
///
/// This macro is the **single source of truth** for stage correctness. Every stage must pass it.
/// It is deliberately exhaustive and unforgiving — because production NLP pipelines demand it.
///
/// ### The Seven Universal Contracts:
/// 1. `zero_copy_when_no_changes` → no allocation when input == output
/// 2. `fast_and_slow_paths_equivalent` → CharMapper and apply() produce identical results
/// 3. `stage_is_idempotent` → applying twice yields same result as once
/// 4. `needs_apply_is_accurate` → correctly predicts whether apply() would change text
/// 5. `handles_empty_string_and_ascii` → graceful on edge cases
/// 6. `no_panic_on_mixed_scripts` → survives pathological real-world input
/// 7. (Implicit) `Send + Sync + 'static` → required by trait bounds
///
/// Failure of any contract is a **critical bug**.
#[macro_export]
macro_rules! assert_stage_contract {
    ($stage:expr) => {
        $crate::testing::stage_contract::zero_copy_when_no_changes($stage);
        $crate::testing::stage_contract::fast_and_slow_paths_equivalent($stage);
        $crate::testing::stage_contract::stage_is_idempotent($stage);
        $crate::testing::stage_contract::needs_apply_is_accurate($stage);
        $crate::testing::stage_contract::handles_empty_string_and_ascii($stage);
        $crate::testing::stage_contract::no_panic_on_mixed_scripts($stage);
    };
}

// ============================================================================
// Universal contract tests
// ============================================================================

#[cfg(test)]
use crate::{ENG, all_langs, context::Context};

#[cfg(test)]
use std::borrow::Cow;

#[cfg(test)]
pub fn zero_copy_when_no_changes<S: StageTestConfig>(stage: S) {
    for &lang in all_langs() {
        let ctx = Context::new(lang);
        for &input in S::samples(lang) {
            // Simulate what ChainedProcess actually does:
            let mut text = Cow::Borrowed(input);
            // First pass
            if stage.needs_apply(&text, &ctx).unwrap() {
                text = stage.apply(text, &ctx).unwrap();
            }
            // Second pass — this is the real zero-copy test
            let before = text.as_ref() as *const str;
            if stage.needs_apply(&text, &ctx).unwrap() {
                text = stage.apply(text, &ctx).unwrap();
            }
            let after = text.as_ref() as *const str;
            assert_eq!(
                before, after,
                "zero-copy failed in real pipeline simulation: pointer changed despite no change needed (lang: {lang:?}, input: `{input}`)"
            );
        }
    }
}

#[cfg(test)]
pub fn fast_and_slow_paths_equivalent<S: StageTestConfig>(stage: S) {
    for &lang in S::one_to_one_languages() {
        let ctx = Context::new(lang);
        let input = "AbCdEfGhIjKlMnOpQrStUvWxYz ÀÉÎÖÜñç 123!@# テスト";
        let via_apply = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        if let Some(mapper) = stage.as_char_mapper(&ctx) {
            let via_fast = mapper.bind(input, &ctx).collect::<Cow<'_, str>>();
            assert_eq!(
                via_apply, via_fast,
                "fast ≠ slow path in {lang:?}\n   apply(): {via_apply:?}\n   mapper(): {via_fast:?}"
            );
        }
    }
}

#[cfg(test)]
pub fn stage_is_idempotent<S: StageTestConfig>(stage: S) {
    for &lang in all_langs() {
        if S::skip_idempotency().contains(&lang) {
            continue;
        }
        let ctx = Context::new(lang);
        for &input in S::samples(lang) {
            use std::borrow::Cow;

            let once = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            let twice = stage.apply(once.clone(), &ctx).unwrap();
            assert_eq!(once, twice, "not idempotent in {lang:?} on `{input}`");
        }
    }
}

// src/testing/stage_contract.rs
#[cfg(test)]
pub fn needs_apply_is_accurate<S: StageTestConfig>(stage: S) {
    // Test in *every* language that the stage claims to support.
    // Stages that are language-agnostic (whitespace, NFC, …) will just be tested once.
    let languages = if S::one_to_one_languages().is_empty() {
        all_langs()
    } else {
        S::one_to_one_languages()
    };

    for &lang in languages {
        let ctx = Context::new(lang);

        // 1. Stage-provided samples (these are the most important ones)
        for &sample in S::samples(lang) {
            check_accuracy(&stage, sample, &ctx, sample);
        }

        // 2. Explicit “must not trigger” set – only characters that the stage
        //     is guaranteed never to touch (pure ASCII without control chars)
        let must_not_touch = ["", "hello", "world123", " !@#"];
        for &clean in &must_not_touch {
            check_accuracy(&stage, clean, &ctx, clean);
        }
    }
}

#[cfg(test)]
#[inline(always)]
fn check_accuracy<S: Stage>(stage: &S, input: &str, ctx: &Context, display: &str) {
    let predicted = stage.needs_apply(input, ctx).expect("needs_apply errored");

    // NOTE: we deliberately clone the input into an Owned Cow so that
    //       stages that always allocate (e.g. NFKC) are not penalised.
    let output = stage
        .apply(Cow::Owned(input.to_owned()), ctx)
        .expect("apply errored");

    // Semantic equality – this is the only thing the pipeline cares about
    let actually_changes = output != input;

    assert_eq!(
        predicted,
        actually_changes,
        "needs_apply() mismatch for stage `{}` in {lang:?} on `{display}`\n\
         predicted: {predicted}\n\
         actual   : {actually_changes} (output = {output:?})",
        stage.name(),
        lang = ctx.lang
    );
}

#[cfg(test)]
pub fn handles_empty_string_and_ascii<S: StageTestConfig>(stage: S) {
    let ctx = Context::new(ENG);

    // Empty string must survive round-trip
    let empty: &str = "";
    let result_empty = if stage.needs_apply(empty, &ctx).unwrap() {
        stage.apply(Cow::Borrowed(empty), &ctx).unwrap()
    } else {
        Cow::Borrowed(empty)
    };
    assert_eq!(result_empty.as_ref(), "");

    // Pure ASCII must not be semantically altered
    let ascii = "hello world 123 !@#";
    let result_ascii = stage.apply(Cow::Borrowed(ascii), &ctx).unwrap();
    assert_eq!(result_ascii.as_ref(), ascii);
}

#[cfg(test)]
pub fn no_panic_on_mixed_scripts<S: StageTestConfig>(stage: S) {
    let ctx = Context::new(ENG);
    let _ = stage.apply(
        Cow::Borrowed("Hello 世界 русский Türkçe العربية 简体中文"),
        &ctx,
    );
}
