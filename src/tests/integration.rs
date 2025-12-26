#[cfg(test)]
mod integration_tests {

    use crate::{
        ARA, COLLAPSE_WHITESPACE, CaseFold, DEU, JPN, LowerCase, NLD, Normy, SegmentWords,
        TRIM_WHITESPACE, TUR, ZHO,
        stage::{
            normalize_punctuation::NormalizePunctuation, remove_diacritics::RemoveDiacritics,
            strip_control_chars::StripControlChars, unify_width::UnifyWidth,
        },
    };

    #[test]
    fn production_pipeline_turkish() {
        let normy = Normy::builder()
            .lang(TUR)
            .add_stage(TRIM_WHITESPACE)
            .add_stage(LowerCase)
            .build();

        let input = " İSTANBUL  ";
        let result = normy.normalize(input).unwrap();
        assert_eq!(result, "istanbul");
    }

    #[test]
    fn production_pipeline_fused_turkish() {
        let normy = Normy::builder()
            .lang(TUR)
            .add_stage(TRIM_WHITESPACE)
            .add_stage(LowerCase)
            .build();

        let input = " İSTANBUL  ";
        let result = normy.normalize(input).unwrap();
        assert_eq!(result, "istanbul");
    }

    #[test]
    fn production_pipeline_fused_german() {
        let normy = Normy::builder().lang(DEU).add_stage(CaseFold).build();

        let input = " Fußball Maßstab Straße ";
        let result = normy.normalize(input).unwrap();
        assert_eq!(result, " fussball massstab strasse ");
    }

    #[test]
    fn production_pipeline_arabic_diacritics() {
        let normy = Normy::builder()
            .lang(ARA)
            .add_stage(TRIM_WHITESPACE)
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
            .add_stage(TRIM_WHITESPACE)
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
    fn arabic_diacritics_fused_missing() {
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
    fn dutch_case_fold_is_canonical() {
        let normy = Normy::builder().lang(NLD).add_stage(CaseFold).build();

        let out = normy.normalize("IJsselmeer").unwrap();
        assert_eq!(out, "ijsselmeer"); // ← CORRECT
    }

    #[test]
    fn whitespace_triming_works() {
        let normy = Normy::builder()
            .lang(TUR)
            .add_stage(COLLAPSE_WHITESPACE)
            .add_stage(LowerCase)
            .build();

        let input = " İSTANBUL   aCILAR ";
        let result = normy.normalize(input).unwrap();
        assert_eq!(result, " istanbul acılar ");
    }

    #[test]
    fn test_replace_fullwidth() {
        let normy = Normy::builder().add_stage(UnifyWidth).build();
        let text = "Ｈｅｌｌｏ　Ｗｏｒｌｄ！";
        let normalized = normy.normalize(text).unwrap();
        assert_eq!(normalized, "Hello World!");
    }

    #[test]
    fn test_normalize_punctuation() {
        let normy = Normy::builder().add_stage(NormalizePunctuation).build();
        let text = "“Hello” – said ‘John’…";
        let normalized = normy.normalize(text).unwrap();
        assert_eq!(normalized, "\"Hello\" - said 'John'.");
    }

    #[test]
    fn test_remove_control_chars() {
        let text = "Hello\x07\x1Bworld\x7F";
        let normy = Normy::builder().add_stage(StripControlChars).build();
        let normalized = normy.normalize(text).unwrap();
        assert_eq!(normalized, "Helloworld");
    }

    #[test]
    fn test_unigram_cjk_opt_in_for_japanese_works() {
        use crate::Normy;

        let normy = Normy::builder()
            .lang(JPN)
            .modify_lang(|lang| lang.set_unigram_cjk(true))
            .add_stage(SegmentWords)
            .build();

        let text = "最高の言語";
        let result = normy.normalize(text).unwrap();
        assert_eq!(&*result, "最 高 の 言 語");
    }

    #[test]
    fn test_unigram_cjk_off_for_chinese_works() {
        use crate::Normy;

        let normy = Normy::builder()
            .lang(ZHO)
            .modify_lang(|lang| lang.set_unigram_cjk(false))
            .add_stage(SegmentWords)
            .build();

        let text = "Hello中华人民共和国";
        let result = normy.normalize(text).unwrap();
        assert_eq!(&*result, "Hello 中华人民共和国");
    }

    // #[test]
    // fn test_remove_html() {
    //     let text = "Hello <b>world</b> <script>alert(1)</script>";
    //     let normalized = Normy::builder().add_stage(RemoveHtml)
    //         .build()
    //         .normalize_with_stage(RemoveHtml, text)
    //         .unwrap();
    //     assert_eq!(normalized, "Hello world ");
    // }
}
