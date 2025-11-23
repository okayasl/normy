use crate::{
    context::Context,
    stage::{Stage, StageError},
};
use std::borrow::Cow;
use unicode_normalization::{
    IsNormalized, UnicodeNormalization, is_nfc_quick, is_nfd_quick, is_nfkc_quick, is_nfkd_quick,
};

/// Unicode normalization stages (NFC, NFD, NFKC, NFKD).
///
/// This module provides a generic `NormalizationStage` for Unicode text normalization,
/// covering the four standard forms:
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NormalizationForm {
    NFC,
    NFD,
    NFKC,
    NFKD,
}

impl NormalizationForm {
    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NFC => "NFC",
            Self::NFD => "NFD",
            Self::NFKC => "NFKC",
            Self::NFKD => "NFKD",
        }
    }
}

/// - **NFC (Canonical Composition)**:  
///   Composes characters into precomposed form where possible (e.g., `e + ́` → `é`).  
///   Preserves ligatures, fractions, superscripts. Reversible with NFD. Ideal for display and string comparison.
pub const NFC: NormalizationStage = NormalizationStage::new(NormalizationForm::NFC);

/// - **NFD (Canonical Decomposition)**:  
///   Decomposes characters into base + combining marks (e.g., `é` → `e + ́`).  
///   Preserves compatibility chars. Reversible with NFC. Ideal for diacritic removal and linguistic analysis.
pub const NFD: NormalizationStage = NormalizationStage::new(NormalizationForm::NFD);

/// - **NFKC (Compatibility Composition)**:  
///   Decomposes compatibility characters, then composes canonical equivalents (e.g., `ﬁ` → `fi`, `²` → `2`).  
///   Lossy: formatting info lost. Some Unicode fractions may preserve Unicode fraction slashes.  
///   Useful for search, string comparison, and canonical storage.
pub const NFKC: NormalizationStage = NormalizationStage::new(NormalizationForm::NFKC);

/// - **NFKD (Compatibility Decomposition)**:  
///   Fully decomposes compatibility and canonical characters (maximal decomposition).  
///   Lossy: formatting info lost. Useful for phonetic processing and maximum normalization.
pub const NFKD: NormalizationStage = NormalizationStage::new(NormalizationForm::NFKD);

/// Generic Unicode normalization stage for all four forms.
pub struct NormalizationStage {
    form: NormalizationForm,
}

impl NormalizationStage {
    pub const fn new(form: NormalizationForm) -> Self {
        Self { form }
    }

    /// Quick check if the text is already normalized.
    #[inline(always)]
    fn quick_check(&self, text: &str) -> bool {
        match self.form {
            NormalizationForm::NFC => matches!(is_nfc_quick(text.chars()), IsNormalized::Yes),
            NormalizationForm::NFD => matches!(is_nfd_quick(text.chars()), IsNormalized::Yes),
            NormalizationForm::NFKC => matches!(is_nfkc_quick(text.chars()), IsNormalized::Yes),
            NormalizationForm::NFKD => matches!(is_nfkd_quick(text.chars()), IsNormalized::Yes),
        }
    }

    /// Perform normalization (returns `Cow::Borrowed` if already normalized).
    #[inline(always)]
    fn normalize<'a>(&self, text: Cow<'a, str>) -> Cow<'a, str> {
        if self.quick_check(&text) {
            return text;
        }
        let owned = match self.form {
            NormalizationForm::NFC => text.nfc().collect(),
            NormalizationForm::NFD => text.nfd().collect(),
            NormalizationForm::NFKC => text.nfkc().collect(),
            NormalizationForm::NFKD => text.nfkd().collect(),
        };
        Cow::Owned(owned)
    }

    pub fn as_str(&self) -> &'static str {
        self.form.as_str()
    }
}

