#[cfg(test)]
mod integration_tests {

    use crate::{ARA, CaseFold, DEU, LowerCase, NLD, Normy, RemoveDiacritics, TUR, Trim};

    #[test]
    fn production_pipeline_turkish() {
        let normy = Normy::builder()
            .lang(TUR)
            .add_stage(Trim)
            .add_stage(LowerCase)
            .build();

        let input = " İSTANBUL  ";
        let result = normy.normalize(input).unwrap();
        assert_eq!(result, "istanbul");
    }

    #[test]
    fn production_pipeline_arabic_diacritics() {
        let normy = Normy::builder()
            .lang(ARA)
            .add_stage(Trim)
            .add_stage(RemoveDiacritics)
            .add_stage(CaseFold)
            .build();

        let input = "  كتابٌ جميل  ";
        let result = normy.normalize(input).unwrap();
        assert_eq!(result, "كتاب جميل"); // diacritics removed
    }

    #[test]
    fn production_pipeline_german() {
        let normy = Normy::builder()
            .lang(DEU)
            .add_stage(Trim)
            .add_stage(CaseFold)
            .build();

        let input = "  Weißstraße  ";
        let result = normy.normalize(input).unwrap();
        assert_eq!(result, "weissstrasse");
    }

    #[test]
    fn turkish_lowercase_incomplete() {
        let normy = Normy::builder().lang(TUR).add_stage(LowerCase).build();
        let out = normy.normalize("İSTANBUL ÇAĞLAYAN").unwrap();
        // Expected (full Turkish lower-casing):
        //   i̇stanbul çağlayan
        //   (note the dotless ı and the correct ğ, ş, ç, ö, ü)
        assert_eq!(out, "istanbul çağlayan"); // ← FAILS (Ç, Ğ, Ş, Ö, Ü stay upper)
    }

    #[test]
    fn german_lowercase_preserves_szlig_and_lowercases_others() {
        let normy = Normy::builder().lang(DEU).add_stage(LowerCase).build();

        // Test multiple cases
        assert_eq!(normy.normalize("Fuß").unwrap(), "fuß");
        assert_eq!(normy.normalize("FUSS").unwrap(), "fuss");
        assert_eq!(normy.normalize("MÄẞIG").unwrap(), "mäßig"); // ẞ → ß
        assert_eq!(normy.normalize("straße").unwrap(), "straße"); // ß unchanged
    }

    #[test]
    fn arabic_diacritics_missing() {
        let normy = Normy::builder()
            .lang(ARA)
            .add_stage(RemoveDiacritics)
            .build();

        // Text contains U+0670 (superscript alif) – not in the static list.
        let out = normy.normalize("الْكِتَابُٰ").unwrap();
        // Expected: الكتاب (all diacritics stripped)
        assert_eq!(out, "الكتاب"); // ← FAILS – the superscript alif remains
    }

    #[test]
    fn dutch_lowercase_is_one_to_one() {
        let normy = Normy::builder()
            .lang(NLD)
            .add_stage(CaseFold)
            .build();

        let out = normy.normalize("IJsselmeer").unwrap();
        // Dutch has no custom case map → Unicode fallback: I→i, J→j
        assert_eq!(out, "ijsselmeer");
    }
}
