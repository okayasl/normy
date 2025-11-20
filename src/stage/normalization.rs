//! Unicode normalization forms (NFC, NFD, NFKC, NFKD).

use crate::{
    context::Context,
    stage::{Stage, StageError},
};
use std::borrow::Cow;
use unicode_normalization::{
    IsNormalized, UnicodeNormalization, is_nfc_quick, is_nfd_quick, is_nfkc_quick, is_nfkd_quick,
};

// ============================================================================
// NFC - Canonical Composition
// ============================================================================

/// Unicode Normalization Form C (Canonical Composition).
///
/// Composes characters into precomposed form where possible:
/// - `e` + combining acute → `é` (single codepoint U+00E9)
///
/// # Characteristics
/// - **Canonical**: Only composes canonical equivalents
/// - **Preserves**: Ligatures, fractions, superscripts (compatibility chars)
/// - **Reversible**: Can round-trip with NFD
///
/// # When to Use
/// - **Display text**: Most fonts expect NFC
/// - **String comparison**: Standard for equality checks
/// - **Storage**: Most databases/formats expect NFC
/// - **After NFD processing**: Recompose after removing diacritics
pub struct NFC;

impl Stage for NFC {
    fn name(&self) -> &'static str {
        "nfc"
    }

    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(!matches!(is_nfc_quick(text.chars()), IsNormalized::Yes))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, _ctx)? {
            return Ok(text);
        }
        Ok(Cow::Owned(text.nfc().collect()))
    }
}

// ============================================================================
// NFD - Canonical Decomposition
// ============================================================================

/// Unicode Normalization Form D (Canonical Decomposition).
///
/// Decomposes precomposed characters into base + combining marks:
/// - `é` (U+00E9) → `e` + combining acute
///
/// # Characteristics
/// - **Canonical**: Only decomposes canonical equivalents
/// - **Preserves**: Ligatures, fractions, superscripts (compatibility chars)
/// - **Reversible**: Can round-trip with NFC
///
/// # When to Use
/// - **Before removing diacritics**: Separates base letters from accents
/// - **Phonetic processing**: Need base forms without marks
/// - **Linguistic analysis**: Process base and marks separately
pub struct NFD;

impl Stage for NFD {
    fn name(&self) -> &'static str {
        "nfd"
    }

    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(!matches!(is_nfd_quick(text.chars()), IsNormalized::Yes))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, _ctx)? {
            return Ok(text);
        }
        Ok(Cow::Owned(text.nfd().collect()))
    }
}

// ============================================================================
// NFKC - Compatibility Composition
// ============================================================================

/// Unicode Normalization Form KC (Compatibility Composition).
///
/// Decomposes compatibility characters, then composes canonical forms:
/// - `ﬁ` → `fi` (ligature expanded)
/// - `½` → decomposed → composed
/// - `²` → `2` (superscript normalized)
/// - `e` + combining acute → `é` (composed)
///
/// # Characteristics
/// - **Lossy**: Formatting information is lost
/// - **Aggressive**: Normalizes all compatibility variants
/// - **Not reversible**: Cannot recover original forms
///
/// # When to Use
/// - **Search/indexing**: "ﬁle" and "file" should match
/// - **String comparison**: Ignore formatting differences
/// - **Data normalization**: Canonical form for storage
///
/// # ⚠️ Warning: Lossy Transformation
/// This form **loses formatting information**:
/// - Ligatures: `ﬀ` → `ff`
/// - Fractions: `¾` → decomposed
/// - Superscripts: `m²` → `m2`
/// - Full-width: `Ａ` → `A`
/// - Circled: `②` → `2`
pub struct NFKC;

impl Stage for NFKC {
    fn name(&self) -> &'static str {
        "nfkc"
    }

    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(!matches!(is_nfkc_quick(text.chars()), IsNormalized::Yes))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, _ctx)? {
            return Ok(text);
        }
        Ok(Cow::Owned(text.nfkc().collect()))
    }
}

// ============================================================================
// NFKD - Compatibility Decomposition
// ============================================================================

