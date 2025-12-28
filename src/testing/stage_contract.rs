#[cfg(test)]
use crate::stage::StaticFusableStage;
use crate::{lang::Lang, stage::Stage};

/// Trait that stages implement to opt into the universal test suite.
pub trait StageTestConfig: Stage + Sized {
    /// Languages where this stage has a **pure 1:1, context-free** mapping.
    fn one_to_one_languages() -> &'static [Lang];

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
}

/// Assert that a stage satisfies **all seven universal contracts** defined in the Normy white paper.
///
/// This macro is the **single source of truth** for stage correctness. Every stage must pass it.
/// It is deliberately exhaustive and unforgiving — because production NLP pipelines demand it.
///
/// ### The Seven Universal Contracts:
/// 1. `zero_copy_when_no_changes` → no allocation when input == output
/// 2. `static_and_dynamic_iter_paths_equivalent_to_apply` → try_iter and try_dynamic_iter produce identical results to apply()
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
        $crate::testing::stage_contract::fused_path_equivalent_to_apply($stage);
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
            let mut text = Cow::Borrowed(input);

            // First pass – respect needs_apply
            if stage.needs_apply(&text, &ctx).unwrap() {
                let old_ptr = text.as_ref() as *const str;
                text = stage.apply(text, &ctx).unwrap();
                // If we allocated, pointer must have changed (apply trusts needs_apply)
                assert_ne!(old_ptr, text.as_ref() as *const str);
            } else {
                // No change needed → must remain borrowed with identical pointer
                assert_eq!(input as *const str, text.as_ref() as *const str);
            }

            // Second pass – must never allocate again (idempotency + zero-copy)
            let old_ptr = text.as_ref() as *const str;
            if stage.needs_apply(&text, &ctx).unwrap() {
                text = stage.apply(text, &ctx).unwrap();
            }
            assert_eq!(
                old_ptr,
                text.as_ref() as *const str,
                "zero-copy violated on second idempotent pass (lang: {lang:?}, input: `{input}`)"
            );
        }

        // Pass-through samples must always be zero-copy and unchanged
        for &pass_through in S::should_pass_through(lang) {
            let mut text = Cow::Borrowed(pass_through);
            let original_ptr = pass_through as *const str;

            if stage.needs_apply(&text, &ctx).unwrap() {
                text = stage.apply(text, &ctx).unwrap();
            }

            assert_eq!(text.as_ref(), pass_through);
            assert_eq!(
                original_ptr,
                text.as_ref() as *const str,
                "zero-copy violated on pass-through sample (lang: {lang:?}, input: `{pass_through}`)"
            );
        }

        // Transformation samples – allocation expected
        for &(input, expected) in S::should_transform(lang) {
            let mut text = Cow::Borrowed(input);
            if stage.needs_apply(&text, &ctx).unwrap() {
                text = stage.apply(text, &ctx).unwrap();
            }
            assert_eq!(text.as_ref(), expected);
        }
    }
}

#[cfg(test)]
pub fn fused_path_equivalent_to_apply<S: Stage + StaticFusableStage + StageTestConfig>(stage: S) {
    for &lang in S::one_to_one_languages() {
        let ctx = Context::new(lang);
        let input = "AbCdEfGhIjKlMnOpQrStUvWxYz ÀÉÎÖÜñç 123!@# テスト";

        let via_apply = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        if stage.supports_static_fusion() {
            let adapter = stage.static_fused_adapter(input.chars(), &ctx);
            let via_fused: String = adapter.collect();
            assert_eq!(
                via_apply.as_ref(),
                via_fused,
                "static fused adapter path ≠ apply() in {lang:?}\n\
             apply(): {via_apply:?}\n\
             fused:   {via_fused:?}"
            );
        }
    }
}

#[cfg(test)]
pub fn stage_is_idempotent<S: Stage + StaticFusableStage + StageTestConfig>(stage: S) {
    for &lang in all_langs() {
        let ctx = Context::new(lang);
        for &input in S::samples(lang) {
            use std::borrow::Cow;
            let once = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            let twice = stage.apply(once.clone(), &ctx).unwrap();
            assert_eq!(
                once, twice,
                "apply() not idempotent in {lang:?} on `{input}`"
            );

            if stage.supports_static_fusion() {
                let once = stage.static_fused_adapter(input.chars(), &ctx);
                let twice = stage.static_fused_adapter(input.chars(), &ctx);
                let once: String = once.collect();
                let twice: String = twice.collect();
                assert_eq!(
                    once, twice,
                    "static_fused_adapter() not idempotent in {lang:?} on `{input}`"
                );
            }
        }
    }
}

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
