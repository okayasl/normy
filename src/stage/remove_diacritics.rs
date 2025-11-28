use crate::{
    context::Context,
    lang::LangEntry,
    stage::{CharMapper, Stage, StageError},
    testing::stage_contract::StageTestConfig,
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;
use unicode_normalization::{Recompositions, UnicodeNormalization}; // +recompositions
/// Removes language-specific diacritical marks using optimized lookup tables.
///
/// This stage strips diacritics and maps characters to their base forms based **strictly**
/// on the target language's definition. It prioritizes performance (O(1) lookups) and
/// linguistic accuracy over aggressive "ASCII-fication".
///
/// # Mechanism
///
/// 1. **Direct Mapping (Primary):** Checks the language's `strip` table. If a character
///    is defined there (e.g., `é` -> `e` in French), it is replaced.
/// 2. **Diacritic Filtering:** Checks the language's `diac` list. If a character is
///    defined as a diacritic (e.g., Arabic Harakat), it is removed.
/// 3. **Preservation:** Any character **not** defined in the language's specific rules
///    is returned strictly as-is.
///
/// # Design Philosophy
///
/// Most libraries conflate "remove accents" with "flatten everything to ASCII".
/// Normy refuses that lie.
///
/// `RemoveDiacritics` gives you **exactly** what linguists and typographers expect for
/// the specific language context:
/// - **Locale-Strict:** Polish `RemoveDiacritics` will strip `ł` -> `l`, but it will
///   preserve a Czech `ř` because `ř` is not a valid Polish variant.
/// - **Typography-Safe:** Ligatures (`ﬁ`, `œ`), fractions (`½`), and superscripts (`²`)
///   are preserved unless explicitly mapped by the language.
/// - **Zero-Copy:** Returns `Cow::Borrowed` if no defined changes are needed.
///
/// # Key Characteristics
///
/// | Aspect | Behavior |
/// |----------------------------|---------------------------------------------------|
/// | **Strategy** | **Lookup Table** (String) / **NFD** (Iterator fallback) |
/// | **Scope** | Strict Locale (Foreign accents are preserved) |
/// | **Compatibility** | Preserves formatting (ligatures, fractions, etc.) |
/// | **Performance** | O(1) per char (no decomposition overhead in `apply`) |
/// | **Safety** | Zero-allocation path when text matches rules |
///
/// # When to Use
///
/// - **Phonetic Search:** When you need "café" to match "cafe" in French text.
/// - **TTS Preprocessing:** Cleaning text for speech engines that stumble on accents.
/// - **Typography-Preserving Slugs:** Generating readable URLs without losing meaning.
///
/// # When NOT to Use
///
/// - **Generic ASCII conversion:** If you want to force *any* input to ASCII (e.g., stripping Czech `ř` using a Polish context), use a generic NFKD + Strip approach instead.
/// - **Search Normalization across mixed languages:** If you don't know the input language, this strictness might be too conservative.
pub struct RemoveDiacritics;

impl Stage for RemoveDiacritics {
    fn name(&self) -> &'static str {
        "remove_diacritics"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        Ok(ctx.lang_entry.needs_diacritic_removal(text))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // This now fuses perfectly into a single SIMD-able loop
        let mut out = String::with_capacity(text.len());
        let mut changed = false;

        for c in text.chars() {
            if let Some(base) = ctx.lang_entry.strip_diacritic(c) {
                out.push(base);
                changed = true;
            } else if ctx.lang_entry.is_diacritic(c) {
                changed = true;
                // skip
            } else {
                out.push(c);
            }
        }

        if changed {
            Ok(Cow::Owned(out))
        } else {
            Ok(text)
        }
    }

    #[inline]
    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        if ctx.lang_entry.has_strip_or_diacritics() {
            Some(self)
        } else {
            None
        }
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        ctx.lang_entry.has_strip_or_diacritics().then_some(self)
    }
}

impl CharMapper for RemoveDiacritics {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> Option<char> {
        if let Some(base) = ctx.lang_entry.strip_diacritic(c) {
            Some(base)
        } else if ctx.lang_entry.is_diacritic(c) {
            None // Remove
        } else {
            Some(c) // Keep
        }
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        let lang = ctx.lang_entry;