impl Stage for NormalizationStage {
    fn name(&self) -> &'static str {
        match self.form {
            NormalizationForm::NFC => "nfc",
            NormalizationForm::NFD => "nfd",
            NormalizationForm::NFKC => "nfkc",
            NormalizationForm::NFKD => "nfkd",
        }
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(!self.quick_check(text))
    }

    #[inline(always)]
    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        Ok(self.normalize(text))
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
        let text = "cafe\u{0301}"; // e + combining acute

        // NFC: composed
        let nfc = NormalizationStage::new(NormalizationForm::NFC).apply(Cow::Borrowed(text), &c)?;
        assert_eq!(nfc, "café");
        assert_eq!(nfc.chars().count(), 4);

        // NFD: decomposed
        let nfd = NormalizationStage::new(NormalizationForm::NFD).apply(Cow::Borrowed(text), &c)?;
        assert_eq!(nfd, "cafe\u{0301}");
        assert_eq!(nfd.chars().count(), 5);

        Ok(())
    }

    #[test]
    fn test_compatibility_nfkc_nfkd() -> TestResult {
        let c = Context::default();
        let text = "ﬀﬁ ½ ①"; // ligatures + fraction + circled number

        // NFKC: compatibility composition
        let nfkc =
            NormalizationStage::new(NormalizationForm::NFKC).apply(Cow::Borrowed(text), &c)?;
        // Expected Unicode output
        assert_eq!(nfkc, "fffi 1⁄2 1");

        // NFKD: compatibility decomposition
        let nfkd =
            NormalizationStage::new(NormalizationForm::NFKD).apply(Cow::Borrowed(text), &c)?;
        assert_eq!(nfkd, "fffi 1⁄2 1"); // note fraction slash is separate U+2044

        Ok(())
    }

    #[test]
    fn test_idempotency_and_needs_apply() -> TestResult {
        let c = Context::default();
        let text = "café naïve ﬁ";

        for form in [
            NormalizationForm::NFC,
            NormalizationForm::NFD,
            NormalizationForm::NFKC,
            NormalizationForm::NFKD,
        ] {
            let stage = NormalizationStage::new(form);
            let once = stage.apply(Cow::Borrowed(text), &c)?;
            let twice = stage.apply(once.clone(), &c)?;
            assert_eq!(once, twice, "Stage {} not idempotent", stage.as_str());
            assert!(!(stage.needs_apply(&once, &c)?));
        }

        Ok(())
    }

    #[test]
    fn test_round_trip_nfc_nfd() -> TestResult {
        let c = Context::default();
        let original = "El Niño café naïve";

        let nfd =
            NormalizationStage::new(NormalizationForm::NFD).apply(Cow::Borrowed(original), &c)?;
        let back_to_nfc = NormalizationStage::new(NormalizationForm::NFC).apply(nfd, &c)?;

        assert_eq!(back_to_nfc, original);
        Ok(())
    }

    #[test]
    fn test_multilingual_nfkc() -> TestResult {
        let stage = NormalizationStage::new(NormalizationForm::NFKC);
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
        let stage_search = NormalizationStage::new(NormalizationForm::NFKC);
        let normalized_query = stage_search.apply(Cow::Borrowed(query), &c)?;

        let display_text = "café naïve ff";
        let stage_display = NormalizationStage::new(NormalizationForm::NFKC);
        let normalized_display = stage_display.apply(Cow::Borrowed(display_text), &c)?;

        assert_eq!(normalized_query, normalized_display);

        Ok(())
    }

    #[test]
    fn test_empty_and_ascii() -> TestResult {
        let c = Context::default();
        let empty = "";
        let ascii = "hello world";

        for form in [
            NormalizationForm::NFC,
            NormalizationForm::NFD,
            NormalizationForm::NFKC,
            NormalizationForm::NFKD,
        ] {
            let stage = NormalizationStage::new(form);
            assert_eq!(stage.apply(Cow::Borrowed(empty), &c)?, "");
            assert_eq!(stage.apply(Cow::Borrowed(ascii), &c)?, ascii);
        }

        Ok(())
    }
}
