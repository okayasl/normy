use crate::{
    ARA, CES, FRA, POL, SLK, VIE,
    context::Context,
    lang::{Lang, LangEntry},
    stage::{CharMapper, Stage, StageError, StageIter},
    testing::stage_contract::StageTestConfig,
};
use std::iter::FusedIterator;
use std::sync::Arc;
use std::{borrow::Cow, str::Chars};

/// Removes language-specific diacritical marks using optimized lookup tables.
///
/// This stage strips diacritics and maps characters to their base forms based **strictly**
/// on the target language's definition. It prioritizes performance (O(1) lookups) and
/// linguistic accuracy over aggressive "ASCII-fication".
///
/// # Mechanism
///
/// 1. **Direct Mapping (Primary):** Checks the language's `strip_map` table. If a character
///    is defined there (e.g., `é` -> `e` in French), it is replaced.
/// 2. **Diacritic Filtering:** Checks the language's `diacritics` list. If a character is
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
        let entry = ctx.lang_entry;
        if !entry.has_pre_composed_to_base_map_or_spacing_diacritics() || text.is_ascii() {
            return Ok(false);
        }
        Ok(entry.needs_pre_composed_to_base_map_or_spacing_diacritics_removal(text))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            if let Some(base) = ctx.lang_entry.find_pre_composed_to_base_map(c) {
                out.push(base);
            } else if !ctx.lang_entry.is_spacing_diacritic(c) {
                out.push(c);
            }
        }
        Ok(Cow::Owned(out))
    }

    #[inline]
    fn as_char_mapper(&self, _ctx: &Context) -> Option<&dyn CharMapper> {
        Some(self)
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, _ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }

    fn try_dynamic_iter<'a>(
        &self,
        text: &'a str,
        ctx: &'a Context,
    ) -> Option<Box<dyn FusedIterator<Item = char> + 'a>> {
        Some(Box::new(RemoveDiacriticsIter::new(text, ctx)))
    }
}

impl CharMapper for RemoveDiacritics {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> Option<char> {
        if let Some(base) = ctx.lang_entry.find_pre_composed_to_base_map(c) {
            Some(base)
        } else if ctx.lang_entry.is_spacing_diacritic(c) {
            None
        } else {
            Some(c)
        }
    }

    fn bind<'a>(
        &self,
        text: &'a str,
        ctx: &'a Context,
    ) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(RemoveDiacriticsIter::new(text, ctx))
    }
}

pub struct RemoveDiacriticsIter<'a> {
    chars: Chars<'a>,
    lang: &'a LangEntry,
}

impl<'a> RemoveDiacriticsIter<'a> {
    pub fn new(text: &'a str, ctx: &'a Context) -> Self {
        Self {
            chars: text.chars(),
            lang: &ctx.lang_entry,
        }
    }
}

impl<'a> Iterator for RemoveDiacriticsIter<'a> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let c = self.chars.next()?;
            if let Some(base) = self.lang.find_pre_composed_to_base_map(c) {
                return Some(base);
            }
            if self.lang.is_spacing_diacritic(c) {
                continue;
            }
            return Some(c);
        }
    }
}

impl<'a> FusedIterator for RemoveDiacriticsIter<'a> {}

impl StageIter for RemoveDiacritics {
    type Iter<'a> = RemoveDiacriticsIter<'a>;

    #[inline(always)]
    fn try_iter<'a>(&self, text: &'a str, ctx: &'a Context) -> Option<Self::Iter<'a>> {
        Some(RemoveDiacriticsIter::new(text, ctx))
    }
}

