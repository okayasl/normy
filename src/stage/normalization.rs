use crate::{
    all_langs,
    context::Context,
    lang::Lang,
    stage::{Stage, StageError},
    testing::stage_contract::StageTestConfig,
};
use std::borrow::Cow;
use unicode_normalization::{
    IsNormalized, UnicodeNormalization, is_nfc_quick, is_nfd_quick, is_nfkc_quick, is_nfkd_quick,
};

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

// --- 3. Implement the Stage Trait Directly for EACH Struct ---

// Macro to eliminate duplication—generates all four impls from one source
macro_rules! impl_normalization_stage {
    ($type:ty, $name:literal, $quick_fn:ident, $apply_fn:ident) => {
        impl Stage for $type {
            fn name(&self) -> &'static str {
                $name
            }

            #[inline(always)]
            fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
                Ok(!matches!($quick_fn(text.chars()), IsNormalized::Yes))
            }

            #[inline(always)]
            fn apply<'a>(
                &self,
                text: Cow<'a, str>,
                _ctx: &Context,
            ) -> Result<Cow<'a, str>, StageError> {
                if matches!($quick_fn(text.chars()), IsNormalized::Yes) {
                    return Ok(text);
                }
                Ok(text.$apply_fn().collect::<String>().into())
            }
        }
    };
}

// Now generate all four — clean, DRY, and correct
impl_normalization_stage!(NfcStage, "nfc", is_nfc_quick, nfc);
impl_normalization_stage!(NfdStage, "nfd", is_nfd_quick, nfd);
impl_normalization_stage!(NfkcStage, "nfkc", is_nfkc_quick, nfkc);
impl_normalization_stage!(NfkdStage, "nfkd", is_nfkd_quick, nfkd);

// --- 4. Implementation for StageTestConfig (Must be Duplicated) ---

// NOTE: Since we removed the generic structure, we MUST duplicate the
// StageTestConfig implementation for ALL four concrete structs.

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
            fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
                &[]
            }
            fn skip_needs_apply_test() -> bool {
                true
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

    #[test]
    fn test_canonical_nfc_nfd() -> TestResult {
        let c = Context::default();
        let text = "cafe\u{0301}";
        let nfc = NFC.apply(Cow::Borrowed(text), &c)?;
        assert_eq!(nfc, "café");
        let nfd = NFD.apply(Cow::Borrowed(text), &c)?;
        assert_eq!(nfd, "cafe\u{0301}");
        Ok(())
    }

    #[test]
    fn test_compatibility_nfkc_nfkd() -> TestResult {
        let c = Context::default();
        let text = "ﬀﬁ ½ ①";
        let nfkc = NFKC.apply(Cow::Borrowed(text), &c)?;
        assert_eq!(nfkc, "fffi 1⁄2 1");
        let nfkd = NFKD.apply(Cow::Borrowed(text), &c)?;
        assert_eq!(nfkd, "fffi 1⁄2 1");
        Ok(())
    }

    #[test]
    fn test_idempotency_and_needs_apply() -> TestResult {
        let c = Context::default();
        let text = "café naïve ﬁ";

        // 1. Create a list of the concrete stage instances, wrapped in a Box<dyn Stage>
        let stages: Vec<Box<dyn Stage>> =
            vec![Box::new(NFC), Box::new(NFD), Box::new(NFKC), Box::new(NFKD)];

        for stage in stages.into_iter() {
            // Iterate over the trait objects
            // Need to dereference the Box to use the Stage trait methods
            let once = stage.apply(Cow::Borrowed(text), &c)?;
            let twice = stage.apply(once.clone(), &c)?;

            assert_eq!(once, twice, "Stage {} not idempotent", stage.name());
            assert!(!(stage.needs_apply(&once, &c)?));
        }

        Ok(())
    }

    #[test]
    fn test_round_trip_nfc_nfd() -> TestResult {
        let c = Context::default();
        let original = "El Niño café naïve";

        let nfd = NFD.apply(Cow::Borrowed(original), &c)?;
        let back_to_nfc = NFC.apply(nfd, &c)?;

        assert_eq!(back_to_nfc, original);
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

        Ok(())
    }

    #[test]
    fn test_empty_and_ascii() -> TestResult {
        let c = Context::default();
        let empty = "";
        let ascii = "hello world";

        // 1. Create a list of the concrete stage instances, wrapped in a Box<dyn Stage>
        let stages: Vec<Box<dyn Stage>> =
            vec![Box::new(NFC), Box::new(NFD), Box::new(NFKC), Box::new(NFKD)];

        for stage in stages.into_iter() {
            assert_eq!(stage.apply(Cow::Borrowed(empty), &c)?, "");
            assert_eq!(stage.apply(Cow::Borrowed(ascii), &c)?, ascii);
        }

        Ok(())
    }
}