/// Unicode Normalization Form KD (Compatibility Decomposition).
///
/// Fully decomposes both compatibility AND canonical characters:
/// - `ﬁ` → `fi` (ligature expanded)
/// - `é` → `e` + combining acute (precomposed → decomposed)
/// - `½` → decomposed
/// - `²` → `2` (superscript normalized)
///
/// # Characteristics
/// - **Lossy**: Formatting information is lost
/// - **Most aggressive**: Maximum decomposition
/// - **Not reversible**: Cannot recover original forms
///
/// # When to Use
/// - **Before removing diacritics**: For compatibility-char-heavy text
/// - **Maximum normalization**: Want all variants expanded
/// - **Text-to-speech**: Need phonetic base forms
///
/// # ⚠️ Warning: Most Lossy Transformation
/// All formatting is lost:
/// - All ligatures expanded
/// - All superscripts/subscripts normalized
/// - All full-width chars converted to ASCII
/// - All circled/parenthesized variants normalized
pub struct NFKD;

impl Stage for NFKD {
    fn name(&self) -> &'static str {
        "nfkd"
    }

    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(!matches!(is_nfkd_quick(text.chars()), IsNormalized::Yes))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, _ctx)? {
            return Ok(text);
        }
        Ok(Cow::Owned(text.nfkd().collect()))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::data::ENG;

    #[test]
    fn test_nfc_compose_accents() {
        let stage = NFC;
        let c = Context::new(ENG);

        let text = "cafe\u{0301}"; // e + combining acute
        let result = stage.apply(Cow::Borrowed(text), &c).unwrap();
        assert_eq!(result, "café");
        assert_eq!(result.chars().count(), 4); // Single é character
    }

    #[test]
    fn test_nfc_preserves_ligatures() {
        let stage = NFC;
        let c = Context::new(ENG);

        let text = "ﬁle ﬂoor oﬀer";
        let result = stage.apply(Cow::Borrowed(text), &c).unwrap();
        assert_eq!(result, "ﬁle ﬂoor oﬀer");
    }

    #[test]
    fn test_nfc_already_normalized() {
        let stage = NFC;
        let c = Context::new(ENG);

        let text = "hello world";
        assert!(!stage.needs_apply(text, &c).unwrap());

        let result = stage.apply(Cow::Borrowed(text), &c).unwrap();
        assert!(matches!(result, Cow::Borrowed(_))); // Zero-copy
    }

    // ------------------------------------------------------------------------
    // NFD Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_nfd_decompose_accents() {
        let stage = NFD;
        let c = Context::new(ENG);

        let text = "café";
        let result = stage.apply(Cow::Borrowed(text), &c).unwrap();
        assert_eq!(result, "cafe\u{0301}");
        assert_eq!(result.chars().count(), 5); // e + combining mark
    }

    #[test]
    fn test_nfd_preserves_ligatures() {
        let stage = NFD;
        let c = Context::new(ENG);

        let text = "ﬁle ﬂoor";
        let result = stage.apply(Cow::Borrowed(text), &c).unwrap();
        assert_eq!(result, "ﬁle ﬂoor"); // Compatibility chars not decomposed
    }

    #[test]
    fn test_nfd_preserves_fractions() {
        let stage = NFD;
        let c = Context::new(ENG);

        let text = "½ ¾";
        let result = stage.apply(Cow::Borrowed(text), &c).unwrap();
        assert_eq!(result, "½ ¾");
    }

    // ------------------------------------------------------------------------
    // NFKC Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_nfkc_expands_ligatures() {
        let stage = NFKC;
        let c = Context::new(ENG);

        assert_eq!(stage.apply(Cow::Borrowed("ﬁle"), &c).unwrap(), "file");
        assert_eq!(stage.apply(Cow::Borrowed("ﬂoor"), &c).unwrap(), "floor");
        assert_eq!(stage.apply(Cow::Borrowed("oﬀer"), &c).unwrap(), "offer");
    }

    #[test]
    fn test_nfkc_normalizes_superscripts() {
        let stage = NFKC;
        let c = Context::new(ENG);

        assert_eq!(stage.apply(Cow::Borrowed("m²"), &c).unwrap(), "m2");
        assert_eq!(stage.apply(Cow::Borrowed("x³"), &c).unwrap(), "x3");
    }

    #[test]
    fn test_nfkc_normalizes_circled() {
        let stage = NFKC;
        let c = Context::new(ENG);

        assert_eq!(stage.apply(Cow::Borrowed("①②③"), &c).unwrap(), "123");
    }

    #[test]
    fn test_nfkc_normalizes_full_width() {
        let stage = NFKC;
        let c = Context::new(ENG);

        assert_eq!(stage.apply(Cow::Borrowed("ＡＢＣＤ"), &c).unwrap(), "ABCD");
    }

    #[test]
    fn test_nfkc_composes_accents() {
        let stage = NFKC;
        let c = Context::new(ENG);

        let text = "cafe\u{0301}";
        let result = stage.apply(Cow::Borrowed(text), &c).unwrap();
        assert_eq!(result, "café"); // Composed after normalization
    }

    // ------------------------------------------------------------------------
    // NFKD Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_nfkd_fully_decomposes() {
        let stage = NFKD;
        let c = Context::new(ENG);

        let text = "café ﬁ";
        let result = stage.apply(Cow::Borrowed(text), &c).unwrap();

        assert!(result.contains("cafe")); // é decomposed
        assert!(result.contains("fi")); // Ligature expanded
        assert!(result.contains('\u{0301}')); // Combining mark present
    }

    #[test]
    fn test_nfkd_expands_ligatures() {
        let stage = NFKD;
        let c = Context::new(ENG);

        assert_eq!(stage.apply(Cow::Borrowed("ﬁ"), &c).unwrap(), "fi");
        assert_eq!(stage.apply(Cow::Borrowed("ﬂ"), &c).unwrap(), "fl");
    }

    #[test]
    fn test_nfkd_normalizes_superscripts() {
        let stage = NFKD;
        let c = Context::new(ENG);

        let text = "m²";
        let result = stage.apply(Cow::Borrowed(text), &c).unwrap();
        assert_eq!(result, "m2");
    }

    // ------------------------------------------------------------------------
    // Comparison Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_canonical_vs_compatibility() {
        let c = Context::new(ENG);
        let text = "café";

        let nfc = NFC.apply(Cow::Borrowed(text), &c).unwrap();
        let nfd = NFD.apply(Cow::Borrowed(text), &c).unwrap();
        let nfkc = NFKC.apply(Cow::Borrowed(text), &c).unwrap();
        let nfkd = NFKD.apply(Cow::Borrowed(text), &c).unwrap();

        // Canonical forms
        assert_eq!(nfc, "café"); // Composed
        assert_eq!(nfd, "cafe\u{0301}"); // Decomposed
        assert_eq!(nfc.chars().count(), 4);
        assert_eq!(nfd.chars().count(), 5);

        // Compatibility forms (same as canonical for no compat chars)
        assert_eq!(nfkc, "café"); // Composed
        assert_eq!(nfkd, "cafe\u{0301}"); // Decomposed
    }

    #[test]
    fn test_ligature_handling_across_forms() {
        let c = Context::new(ENG);
        let text = "ﬁ";

        let nfc = NFC.apply(Cow::Borrowed(text), &c).unwrap();
        let nfd = NFD.apply(Cow::Borrowed(text), &c).unwrap();
        let nfkc = NFKC.apply(Cow::Borrowed(text), &c).unwrap();
        let nfkd = NFKD.apply(Cow::Borrowed(text), &c).unwrap();

        // Canonical: preserve ligature
        assert_eq!(nfc, "ﬁ");
        assert_eq!(nfd, "ﬁ");

        // Compatibility: expand ligature
        assert_eq!(nfkc, "fi");
        assert_eq!(nfkd, "fi");
    }

    #[test]
    fn test_fraction_handling() {
        let c = Context::new(ENG);
        let text = "½";

        let nfc = NFC.apply(Cow::Borrowed(text), &c).unwrap();
        let nfd = NFD.apply(Cow::Borrowed(text), &c).unwrap();

        // Canonical: preserve
        assert_eq!(nfc, "½");
        assert_eq!(nfd, "½");

        // Compatibility: decompose (becomes multiple chars)
        let nfkc = NFKC.apply(Cow::Borrowed(text), &c).unwrap();
        let nfkd = NFKD.apply(Cow::Borrowed(text), &c).unwrap();

        assert_ne!(nfkc, "½");
        assert_ne!(nfkd, "½");
        assert!(nfkc.chars().count() > 1 || nfkd.chars().count() > 1);
    }

    // ------------------------------------------------------------------------
    // Round-Trip Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_nfc_nfd_round_trip() {
        let c = Context::new(ENG);
        let original = "café naïve résumé";

        // NFC → NFD → NFC should be idempotent
        let nfd = NFD.apply(Cow::Borrowed(original), &c).unwrap();
        let back_to_nfc = NFC.apply(Cow::Borrowed(&nfd), &c).unwrap();

        assert_eq!(back_to_nfc, original);
    }

    #[test]
    fn test_nfkc_nfkd_not_reversible() {
        let c = Context::new(ENG);
        let original = "ﬁle"; // Ligature

        // NFKD expands ligature
        let nfkd = NFKD.apply(Cow::Borrowed(original), &c).unwrap();
        assert_eq!(nfkd, "file");

        // NFKC cannot restore ligature
        let nfkc = NFKC.apply(Cow::Borrowed(&nfkd), &c).unwrap();
        assert_eq!(nfkc, "file"); // Still expanded
        assert_ne!(nfkc, original); // Cannot recover original
    }

    // ------------------------------------------------------------------------
    // Edge Cases
    // ------------------------------------------------------------------------

    #[test]
    fn test_empty_string() {
        let c = Context::new(ENG);

        for stage in [&NFC as &dyn Stage, &NFD, &NFKC, &NFKD] {
            assert!(!stage.needs_apply("", &c).unwrap());
            assert_eq!(stage.apply(Cow::Borrowed(""), &c).unwrap(), "");
        }
    }

    #[test]
    fn test_ascii_only() {
        let c = Context::new(ENG);
        let text = "hello world";

        for stage in [&NFC as &dyn Stage, &NFD, &NFKC, &NFKD] {
            let result = stage.apply(Cow::Borrowed(text), &c).unwrap();
            assert_eq!(result, text);
        }
    }

    #[test]
    fn test_idempotency() {
        let c = Context::new(ENG);
        let text = "café ﬁ";

        // Each form should be idempotent
        for stage in [&NFC as &dyn Stage, &NFD, &NFKC, &NFKD] {
            let once = stage.apply(Cow::Borrowed(text), &c).unwrap();
            let twice = stage.apply(Cow::Borrowed(&once), &c).unwrap();
            assert_eq!(once, twice, "Stage {} not idempotent", stage.name());
        }
    }

    // ------------------------------------------------------------------------
    // Real-World Examples
    // ------------------------------------------------------------------------

    #[test]
    fn test_search_normalization_pipeline() {
        let c = Context::new(ENG);

        // For search: NFKC to normalize everything
        let query = "½ ﬁle café";
        let normalized = NFKC.apply(Cow::Borrowed(query), &c).unwrap();

        // Ligature expanded, fraction decomposed, accent composed
        assert!(normalized.contains("file"));
        assert!(!normalized.contains("ﬁ"));
    }

    #[test]
    fn test_display_normalization_pipeline() {
        let c = Context::new(ENG);

        // For display: NFC to get precomposed characters
        let text = "cafe\u{0301}"; // Decomposed
        let display = NFC.apply(Cow::Borrowed(text), &c).unwrap();

        assert_eq!(display, "café");
        assert_eq!(display.chars().count(), 4); // Single é
    }
}