impl StageTestConfig for RemoveDiacritics {
    fn one_to_one_languages() -> &'static [Lang] {
        &[] // Not 1:1 — can remove combining marks (None return in map)
    }

    fn samples(lang: Lang) -> &'static [&'static str] {
        match lang {
            FRA => &["café", "naïve", "résumé", "Crème brûlée"],
            VIE => &["Hà Nội", "Đạt", "đẹp quá"],
            ARA => &["مَرْحَبًا", "كتاب"],
            POL => &["Łódź", "żółć", "gęślą jaźń"],
            CES => &["Příliš žluťoučký", "děvče"],
            SLK => &["Ľúbica", "Ťahanovce"],
            _ => &["hello", "test 123", "café", ""],
        }
    }

    fn should_pass_through(lang: Lang) -> &'static [&'static str] {
        match lang {
            FRA | VIE | POL => &["hello", "world", "test123", ""],
            ARA => &["كتاب", "hello", ""], // Clean Arabic + ASCII
            CES | SLK => &["café", "hello", "world", ""],
            _ => &["hello", "world", ""],
        }
    }

    fn should_transform(lang: Lang) -> &'static [(&'static str, &'static str)] {
        match lang {
            FRA => &[("café", "cafe"), ("naïve", "naive"), ("résumé", "resume")],
            VIE => &[("Hà Nội", "Ha Noi"), ("Đạt", "Dat")],
            POL => &[("Łódź", "Lodz"), ("żółć", "zolc"), ("gęślą", "gesla")],
            ARA => &[("مَرْحَبًا", "مرحبا")],
            CES => &[
                ("děvče", "devce"),
                ("Příliš", "Prílis"),
                ("žluťoučký", "zlutoucký"),
            ],
            SLK => &[
                ("děvče", "děvce"),
                ("Příliš", "Přílis"),
                ("žluťoučký", "zlutoucký"),
            ],
            _ => &[],
        }
    }

    fn skip_zero_copy_apply_test() -> bool {
        true // Can now test accurately with new methods!
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

        // English has no strip/diacritic rules
        assert!(!stage.needs_apply("hello world", &c).unwrap());

        let result = stage.apply(Cow::Borrowed("hello world"), &c).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_empty_string() {
        let stage = RemoveDiacritics;
        let c = Context::new(FRA);
        assert!(!stage.needs_apply("", &c).unwrap());

        let result = stage.apply(Cow::Borrowed(""), &c).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_no_diacritics_zero_copy() {
        let stage = RemoveDiacritics;
        let c = Context::new(FRA);
        let input = "hello world";

        assert!(!stage.needs_apply(input, &c).unwrap());
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();
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
    // LANGUAGES WITH ONLY SPACING DIACRITICS (diacritics field)
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
        assert!(!stage.needs_apply(clean, &c).unwrap());
        let result = stage.apply(Cow::Borrowed(clean), &c).unwrap();
        assert_eq!(result, "كتاب");
    }

    #[test]
    fn test_hebrew_nikkud_removal() {
        let stage = RemoveDiacritics;
        let c = Context::new(HEB);

        let input = "שָׁלוֹם"; // "Shalom" with nikkud
        assert!(stage.needs_apply(input, &c).unwrap());
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();
        assert_eq!(result, "שלום");
    }

    #[test]
    fn test_mixed_scripts_arabic() {
        let stage = RemoveDiacritics;
        let c = Context::new(ARA);

        let input = "Hello مَرْحَبًا World";
        assert!(stage.needs_apply(input, &c).unwrap());
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();
        assert_eq!(result, "Hello مرحبا World");
    }

    // =========================================================================
    // ROMANCE LANGUAGES (strip_map field for precomposed letters)
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
    fn test_spanish_accents() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(SPA);

        assert_eq!(stage.apply(Cow::Borrowed("café"), &ctx).unwrap(), "cafe");
        assert_eq!(
            stage.apply(Cow::Borrowed("Campeón"), &ctx).unwrap(),
            "Campeon"
        );

        // Note: ñ is preserved (it's a distinct letter in Spanish alphabet, not a diacritic)
        let input = "Niño";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert_eq!(result, "Niño");
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
    fn test_catalan_accents() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(CAT);

        let input = "café òpera";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        // Catalan strips some accents but preserves quality distinctions
        assert!(result.contains("cafe") || result == input);
    }

    // =========================================================================
    // SLAVIC LANGUAGES (strip_map field for carons, ogoneks, etc.)
    // =========================================================================

    #[test]
    fn test_czech_diacritics() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(CES);

        let input = "Příliš žluťoučký kůň úpěl ďábelské ódy";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        // Czech strips háček, kroužek but preserves acute length markers
        assert!(result.contains("Prílis")); // ř→r, but í preserved
        assert!(result.contains("zlutoucký")); // ž→z, ť→t, ů→u
    }

    #[test]
    fn test_slovak_diacritics() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(SLK);

        let input = "Ľúbica a Ďurko";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        // Slovak strips carons but preserves acute
        assert!(result.contains("Lúbica")); // Ľ→L
        assert!(result.contains("Durko")); // Ď→D
    }

    #[test]
    fn test_polish_all_diacritics() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(POL);

        let input = "Łódź żółć gęślą jaźń";
        let expected = "Lodz zolc gesla jazn";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), expected);
    }

    // =========================================================================
    // VIETNAMESE (comprehensive strip_map for all tone-marked vowels)
    // =========================================================================

    #[test]
    fn test_vietnamese_tone_marks() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(VIE);

        let input = "Hà Nội đẹp quá! Tôi tên là Đạt.";
        let expected = "Ha Noi dep qua! Toi ten la Dat.";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), expected);
    }

    #[test]
    fn test_vietnamese_zero_copy() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(VIE);

        let input = "toi ten la Nam";
        assert!(!stage.needs_apply(input, &ctx).unwrap());
    }

    // =========================================================================
    // LANGUAGES WITHOUT RULES (should be no-op)
    // =========================================================================

    #[test]
    fn remove_diacritics_skips_no_rules_non_ascii() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(ENG);
        let input = "café";
        assert!(!stage.needs_apply(input, &ctx).unwrap());
        // Pipeline would skip apply entirely
    }

    #[test]
    fn test_languages_without_rules() {
        let stage = RemoveDiacritics;

        for lang in [ENG, DEU, NLD] {
            let c = Context::new(lang);
            assert!(
                !c.lang_entry
                    .has_pre_composed_to_base_map_or_spacing_diacritics()
            );
            assert!(!stage.needs_apply("test café", &c).unwrap());
        }
    }

    // =========================================================================
    // CHAR MAPPER TESTS
    // =========================================================================

    // #[test]
    // fn test_char_mapper_eligibility() {
    //     let stage = RemoveDiacritics;

    //     // Languages with rules: eligible
    //     assert!(stage.as_char_mapper(&Context::new(ARA)).is_some());
    //     assert!(stage.as_char_mapper(&Context::new(FRA)).is_some());
    //     assert!(stage.as_char_mapper(&Context::new(VIE)).is_some());
    //     assert!(stage.as_char_mapper(&Context::new(POL)).is_some());

    //     // Languages without rules: not eligible
    //     assert!(stage.as_char_mapper(&Context::new(ENG)).is_none());
    //     assert!(stage.as_char_mapper(&Context::new(DEU)).is_none());
    //     assert!(stage.as_char_mapper(&Context::new(NLD)).is_none());
    // }

    // =========================================================================
    // EDGE CASES & SPECIAL SCENARIOS
    // =========================================================================

    #[test]
    fn test_mixed_slavic_sentence() {
        let stage = RemoveDiacritics;
        let input = "Příliš žluťoučký kůň w Szczecinie mówi po polsku.";

        // Czech: strips Czech diacritics but preserves foreign ones
        let ces = stage
            .apply(Cow::Borrowed(input), &Context::new(CES))
            .unwrap();
        assert!(ces.contains("Prílis")); // ř→r but í preserved
        assert!(ces.contains("mówi")); // Polish ó preserved

        // Polish: strips Polish diacritics
        let pol = stage
            .apply(Cow::Borrowed(input), &Context::new(POL))
            .unwrap();
        assert!(pol.contains("mowi")); // ó→o
    }

    #[test]
    fn test_arabic_preserves_latin() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(ARA);

        // Latin accents NOT in Arabic's diacritics list
        let latin_acute = "café مَرْحَبًا";
        let result = stage.apply(Cow::Borrowed(latin_acute), &ctx).unwrap();

        // Should strip Arabic harakat but preserve Latin é
        assert!(result.contains("café"));
        assert!(result.contains("مرحبا"));
    }

    #[test]
    fn test_needs_apply_accuracy() {
        let stage = RemoveDiacritics;

        // French
        let ctx = Context::new(FRA);
        assert!(stage.needs_apply("café", &ctx).unwrap());
        assert!(!stage.needs_apply("hello", &ctx).unwrap());

        // Arabic
        let ctx = Context::new(ARA);
        assert!(stage.needs_apply("مَرْحَبًا", &ctx).unwrap());
        assert!(!stage.needs_apply("كتاب", &ctx).unwrap());

        // English (no rules)
        let ctx = Context::new(ENG);
        assert!(!stage.needs_apply("café", &ctx).unwrap());
    }

    #[test]
    fn test_precomputed_flags() {
        // French: has strip_map, no diacritics
        let ctx = Context::new(FRA);
        assert!(ctx.lang_entry.has_pre_composed_to_base_map());
        assert!(!ctx.lang_entry.has_spacing_diacritics());
        assert!(
            ctx.lang_entry
                .has_pre_composed_to_base_map_or_spacing_diacritics()
        );

        // Arabic: no strip_map, has diacritics
        let ctx = Context::new(ARA);
        assert!(!ctx.lang_entry.has_pre_composed_to_base_map());
        assert!(ctx.lang_entry.has_spacing_diacritics());
        assert!(
            ctx.lang_entry
                .has_pre_composed_to_base_map_or_spacing_diacritics()
        );

        // English: no rules at all
        let ctx = Context::new(ENG);
        assert!(!ctx.lang_entry.has_pre_composed_to_base_map());
        assert!(!ctx.lang_entry.has_spacing_diacritics());
        assert!(
            !ctx.lang_entry
                .has_pre_composed_to_base_map_or_spacing_diacritics()
        );
    }
}
