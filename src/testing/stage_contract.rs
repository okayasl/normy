use crate::{lang::Lang, stage::Stage};

/// Trait that stages implement to opt into the universal test suite.
/// Trait that stages implement to opt into the universal test suite.
pub trait StageTestConfig: Stage + Sized {
    /// Languages where this stage has a **pure 1:1, context-free** mapping.
    fn one_to_one_languages() -> &'static [Lang];

    /// Optional: skip idempotency test (e.g. SegmentWords)
    fn skip_idempotency() -> &'static [Lang] {
        &[]
    }

    /// General test samples (may or may not trigger changes)
    fn samples(_lang: Lang) -> &'static [&'static str] {
        &["Hello World 123", " déjà-vu ", "TEST", ""]
    }

    /// Samples that should pass through unchanged (zero-copy test).
    /// These test the zero-copy guarantee when no transformation is needed.
    ///
    /// Default: common ASCII patterns that most stages should pass through unchanged.
    fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
        &[
            "hello",   // Simple lowercase
            "world",   // Another simple word
            "test123", // Alphanumeric
            "abc def", // Simple phrase with space
            "",        // Empty string
        ]
    }

    /// Input/output pairs that verify correct transformations.
    /// These test that the stage produces expected output for known inputs.
    ///
    /// Return empty slice if stage doesn't have predictable transformations.
    fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    /// Skip the needs_apply accuracy test (for stages where it's complex to predict)
    fn skip_needs_apply_test() -> bool {
        false
    }

    /// Skip the needs_apply accuracy test (for stages where it's complex to predict)
    fn skip_zero_copy_apply_test() -> bool {
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
    use crate::{all_langs, context::Context};
    use std::borrow::Cow;

    if S::skip_zero_copy_apply_test() {
        return;
    }

    for &lang in all_langs() {
        let ctx = Context::new(lang);

        // TEST 1: General samples - test whatever happens
        for &input in S::samples(lang) {
            let before_ptr = input as *const str;
            let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

            if result == input {
                let after_ptr = result.as_ref() as *const str;
                assert_eq!(
                    before_ptr, after_ptr,
                    "zero-copy violated: stage claimed to not change text but allocated anyway \
                     (lang: {lang:?}, input: `{input}`)\n\
                     This means apply() returned Cow::Owned even though output == input"
                );
            }

            // Pipeline simulation
            let mut text = Cow::Borrowed(input);
            if stage.needs_apply(&text, &ctx).unwrap() {
                text = stage.apply(text, &ctx).unwrap();
            }
            let before = text.as_ref() as *const str;
            if stage.needs_apply(&text, &ctx).unwrap() {
                text = stage.apply(text, &ctx).unwrap();
            }
            let after = text.as_ref() as *const str;
            assert_eq!(
                before, after,
                "zero-copy failed in pipeline simulation: pointer changed on idempotent pass \
                 (lang: {lang:?}, input: `{input}`)"
            );
        }

        // TEST 2: Pass-through samples - MUST be zero-copy
        for &pass_through_input in S::should_pass_through(lang) {
            let before_ptr = pass_through_input as *const str;
            let result = stage
                .apply(Cow::Borrowed(pass_through_input), &ctx)
                .unwrap();

            assert_eq!(
                result.as_ref(),
                pass_through_input,
                "Stage '{}' modified pass-through sample (lang: {lang:?}, input: `{pass_through_input}`)\n\
                 Expected: no change\n\
                 Got: {:?}",
                stage.name(),
                result.as_ref()
            );

            let after_ptr = result.as_ref() as *const str;
            assert_eq!(
                before_ptr,
                after_ptr,
                "zero-copy violated on pass-through sample (lang: {lang:?}, input: `{pass_through_input}`)\n\
                 Stage: {}\n\
                 The stage correctly didn't change the text, but allocated anyway!",
                stage.name()
            );
        }

        // TEST 3: Transformation samples - verify correctness
        for &(input, expected) in S::should_transform(lang) {
            let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            assert_eq!(
                result.as_ref(),
                expected,
                "Stage '{}' produced incorrect transformation (lang: {lang:?})\n\
                 Input:    `{input}`\n\
                 Expected: `{expected}`\n\
                 Got:      `{}`",
                stage.name(),
                result.as_ref()
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

#[cfg(test)]
pub fn needs_apply_is_accurate<S: StageTestConfig>(stage: S) {
    if S::skip_needs_apply_test() {
        return;
    }
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