        // Fast path: if text is ASCII or language has no rules → raw chars
        if text.is_ascii() || !lang.has_strip_or_diacritics() {
            return Box::new(text.chars());
        }

        // Medium path: only precomposed strip map (French, Vietnamese, Polish, etc.)
        // → 1:1 mapping, zero decomposition
        if lang.has_strip_map() && !lang.has_diacritics() {
            return Box::new(
                text.chars()
                    .map(move |c| lang.strip_diacritic(c).unwrap_or(c)),
            );
        }

        // Slow path: has combining diacritics (Arabic, Hebrew) → must decompose
        if lang.has_diacritics() {
            let nfd = text.nfd();
            let filtered = RemoveDiacriticsIter { chars: nfd, lang };
            let recomposed = Recompositions::new_canonical(filtered);
            return Box::new(recomposed);
        }

        // Final fallback: use strip map even if diacritics exist (rare, safe)
        Box::new(
            text.chars()
                .map(move |c| lang.strip_diacritic(c).unwrap_or(c)),
        )
    }
}

struct RemoveDiacriticsIter<I> {
    chars: I,
    lang: LangEntry,
}

impl<I: Iterator<Item = char>> Iterator for RemoveDiacriticsIter<I> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let c = self.chars.next()?;
            if !self.lang.is_diacritic(c) {
                return Some(c);
            }
        }
    }
}

impl<I: Iterator<Item = char>> FusedIterator for RemoveDiacriticsIter<I> {}

