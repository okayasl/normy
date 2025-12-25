use crate::{
    all_langs,
    context::Context,
    lang::Lang,
    stage::{FusableStage, Stage, StageError, StaticFusableStage},
    testing::stage_contract::StageTestConfig,
};
use std::iter::FusedIterator;

use icu_normalizer::{
    ComposingNormalizer, ComposingNormalizerBorrowed, DecomposingNormalizer,
    DecomposingNormalizerBorrowed,
};
use std::{borrow::Cow, sync::LazyLock};
// ── ICU4X ──
static ICU4X_NFC: LazyLock<ComposingNormalizerBorrowed> =
    LazyLock::new(ComposingNormalizer::new_nfc);
static ICU4X_NFKC: LazyLock<ComposingNormalizerBorrowed> =
    LazyLock::new(ComposingNormalizer::new_nfkc);
static ICU4X_NFD: LazyLock<DecomposingNormalizerBorrowed<'static>> =
    LazyLock::new(DecomposingNormalizer::new_nfd);
static ICU4X_NFKD: LazyLock<DecomposingNormalizerBorrowed<'static>> =
    LazyLock::new(DecomposingNormalizerBorrowed::new_nfkd);

// --- 1. Define Concrete Normalization Stage Structs ---

/// Unicode Normalization Form C (Canonical Composition)
#[derive(Default, Clone, Copy)]
pub struct NfcStage;

/// Unicode Normalization Form D (Canonical Decomposition)
#[derive(Default, Clone, Copy)]
pub struct NfdStage;

/// Unicode Normalization Form KC (Compatibility Composition)
#[derive(Default, Clone, Copy)]
pub struct NfkcStage;

/// Unicode Normalization Form KD (Compatibility Decomposition)
#[derive(Default, Clone, Copy)]
pub struct NfkdStage;

// --- 2. Public Constants ---

// The constants now use the direct, concrete stage structs.
pub const NFC: NfcStage = NfcStage;
pub const NFD: NfdStage = NfdStage;
pub const NFKC: NfkcStage = NfkcStage;
pub const NFKD: NfkdStage = NfkdStage;

macro_rules! impl_normalization_stage {
    ($stage:ty, $name:literal, $norm:ident, $adapter:ident) => {
        impl Stage for $stage {
            fn name(&self) -> &'static str {
                $name
            }

            #[inline(always)]
            fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
                Ok(!$norm.is_normalized(text))
            }

            #[inline(always)]
            fn apply<'a>(
                &self,
                text: Cow<'a, str>,
                _ctx: &Context,
            ) -> Result<Cow<'a, str>, StageError> {
                Ok($norm.normalize(text.as_ref()).into_owned().into())
            }

            #[inline]
            fn safe_skip_approximation(&self) -> bool {
                // UNSAFE when chained with different normalization forms!
                // See impl_composing_stage for explanation.
                false
            }

            #[inline]
            fn as_fusable(&self) -> Option<&dyn FusableStage> {
                Some(self)
                //None
            }
        }

        impl StaticFusableStage for $stage {
            type Adapter<'a, I>
                = $adapter<'a, I>
            where
                I: FusedIterator<Item = char> + 'a;

            fn supports_static_fusion(&self) -> bool {
                true
                //false
            }

            #[inline(always)]
            fn static_fused_adapter<'a, I>(
                &self,
                input: I,
                _ctx: &'a Context,
            ) -> Self::Adapter<'a, I>
            where
                I: FusedIterator<Item = char> + 'a,
            {
                $adapter {
                    iter: $norm.normalize_iter(input),
                    _marker: std::marker::PhantomData,
                }
            }
        }

        impl FusableStage for $stage {
            fn dyn_fused_adapter<'a>(
                &self,
                input: Box<dyn FusedIterator<Item = char> + 'a>,
                _ctx: &'a Context,
            ) -> Box<dyn FusedIterator<Item = char> + 'a> {
                // Wrap ICU4X's iterator with our FusedIterator wrapper
                Box::new($adapter {
                    iter: $norm.normalize_iter(input),
                    _marker: std::marker::PhantomData,
                })
            }
        }
    };
}

// --- 5. Apply Macros ---
impl_normalization_stage!(NfcStage, "nfc", ICU4X_NFC, NormalizationComposeAdapter);
impl_normalization_stage!(NfkcStage, "nfkc", ICU4X_NFKC, NormalizationComposeAdapter);
impl_normalization_stage!(NfdStage, "nfd", ICU4X_NFD, NormalizationDecomposeAdapter);
impl_normalization_stage!(NfkdStage, "nfkd", ICU4X_NFKD, NormalizationDecomposeAdapter);

