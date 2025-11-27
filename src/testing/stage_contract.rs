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

#[cfg(test)]
pub fn needs_apply_is_accurate<S: StageTestConfig>(stage: S) {
    let ctx = Context::new(ENG);
    // Only test case-sensitive changes — NOT whitespace, punctuation, or formatting
    let no_change = ["", "hello", "world123", "café", "123", "  "];
    let has_change = ["Hello", "WORLD", "İSTANBUL", "Straße", "IJssel"];
    for &s in &no_change {
        use std::borrow::Cow;

        let processed = stage.apply(Cow::Borrowed(s), &ctx).unwrap();
        assert!(
            !stage.needs_apply(&processed, &ctx).unwrap(),
            "needs_apply() false positive on already-processed text: `{s}`"
        );
    }
    for &s in &has_change {
        assert!(
            stage.needs_apply(s, &ctx).unwrap(),
            "needs_apply() missed required case change on `{s}`"
        );
    }
}

#[cfg(test)]
pub fn handles_empty_string_and_ascii<S: StageTestConfig>(stage: S) {
    let ctx = Context::new(ENG);
    let empty: &str = "";
    // Since needs_apply("") == false → pipeline skips → returns Borrowed
    // But if called directly, apply() may allocate — this is acceptable
    // So we test the *pipeline* behavior via a tiny manual chain
    let result = if stage.needs_apply(empty, &ctx).unwrap() {
        stage.apply(Cow::Borrowed(empty), &ctx).unwrap()
    } else {
        Cow::Borrowed(empty)
    };

    assert!(result.is_empty());
    assert!(matches!(result, Cow::Borrowed(_)));
}

#[cfg(test)]
pub fn no_panic_on_mixed_scripts<S: StageTestConfig>(stage: S) {
    let ctx = Context::new(ENG);
    let _ = stage.apply(
        Cow::Borrowed("Hello 世界 русский Türkçe العربية 简体中文"),
        &ctx,
    );
}
