use crate::{
    ARA, CES, FRA, POL, SLK, VIE,
    context::Context,
    lang::{Lang, LangEntry},
    stage::{Stage, StageError, StaticFusableStage},
    testing::stage_contract::StageTestConfig,
};
use std::borrow::Cow;
use std::iter::FusedIterator;

/// Removes language-specific diacritical marks using optimized lookup tables.
///
/// This stage strips diacritics and maps characters to their base forms based **strictly**
/// on the target language's definition. It prioritizes performance (O(1) lookups) and
/// linguistic accuracy over aggressive "ASCII-fication".
///
/// # Mechanism
///
/// 1. **Direct Mapping:** Uses language's `pre_composed_to_base_map` (e.g., `é` → `e` in French)
/// 2. **Diacritic Removal:** Skips characters in `spacing_diacritics` (e.g., Arabic harakat)
/// 3. **Preservation:** All other characters unchanged
///
/// # Design Philosophy
///
/// Strict locale behavior:
/// - Polish strips `ł` → `l` but preserves Czech `ř`
/// - Spanish preserves `ñ` (distinct letter)
/// - Foreign scripts untouched
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
}

impl StaticFusableStage for RemoveDiacritics {
    type Adapter<'a, I>
        = RemoveDiacriticsAdapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    #[inline(always)]
    fn supports_static_fusion(&self) -> bool {
        true
    }

    #[inline(always)]
    fn static_fused_adapter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::Adapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        RemoveDiacriticsAdapter {
            input,
            lang: &ctx.lang_entry,
        }
    }
}

pub struct RemoveDiacriticsAdapter<'a, I> {
    input: I,
    lang: &'a LangEntry,
}

impl<'a, I: Iterator<Item = char>> Iterator for RemoveDiacriticsAdapter<'a, I> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let c = self.input.next()?;

            // 1. Try 1:1 mapping (é -> e)
            if let Some(base) = self.lang.find_pre_composed_to_base_map(c) {
                return Some(base);
            }

            // 2. Skip if it's a standalone spacing diacritic
            if self.lang.is_spacing_diacritic(c) {
                continue;
            }

            // 3. Return as-is
            return Some(c);
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.input.size_hint();
        // Lower is 0 because spacing diacritics can be removed entirely
        (0, upper)
    }
}

impl<'a, I: FusedIterator<Item = char>> FusedIterator for RemoveDiacriticsAdapter<'a, I> {}

