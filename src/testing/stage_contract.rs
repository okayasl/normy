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
}

// ============================================================================
// Universal contract tests
// ============================================================================

#[cfg(test)]
pub fn zero_copy_when_no_changes<S: StageTestConfig>(stage: S) {
    use crate::all_langs;

    for &lang in all_langs() {
        // ← use Lang::all() if you have it, or iterate keys

        use crate::context::Context;
        let ctx = Context::new(lang);

        for &input in S::samples(lang) {
            use std::borrow::Cow;

            let already = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            let result = stage.apply(already.clone(), &ctx).unwrap();

            assert!(
                matches!(result, Cow::Borrowed(_)),
                "zero-copy failed in {lang:?} on input `{input}`"
            );
        }
    }
}

#[cfg(test)]
pub fn fast_and_slow_paths_equivalent<S: StageTestConfig>(stage: S) {
    for &lang in S::one_to_one_languages() {
        use crate::context::Context;
        use std::borrow::Cow;

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
    use crate::all_langs;

    for &lang in all_langs() {
        use crate::context::Context;

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
    use crate::{ENG, context::Context};

    let ctx = Context::new(ENG);

    let no_change = ["", "hello", "world123", "   ", "café"];
    let has_change = ["Hello", "WORLD", "  hello  ", "café\t\n", "İSTANBUL"];

    for &s in &no_change {
        use std::borrow::Cow;

        let processed = stage.apply(Cow::Borrowed(s), &ctx).unwrap();
        assert!(
            !stage.needs_apply(&processed, &ctx).unwrap(),
            "false positive on `{s}`"
        );
    }
    for &s in &has_change {
        assert!(
            stage.needs_apply(s, &ctx).unwrap(),
            "missed change on `{s}`"
        );
    }
}

#[cfg(test)]
pub fn handles_empty_string_and_ascii<S: StageTestConfig>(stage: S) {
    use std::borrow::Cow;

    use crate::{ENG, context::Context};

    let ctx = Context::new(ENG);
    let result = stage.apply(Cow::Borrowed(""), &ctx).unwrap();
    assert!(result.is_empty() && matches!(result, Cow::Borrowed(_)));
}

#[cfg(test)]
pub fn no_panic_on_mixed_scripts<S: StageTestConfig>(stage: S) {
    use std::borrow::Cow;

    use crate::{ENG, context::Context};

    let ctx = Context::new(ENG);
    let _ = stage.apply(
        Cow::Borrowed("Hello 世界 русский Türkçe العربية 简体中文"),
        &ctx,
    );
}
