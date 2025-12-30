use crate::{
    all_langs,
    context::Context,
    lang::Lang,
    stage::{Stage, StageError, StaticFusableStage},
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
                &[
                    "café",
                    "naïve",
                    "e\u{0301}",
                    "ﬃﬃ",
                    "ﬁle",
                    "½",
                    "Ⅻ",
                    "①②③",
                    "㍿",
                    "ﬀ", // extra ligatures/fractions
                    "日本語",
                    "El Niño",
                    "Résumé", // more composed
                    "",
                    "hello", // already in pass-through but harmless
                    "𝕳𝖊𝖑𝖑𝖔",
                    "ℌℨℓℓℴ", // math alphanumerics
                    ";",     // Greek question mark (compatibility)
                ]
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

// Single source of truth: universal contract compliance
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use std::borrow::Cow;

    #[test]
    fn nfc_nfd_round_trip() {
        let ctx = Context::default();
        let test_cases = ["El Niño café naïve", "Résumé", "日本語テスト", "ﬁﬀ½①"];

        for original in test_cases {
            // Decompose then recompose
            let nfd = NFD.apply(Cow::Borrowed(original), &ctx).unwrap();
            let back_to_nfc = NFC.apply(nfd, &ctx).unwrap();

            // Should equal original NFC form
            let original_nfc = NFC.apply(Cow::Borrowed(original), &ctx).unwrap();
            assert_eq!(
                back_to_nfc, original_nfc,
                "Round-trip failed for: {}",
                original
            );
        }
    }
}