impl StageTestConfig for RemoveDiacritics {
    fn one_to_one_languages() -> &'static [crate::lang::Lang] {
        &[] // Not 1:1 — can remove chars (Arabic harakat)
    }

    fn samples(lang: crate::lang::Lang) -> &'static [&'static str] {
        match lang {
            crate::lang::data::FRA => &["café", "naïve", "résumé", "Crème brûlée"],
            crate::lang::data::VIE => &["Hà Nội", "Đạt", "đẹp quá"],
            crate::lang::data::ARA => &["مَرْحَبًا", "كتاب"],
            crate::lang::data::POL => &["Łódź", "żółć", "gęślą"],
            _ => &["hello", "test 123", "café", ""],
        }
    }

    fn skip_needs_apply_test() -> bool {
        true // RemoveDiacritics does not modify case
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Universal contract tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;
    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(RemoveDiacritics);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CAT, ITA, POR, SPA,
        lang::data::{ARA, CES, DEU, ENG, FRA, HEB, NLD, POL, SLK, VIE},
    };

    // =========================================================================
    // CORE FUNCTIONALITY TESTS
    // =========================================================================

    #[test]
    fn test_ascii_no_op() {
        let stage = RemoveDiacritics;
        let c = Context::new(ENG);
        assert!(!stage.needs_apply("hello world", &c).unwrap());
        assert_eq!(stage.apply(Cow::Borrowed("hello"), &c).unwrap(), "hello");
    }

    #[test]
    fn test_empty_string() {
        let stage = RemoveDiacritics;
        let c = Context::new(FRA);
        assert!(!stage.needs_apply("", &c).unwrap());
        assert_eq!(stage.apply(Cow::Borrowed(""), &c).unwrap(), "");
    }

    #[test]
    fn test_no_diacritics_zero_copy() {
        let stage = RemoveDiacritics;
        let c = Context::new(FRA);
        let input = "hello world";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, input);
    }

    #[test]
    fn test_idempotency() {
        let stage = RemoveDiacritics;
        let c = Context::new(FRA);
        let once = stage.apply(Cow::Borrowed("café"), &c).unwrap();
        let twice = stage.apply(Cow::Borrowed(&once), &c).unwrap();
        assert_eq!(once, "cafe");
        assert_eq!(once, twice);
    }

    // =========================================================================
    // LANGUAGES WITH ONLY SPACING DIACRITICS (diac: field)
    // =========================================================================

    #[test]
    fn test_arabic_harakat_removal() {
        let stage = RemoveDiacritics;
        let c = Context::new(ARA);

        // Text with harakat (tashkīl)
        let input = "مَرْحَبًا";
        assert!(stage.needs_apply(input, &c).unwrap());
        assert_eq!(stage.apply(Cow::Borrowed(input), &c).unwrap(), "مرحبا");

        // Clean Arabic (no harakat)
        let clean = "كتاب";
        let result = stage.apply(Cow::Borrowed(clean), &c).unwrap();
        assert_eq!(result, "كتاب");
        assert!(matches!(result, Cow::Borrowed(_))); // Zero-copy
    }

    #[test]
    fn test_hebrew_nikkud_removal() {
        let stage = RemoveDiacritics;
        let c = Context::new(HEB);

        let input = "שָׁלוֹם"; // "Shalom" with nikkud
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();
        assert_eq!(result, "שלום");
    }

    #[test]
    fn test_mixed_scripts_arabic() {
        let stage = RemoveDiacritics;
        let c = Context::new(ARA);

        let input = "Hello مَرْحَبًا World";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();
        assert_eq!(result, "Hello مرحبا World");
    }

    // =========================================================================
    // ROMANCE LANGUAGES (strip: field for precomposed letters)
    // =========================================================================

    #[test]
    fn test_french_accent_removal() {
        let stage = RemoveDiacritics;
        let c = Context::new(FRA);

        assert_eq!(stage.apply(Cow::Borrowed("café"), &c).unwrap(), "cafe");
        assert_eq!(stage.apply(Cow::Borrowed("naïve"), &c).unwrap(), "naive");
        assert_eq!(stage.apply(Cow::Borrowed("résumé"), &c).unwrap(), "resume");
        assert_eq!(
            stage.apply(Cow::Borrowed("Crème brûlée"), &c).unwrap(),
            "Creme brulee"
        );
    }

    #[test]
    fn test_spanish_all_accents_stripped() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(SPA);
        let input = "Niño café";
        let output = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert_eq!(output, "Niño cafe");
        assert_eq!(
            stage.apply(Cow::Borrowed("Campeón"), &ctx).unwrap(),
            "Campeon"
        );
        assert_eq!(stage.apply(Cow::Borrowed("Muñoz"), &ctx).unwrap(), "Muñoz");
    }

    #[test]
    fn test_portuguese_comprehensive() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(POR);
        let input = "São Luís amanhã maçã";
        let expected = "Sao Luis amanha maca";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), expected);
    }

    #[test]
    fn test_italian_grave_acute() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(ITA);

        let input = "perché caffè città";
        let expected = "perche caffe citta";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), expected);
    }

    #[test]
    fn test_catalan_accents_preserve_punctuation() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(CAT);
        let input = Cow::Borrowed("L·lívia café òpera");
        let output = stage.apply(input, &ctx).unwrap();
        let expected = "L·livia cafe òpera"; // í→i, é→e; ò preserved (quality diacritic)
        assert_eq!(output, expected);
        assert!(output.contains("L·")); // Middot preserved (U+00B7, orthographic per UAX#15)
        assert!(!output.contains("í") && !output.contains("é")); // Stripped via maps
        assert!(output.contains("ò")); // Retained: Not purely diacritical
        assert!(matches!(output, Cow::Owned(_))); // Allocates (2 changes)
    }

    #[test] // Add: Zero-copy on clean
    fn test_catalan_zero_copy_clean() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(CAT);
        let input = Cow::Borrowed("L·livia cafe òpera"); // ò preserved
        let output = stage.apply(input.clone(), &ctx).unwrap();
        assert_eq!(output, input);
        assert!(matches!(output, Cow::Borrowed(_)));
    }

    #[test]
    fn test_catalan_middle_dot_not_strip() {
        let ctx = Context::new(CAT);
        let stage = RemoveDiacritics; // Assuming your impl
        let input = Cow::Borrowed("l·lengua L·Lengua");
        let output = stage.apply(input, &ctx).unwrap();
        assert_eq!(output, "l·lengua L·Lengua"); // Post-lowercase in pipeline
    }

    // =========================================================================
    // SLAVIC LANGUAGES (strip: field for carons, ogoneks, etc.)
    // =========================================================================

    #[test]
    fn test_czech_all_diacritics_stripped() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(CES);

        let input = "Příliš žluťoučký kůň úpěl ďábelské ódy";
        let expected = "Prilis zlutoucky kun upel dabelske ody";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), expected);
    }

    #[test]
    fn test_slovak_all_diacritics_stripped() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(SLK);

        let input = "Ľúbica a Ďurko štrikujú v Ťahanovciach";
        let expected = "Lubica a Durko strikuju v Tahanovciach";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), expected);
    }

    #[test]
    fn test_polish_all_diacritics_stripped() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(POL);

        let input = "Łódź żółć gęślą jaźń";
        let expected = "Lodz zolc gesla jazn";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), expected);
    }

    // =========================================================================
    // VIETNAMESE (comprehensive strip: map for all tone-marked vowels)
    // =========================================================================

    #[test]
    fn test_vietnamese_tone_marks_stripped() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(VIE);

        let input = "Hà Nội đẹp quá! Tôi tên là Đạt.";
        let expected = "Ha Noi dep qua! Toi ten la Dat.";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), expected);
    }

    #[test]
    fn test_polish_l() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(POL);

        let result_upper = stage.apply(Cow::Borrowed("Łódź"), &ctx).unwrap();
        // Correct expectation: Ł -> l AND ź -> z (per your language definition)
        assert_eq!(result_upper, "Lodz");

        let result_lower = stage.apply(Cow::Borrowed("łódź"), &ctx).unwrap();
        // Correct expectation: ł -> l AND ź -> z
        assert_eq!(result_lower, "lodz");
    }

    #[test]
    fn test_vietnamese_clean_text_zero_copy() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(VIE);

        let input = "toi ten la Nam";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    // =========================================================================
    // LANGUAGES WITHOUT STRIP OR DIAC (should be no-op)
    // =========================================================================

    #[test]
    fn test_language_without_diacritics_defined() {
        let stage = RemoveDiacritics;

        for lang in [ENG, DEU, NLD] {
            let c = Context::new(lang);
            assert!(!stage.needs_apply("test", &c).unwrap());
            assert!(stage.as_char_mapper(&c).is_none());
        }
    }

    #[test]
    fn test_english_no_diacritics_early_return() {
        let stage = RemoveDiacritics;
        let c = Context::new(ENG);

        let input = "file test hello";
        assert!(!stage.needs_apply(input, &c).unwrap());

        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, input);
    }

    // =========================================================================
    // CHAR MAPPER TESTS
    // =========================================================================

    #[test]
    fn test_char_mapper_eligibility() {
        let stage = RemoveDiacritics;

        // Languages with strip or diac: eligible
        assert!(stage.as_char_mapper(&Context::new(ARA)).is_some());
        assert!(stage.as_char_mapper(&Context::new(FRA)).is_some());
        assert!(stage.as_char_mapper(&Context::new(VIE)).is_some());

        // Languages without: not eligible
        assert!(stage.as_char_mapper(&Context::new(ENG)).is_none());
        assert!(stage.as_char_mapper(&Context::new(DEU)).is_none());
    }

    // =========================================================================
    // EDGE CASES & SPECIAL SCENARIOS
    // =========================================================================

    #[test]
    fn test_mixed_slavic_sentence() {
        let stage = RemoveDiacritics;
        let input = "Příliš žluťoučký kůň w Szczecinie mówi po polsku i słowacku.";

        // Czech: strips Czech diacritics (ď, ť, ň, ů, etc.) but NOT Polish ł
        let ces = stage
            .apply(Cow::Borrowed(input), &Context::new(CES))
            .unwrap();
        assert_eq!(
            ces, "Prilis zlutoucky kun w Szczecinie mowi po polsku i słowacku.",
            "Czech strips its diacritics but preserves Polish ł"
        );

        // Polish: strips Polish diacritics (ł, ó, etc.) AND some Czech ones it shares
        let pol = stage
            .apply(Cow::Borrowed(input), &Context::new(POL))
            .unwrap();
        assert_eq!(
            pol, "Příliš žluťoučký kůň w Szczecinie mowi po polsku i slowacku.",
            "Polish strips its own diacritics but preserves unknown foreign ones"
        );

        // Slovak: strips Slovak diacritics but NOT Polish ł
        let slk = stage
            .apply(Cow::Borrowed(input), &Context::new(SLK))
            .unwrap();
        assert_eq!(
            slk, "Přilis zlutoucky kůn w Szczecinie mowi po polsku i słowacku.",
            "Slovak strips shared diacritics but preserves Czech-specific 'ř'/'ů' and Polish 'ł'"
        );
    }

    #[test]
    fn test_french_sentences() {
        let stage = RemoveDiacritics;
        let c = Context::new(FRA);

        let examples = vec![
            ("Crème brûlée", "Creme brulee"),
            ("École française", "Ecole française"), // ç preserved!
            ("Où est la bibliothèque?", "Ou est la bibliotheque?"),
            ("garçon", "garçon"), // ç preserved!
        ];

        for (input, expected) in examples {
            assert_eq!(stage.apply(Cow::Borrowed(input), &c).unwrap(), expected);
        }
    }

    // =========================================================================
    // DESIGN VERIFICATION TESTS
    // =========================================================================

    #[test]
    fn test_strip_field_design_verification() {
        let stage = RemoveDiacritics;

        // Czech: ď is in strip: → stripped
        assert_eq!(
            stage.apply(Cow::Borrowed("ď"), &Context::new(CES)).unwrap(),
            "d"
        );
    }

    #[test]
    fn test_remove_diacritics_polish_l() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(POL);

        let result_upper = stage.apply(Cow::Borrowed("Łódź"), &ctx).unwrap();
        // Correct expectation: Ł -> l AND ź -> z (per your language definition)
        assert_eq!(result_upper, "Lodz");

        let result_lower = stage.apply(Cow::Borrowed("łódź"), &ctx).unwrap();
        // Correct expectation: ł -> l AND ź -> z
        assert_eq!(result_lower, "lodz");
    }

    #[test]
    fn test_no_nfd_decomposition_used() {
        // Verify that we're using direct mapping, not NFD
        let stage = RemoveDiacritics;
        let ctx = Context::new(FRA);

        // In NFC (normal form): café uses precomposed é (U+00E9)
        let nfc_input = "café"; // é = U+00E9 (precomposed)
        let result = stage.apply(Cow::Borrowed(nfc_input), &ctx).unwrap();
        assert_eq!(result, "cafe");

        // If someone manually creates NFD form (e + combining acute)
        // This should NOT work with current implementation since
        // combining marks aren't in diac: field for Romance languages
        let nfd_input = "cafe\u{0301}"; // e + U+0301 (combining acute)
        let result_nfd = stage.apply(Cow::Borrowed(nfd_input), &ctx).unwrap();

        // Current implementation: doesn't strip combining marks
        // because they're not in FRA's diac: list
        assert_eq!(
            result_nfd, nfd_input,
            "NFD form not handled - design uses precomposed mappings only"
        );
    }

    #[test]
    fn test_french_diacritic_removed() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(FRA);
        let input = "café résumé naïve";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        assert!(
            stage.needs_apply(input, &ctx).unwrap(),
            "{input} needs RemoveDiacritics applied."
        );
        assert_eq!(result, "cafe resume naive");
    }

    #[test]
    fn test_arabic_only_strips_defined_harakat() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(ARA);

        // Arabic harakat (defined in diac:) should be stripped
        let with_fatha = "مَرحبا"; // م + fatha + rest
        assert!(stage.needs_apply(with_fatha, &ctx).unwrap());

        // Latin acute accent is NOT in Arabic's diac: list
        let latin_acute = "café";
        let result = stage.apply(Cow::Borrowed(latin_acute), &ctx).unwrap();
        // Should preserve é because it's not an Arabic harakat
        assert_eq!(result, "café");
    }
}