impl StageTestConfig for RemoveDiacritics {
    fn one_to_one_languages() -> &'static [Lang] {
        &[]
    }

    fn samples(lang: Lang) -> &'static [&'static str] {
        match lang {
            FRA => &["café", "naïve", "résumé", "Crème brûlée"],
            VIE => &["Hà Nội", "Đạt", "đẹp quá", "tôi tên là"],
            ARA => &["مَرْحَبًا", "كتاب", "الْعَرَبِيَّةُ"],
            POL => &["Łódź", "żółć", "gęślą jaźń"],
            CES => &["Příliš žluťoučký", "děvče"],
            SLK => &["Ľúbica", "Ťahanovce"],
            _ => &["hello", "test 123", "café", ""],
        }
    }

    fn should_pass_through(lang: Lang) -> &'static [&'static str] {
        match lang {
            FRA | VIE | POL => &["hello", "world", "test123", ""],
            ARA => &["كتاب", "hello", ""],
            CES | SLK => &["hello", "world", "café", ""],
            _ => &["hello", "world", ""],
        }
    }

    fn should_transform(lang: Lang) -> &'static [(&'static str, &'static str)] {
        match lang {
            FRA => &[
                ("café", "cafe"),
                ("naïve", "naive"),
                ("résumé", "resume"),
                ("Crème brûlée", "Creme brulee"),
            ],
            VIE => &[("Hà Nội", "Ha Noi"), ("Đạt", "Dat"), ("đẹp quá", "dep qua")],
            ARA => &[
                ("مَرْحَبًا", "مرحبا"),   // tanwin, fatha removed
                ("الْكِتَابُ", "الكتاب"), // shadda on ت preserved, damma removed
                ("قُرْآنٌ", "قرآن"),     // shadda + sukun + damma + tanwin → only optional marks gone
            ],
            POL => &[
                ("Łódź", "Lodz"),
                ("żółć", "zolc"),
                ("gęślą jaźń", "gesla jazn"),
            ],
            CES => &[
                ("děvče", "devce"),
                ("Příliš", "Prílis"), // Acute preserved — correct
                ("žluťoučký", "zlutoucký"),
            ],
            SLK => &[
                ("Ľúbica", "Lúbica"), // Ľ→L, caron stripped; ú preserved
                ("Ťahanovce", "Tahanovce"),
            ],
            _ => &[],
        }
    }
}

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
        HEB, ITA, POR, SPA,
        context::Context,
        lang::data::{ARA, CES, ENG, FRA, POL},
    };
    use std::borrow::Cow;

    #[test]
    fn test_language_isolation_slavic() {
        // Czech and Polish have different diacritic rules
        // Each should only apply its own rules, preserving foreign characters
        let stage = RemoveDiacritics;
        let input = "Příliš žluťoučký kůň w Szczecinie mówi po polsku.";

        // Czech context: strips Czech diacritics but preserves Polish ones
        let ces_ctx = Context::new(CES);
        let ces_result = stage.apply(Cow::Borrowed(input), &ces_ctx).unwrap();
        assert!(
            ces_result.contains("Prílis"),
            "Czech should strip ř→r but preserve í"
        );
        assert!(
            ces_result.contains("mówi"),
            "Czech should preserve Polish ó (not in Czech rules)"
        );

        // Polish context: strips Polish diacritics
        let pol_ctx = Context::new(POL);
        let pol_result = stage.apply(Cow::Borrowed(input), &pol_ctx).unwrap();
        assert!(pol_result.contains("mowi"), "Polish should strip ó→o");
    }

    #[test]
    fn test_language_isolation_arabic_latin() {
        // Arabic rules should strip Arabic diacritics but preserve Latin accents
        let stage = RemoveDiacritics;
        let ctx = Context::new(ARA);

        let input = "café مَرْحَبًا résumé";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        // Arabic harakat stripped
        assert!(
            result.contains("مرحبا"),
            "Arabic diacritics should be stripped"
        );

        // Latin accents preserved (not in Arabic diacritic rules)
        assert!(
            result.contains("café"),
            "Latin accents should be preserved in Arabic context"
        );
        assert!(
            result.contains("résumé"),
            "Latin accents should be preserved in Arabic context"
        );
    }

    #[test]
    fn test_spanish_distinct_letter_preservation() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(SPA);

        // ñ is a DISTINCT LETTER in Spanish alphabet, not a diacritic
        let input = "Niño";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert_eq!(
            result, "Niño",
            "Spanish ñ should be preserved (distinct letter, not diacritic)"
        );

        // But actual diacritics are stripped
        assert_eq!(
            stage.apply(Cow::Borrowed("café"), &ctx).unwrap(),
            "cafe",
            "Spanish acute accents should be stripped"
        );
        assert_eq!(
            stage.apply(Cow::Borrowed("Campeón"), &ctx).unwrap(),
            "Campeon",
            "Spanish accents should be stripped"
        );
    }

    #[test]
    fn test_hebrew_nikkud_removal() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(HEB);

        // Hebrew with nikkud (vowel points)
        let input = "שָׁלוֹם"; // "Shalom" with nikkud
        assert!(
            stage.needs_apply(input, &ctx).unwrap(),
            "Hebrew with nikkud should trigger needs_apply"
        );

        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert_eq!(result, "שלום", "Hebrew nikkud should be removed");

        // Clean Hebrew (no nikkud)
        let clean = "שלום";
        assert!(
            !stage.needs_apply(clean, &ctx).unwrap(),
            "Clean Hebrew should not trigger needs_apply"
        );
    }

    #[test]
    fn test_mixed_scripts_arabic() {
        // Validates that Latin text passes through unchanged in Arabic context
        let stage = RemoveDiacritics;
        let ctx = Context::new(ARA);

        let input = "Hello مَرْحَبًا World";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        assert_eq!(
            result, "Hello مرحبا World",
            "Latin text should pass through, Arabic diacritics stripped"
        );
    }

    #[test]
    fn test_languages_without_rules_skip() {
        let stage = RemoveDiacritics;

        // English, German, Dutch have no diacritic removal rules
        for &lang in &[ENG, crate::DEU, crate::NLD] {
            let ctx = Context::new(lang);

            // Should not have rules
            assert!(
                !ctx.lang_entry
                    .has_pre_composed_to_base_map_or_spacing_diacritics(),
                "{:?} should not have diacritic rules",
                lang
            );

            // Should not apply even to accented text
            assert!(
                !stage.needs_apply("test café résumé", &ctx).unwrap(),
                "{:?} should skip diacritic removal",
                lang
            );
        }
    }

    #[test]
    fn test_portuguese_comprehensive() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(POR);

        let input = "São Luís amanhã maçã";
        let expected = "Sao Luis amanha maca";

        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx).unwrap(),
            expected,
            "Portuguese diacritics should be fully stripped"
        );
    }

    #[test]
    fn test_italian_grave_acute() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(ITA);

        let input = "perché caffè città";
        let expected = "perche caffe citta";

        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx).unwrap(),
            expected,
            "Italian grave and acute accents should be stripped"
        );
    }

    #[test]
    fn test_empty_string() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(FRA);

        assert!(!stage.needs_apply("", &ctx).unwrap());
        assert_eq!(stage.apply(Cow::Borrowed(""), &ctx).unwrap(), "");
    }

    #[test]
    fn test_needs_apply_accuracy() {
        let stage = RemoveDiacritics;

        // French: should detect accents
        let fra = Context::new(FRA);
        assert!(stage.needs_apply("café", &fra).unwrap());
        assert!(!stage.needs_apply("hello", &fra).unwrap());

        // Arabic: should detect harakat
        let ara = Context::new(ARA);
        assert!(stage.needs_apply("مَرْحَبًا", &ara).unwrap());
        assert!(!stage.needs_apply("كتاب", &ara).unwrap());

        // English: no rules, never applies
        let eng = Context::new(ENG);
        assert!(!stage.needs_apply("café", &eng).unwrap());
    }
}