// Generic Adapter for Composition (NFC, NFKC)
pub struct NormalizationComposeAdapter<'a, I>
where
    I: Iterator<Item = char>,
{
    iter: icu_normalizer::Composition<'static, I>,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a, I: Iterator<Item = char>> Iterator for NormalizationComposeAdapter<'a, I> {
    type Item = char;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<'a, I: Iterator<Item = char>> FusedIterator for NormalizationComposeAdapter<'a, I> {}

// Generic Adapter for Decomposition (NFD, NFKD)
pub struct NormalizationDecomposeAdapter<'a, I>
where
    I: Iterator<Item = char>,
{
    iter: icu_normalizer::Decomposition<'static, I>,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a, I: Iterator<Item = char>> Iterator for NormalizationDecomposeAdapter<'a, I> {
    type Item = char;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<'a, I: Iterator<Item = char>> FusedIterator for NormalizationDecomposeAdapter<'a, I> {}

// --- 4. Implementation for StageTestConfig (Must be Duplicated) ---
macro_rules! impl_stage_test_config {
    ($type:ty) => {
        impl StageTestConfig for $type {
            fn one_to_one_languages() -> &'static [Lang] {
                all_langs()
            }
            fn samples(_lang: Lang) -> &'static [&'static str] {
                &["café", "naïve", "e\u{0301}", "ﬁle", "①②③", ""]
            }
            fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
                &["hello", "world123", "test", ""]
            }
        }
    };
}

impl_stage_test_config!(NfcStage);
impl_stage_test_config!(NfdStage);
impl_stage_test_config!(NfkcStage);
impl_stage_test_config!(NfkdStage);

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;

    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(NFC);
        assert_stage_contract!(NFD);
        assert_stage_contract!(NFKC);
        assert_stage_contract!(NFKD);
    }

    #[test]
    fn samples_include_decomposed() {
        let c = Context::default();
        let decomposed = "cafe\u{0301}"; // e + combining acute
        let nfc = NFC.apply(Cow::Borrowed(decomposed), &c).unwrap();
        assert_eq!(nfc, "café");
        let nfd = NFD.apply(Cow::Borrowed(&nfc), &c).unwrap();
        assert_eq!(nfd, decomposed);
    }

    #[test]
    fn compatibility_decomposes_ligatures() {
        let c = Context::default();
        let ligature = "ﬁ"; // fi ligature
        let nfkc = NFKC.apply(Cow::Borrowed(ligature), &c).unwrap();
        assert_eq!(nfkc, "fi");
        let nfkd = NFKD.apply(Cow::Borrowed(ligature), &c).unwrap();
        assert_eq!(nfkd, "fi");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use std::borrow::Cow;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// Helper to verify that apply() and fusable iterator produce identical results
    fn assert_apply_equals_fusable(stage: &dyn Stage, text: &str, ctx: &Context) -> TestResult {
        // Get result from apply method
        let apply_result = stage.apply(Cow::Borrowed(text), ctx)?;

        // Get result from fusable iterator path
        if let Some(fusable) = stage.as_fusable() {
            let iter: Box<dyn FusedIterator<Item = char>> = Box::new(text.chars());
            let fused_iter = fusable.dyn_fused_adapter(iter, ctx);
            let fusable_result: String = fused_iter.collect();

            assert_eq!(
                apply_result.as_ref(),
                fusable_result.as_str(),
                "Stage {} produced different results: apply='{}' vs fusable='{}'",
                stage.name(),
                apply_result,
                fusable_result
            );
        } else {
            panic!(
                "Stage {} should be fusable but as_fusable() returned None",
                stage.name()
            );
        }

        Ok(())
    }

    #[test]
    fn test_canonical_nfc_nfd() -> TestResult {
        let c = Context::default();
        let text = "cafe\u{0301}";

        // Test NFC
        let nfc = NFC.apply(Cow::Borrowed(text), &c)?;
        assert_eq!(nfc, "café");
        assert_apply_equals_fusable(&NFC, text, &c)?;

        // Test NFD
        let nfd = NFD.apply(Cow::Borrowed(text), &c)?;
        assert_eq!(nfd, "cafe\u{0301}");
        assert_apply_equals_fusable(&NFD, text, &c)?;

        Ok(())
    }

    #[test]
    fn test_compatibility_nfkc_nfkd() -> TestResult {
        let c = Context::default();
        let text = "ﬀﬁ ½ ①";

        // Test NFKC
        let nfkc = NFKC.apply(Cow::Borrowed(text), &c)?;
        assert_eq!(nfkc, "fffi 1⁄2 1");
        assert_apply_equals_fusable(&NFKC, text, &c)?;

        // Test NFKD
        let nfkd = NFKD.apply(Cow::Borrowed(text), &c)?;
        assert_eq!(nfkd, "fffi 1⁄2 1");
        assert_apply_equals_fusable(&NFKD, text, &c)?;

        Ok(())
    }

    #[test]
    fn test_idempotency_and_needs_apply() -> TestResult {
        let c = Context::default();
        let text = "café naïve ﬁ";

        let stages: Vec<Box<dyn Stage>> =
            vec![Box::new(NFC), Box::new(NFD), Box::new(NFKC), Box::new(NFKD)];

        for stage in stages.iter() {
            // Test apply idempotency
            let once = stage.apply(Cow::Borrowed(text), &c)?;
            let twice = stage.apply(once.clone(), &c)?;
            assert_eq!(once, twice, "Stage {} not idempotent", stage.name());
            assert!(!stage.needs_apply(&once, &c)?);

            // Verify apply equals fusable for both original and normalized text
            assert_apply_equals_fusable(stage.as_ref(), text, &c)?;
            assert_apply_equals_fusable(stage.as_ref(), &once, &c)?;
        }

        Ok(())
    }

    #[test]
    fn test_round_trip_nfc_nfd() -> TestResult {
        let c = Context::default();
        let original = "El Niño café naïve";

        // Test with apply
        let nfd = NFD.apply(Cow::Borrowed(original), &c)?;
        let back_to_nfc = NFC.apply(nfd, &c)?;
        assert_eq!(back_to_nfc, original);

        // Verify fusable produces same results
        assert_apply_equals_fusable(&NFD, original, &c)?;
        assert_apply_equals_fusable(&NFC, &back_to_nfc, &c)?;

        Ok(())
    }

    #[test]
    fn test_multilingual_nfkc() -> TestResult {
        let stage = NFKC;
        let c = Context::default();
        let input = "Hello, 世界! ﬁﬀ caféﬀﬃﬃ";

        let result = stage.apply(Cow::Borrowed(input), &c)?;
        // ligatures expanded, accents composed, full-width preserved
        assert_eq!(result, "Hello, 世界! fiff caféffffiffi");

        // Already normalized
        assert!(!stage.needs_apply(&result, &c)?);

        // Verify fusable produces same results
        assert_apply_equals_fusable(&stage, input, &c)?;
        assert_apply_equals_fusable(&stage, &result, &c)?;

        Ok(())
    }

    #[test]
    fn test_search_vs_display_pipeline() -> TestResult {
        let c = Context::default();
        let query = "café naïve ff"; // decomposed + ligatures
        let stage_search = NFKC;
        let normalized_query = stage_search.apply(Cow::Borrowed(query), &c)?;

        let display_text = "café naïve ff";
        let stage_display = NFKC;
        let normalized_display = stage_display.apply(Cow::Borrowed(display_text), &c)?;

        assert_eq!(normalized_query, normalized_display);

        // Verify fusable produces same results
        assert_apply_equals_fusable(&stage_search, query, &c)?;
        assert_apply_equals_fusable(&stage_display, display_text, &c)?;

        Ok(())
    }

    #[test]
    fn test_empty_and_ascii() -> TestResult {
        let c = Context::default();
        let empty = "";
        let ascii = "hello world";

        let stages: Vec<Box<dyn Stage>> =
            vec![Box::new(NFC), Box::new(NFD), Box::new(NFKC), Box::new(NFKD)];

        for stage in stages.iter() {
            assert_eq!(stage.apply(Cow::Borrowed(empty), &c)?, "");
            assert_eq!(stage.apply(Cow::Borrowed(ascii), &c)?, ascii);

            // Verify fusable produces same results
            assert_apply_equals_fusable(stage.as_ref(), empty, &c)?;
            assert_apply_equals_fusable(stage.as_ref(), ascii, &c)?;
        }

        Ok(())
    }

    #[test]
    fn test_fusable_chaining() -> TestResult {
        let c = Context::default();
        let text = "café ﬁ";

        // Test chaining multiple fusable stages
        // Chain: text -> NFD -> NFKC
        let nfd_fusable = NFD.as_fusable().unwrap();
        let nfkc_fusable = NFKC.as_fusable().unwrap();

        // Build fused chain
        let iter1: Box<dyn FusedIterator<Item = char>> = Box::new(text.chars());
        let iter2 = nfd_fusable.dyn_fused_adapter(iter1, &c);
        let iter3 = nfkc_fusable.dyn_fused_adapter(iter2, &c);
        let fused_result: String = iter3.collect();

        // Compare with sequential apply
        let step1 = NFD.apply(Cow::Borrowed(text), &c)?;
        let step2 = NFKC.apply(step1, &c)?;

        assert_eq!(
            fused_result,
            step2.as_ref(),
            "Fused chain should produce same result as sequential apply"
        );

        Ok(())
    }

    #[test]
    fn test_fusable_with_special_chars() -> TestResult {
        let c = Context::default();

        // Test various Unicode categories
        let test_cases = vec![
            ("", "empty string"),
            ("a", "simple ASCII"),
            ("café", "composed accents"),
            ("cafe\u{0301}", "decomposed accents"),
            ("ﬁﬀ", "ligatures"),
            ("½①", "fractions and circled numbers"),
            ("日本語", "CJK characters"),
            ("𝕳𝖊𝖑𝖑𝖔", "mathematical alphanumeric symbols"),
            ("🎉🎊", "emoji"),
            ("\u{200B}\u{200C}\u{200D}", "zero-width characters"),
        ];

        let stages: Vec<Box<dyn Stage>> =
            vec![Box::new(NFC), Box::new(NFD), Box::new(NFKC), Box::new(NFKD)];

        for (text, desc) in test_cases {
            for stage in stages.iter() {
                assert_apply_equals_fusable(stage.as_ref(), text, &c)
                    .map_err(|e| format!("Failed for {}: {}", desc, e))?;
            }
        }

        Ok(())
    }
}
