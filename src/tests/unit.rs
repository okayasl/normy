#[cfg(test)]
mod unit_tests {

    use crate::{
        DEU, ENG, FRA, CaseFold, JPN, LowerCase, NLD, Normy, RemoveDiacritics, TUR,
        stage::normalize_whitespace::NormalizeWhitespace,
    };
    use std::borrow::Cow;
    #[test]
    fn ascii_fast_path() {
        let normy = Normy::builder().lang(ENG).add_stage(LowerCase).build();
        let input = "HELLO WORLD";
        let result = normy.normalize(input).unwrap();
        assert!(matches!(result, Cow::Owned(_)));
        assert_eq!(result, "hello world");
    }

    #[test]
    fn turkish_i() {
        let normy = Normy::builder().lang(TUR).add_stage(LowerCase).build();
        assert_eq!(normy.normalize("İSTANBUL").unwrap(), "istanbul");
        assert_eq!(normy.normalize("IĞDIR").unwrap(), "ığdır");
    }

    #[test]
    fn zero_copy_when_already_lower() {
        let normy = Normy::builder().lang(ENG).add_stage(LowerCase).build();
        let input = "already lower";
        let result = normy.normalize(input).unwrap();
        assert!(matches!(result, Cow::Borrowed(s) if s.as_ptr() == input.as_ptr()));
    }

    #[test]
    fn german_sharp_s() {
        let normy = Normy::builder().lang(DEU).add_stage(CaseFold).build();
        assert_eq!(normy.normalize("Weißstraße").unwrap(), "weissstrasse");
        assert_eq!(normy.normalize("GROß").unwrap(), "gross");
    }

    #[test]
    fn turkish_case_fold() {
        let normy = Normy::builder().lang(TUR).add_stage(CaseFold).build();
        assert_eq!(normy.normalize("İ").unwrap(), "i");
        assert_eq!(normy.normalize("I").unwrap(), "ı");
    }

    #[test]
    fn french_diacritic_removed() {
        let n = Normy::builder()
            .lang(FRA)
            .add_stage(RemoveDiacritics)
            .build();
        let result = n.normalize("café résumé naïve").unwrap();
        assert_eq!(result,"cafe resume naive");
        // Edge: composed → decomposed → stripped
        assert_eq!(n.normalize(" naïve").unwrap(), " naive"); // U+00E9 → e + ́ → e
    }

    #[test]
    fn length_expansion() {
        let normy = Normy::builder().lang(DEU).add_stage(CaseFold).build();
        let input = "ß";
        let result = normy.normalize(input).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result, "ss");
    }

    #[test]
    fn zero_copy_no_whitespace() {
        let normy = Normy::builder()
            .lang(ENG)
            .add_stage(NormalizeWhitespace::trim_only())
            .build();
        let input = "hello";
        let result = normy.normalize(input).unwrap();
        assert!(matches!(result, Cow::Borrowed(s) if s.as_ptr() == input.as_ptr()));
    }

    #[test]
    fn trims_all_ascii_whitespace() {
        let normy = Normy::builder()
            .lang(ENG)
            .add_stage(NormalizeWhitespace::trim_only())
            .build();
        assert_eq!(
            normy.normalize(" \t\n hello \r\n pop ").unwrap(),
            "hello \r\n pop"
        );
    }

    #[test]
    fn full_width_spaces_japanese() {
        let normy = Normy::builder()
            .lang(JPN)
            .add_stage(NormalizeWhitespace::trim_only())
            .build();
        assert_eq!(normy.normalize("　こんにちは　").unwrap(), "こんにちは");
    }

    #[test]
    fn valid_utf8_no_alloc() {
        let normy = Normy::builder().lang(ENG).build();
        let input = "hello 世界";
        let result = normy.normalize(input).unwrap();
        assert!(matches!(result, Cow::Borrowed(s) if s.as_ptr() == input.as_ptr()));
    }

    #[test]
    fn simd_enabled_by_default() {
        let normy = Normy::builder().lang(ENG).build();
        let input = "a".repeat(1000);
        let result = normy.normalize(&input).unwrap();
        assert_eq!(result, input);
    }

    #[test]
    fn case_fold_idempotent() {
        let normy = Normy::builder().lang(DEU).add_stage(CaseFold).build();
        let s = "ẞ";
        let once = normy.normalize(s).unwrap().into_owned();
        let twice = normy.normalize(&once).unwrap().into_owned();
        assert_eq!(once, twice);
    }

    #[test]
    fn dutch_ij_sequence() {
        let n = Normy::builder().lang(NLD).add_stage(CaseFold).build();
        let cases = ["IJssel", "IJsland", "IJmuiden", "ijsselmeer"];
        for s in cases {
            let once = n.normalize(s).unwrap().into_owned();
            let twice = n.normalize(&once).unwrap().into_owned();
            assert_eq!(once, twice);
            assert!(once.contains("ij")); // always the digraph
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // 1. Turkish 1-to-1 mapping must use CharMapper → zero allocation
    // ─────────────────────────────────────────────────────────────────────────────
    #[test]
    fn turkish_zero_alloc_when_already_lower() {
        use crate::{TUR, normy::Normy, stage::case_fold::CaseFold};

        // Build a pipeline that only contains CaseFold (no other stages)
        let normy = Normy::builder().lang(TUR).add_stage(CaseFold).build();

        // Input is already lower-case Turkish text → no change required
        let input = "istanbul";

        // The result **must** be `Cow::Borrowed` (no heap allocation)
        let result = normy.normalize(input).unwrap();
        assert!(
            matches!(result, std::borrow::Cow::Borrowed(_)),
            "Turkish 1-to-1 case-fold should be zero-copy, got Owned"
        );

        // Also verify correctness
        assert_eq!(result.as_ref(), "istanbul");
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // 2. Dutch “IJ” two-character sequence must be handled with peek-ahead
    // ─────────────────────────────────────────────────────────────────────────────
    fn count_ij_digraphs(s: &str) -> usize {
        let mut count = 0;
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if (c == 'I' || c == 'i') && chars.peek() == Some(&'J') || chars.peek() == Some(&'j') {
                count += 1;
                chars.next(); // Consume the 'J'
            }
        }
        count
    }

    #[test]
    fn dutch_ij_sequence_is_idempotent_and_correct() {
        let normy = Normy::builder().lang(NLD).add_stage(CaseFold).build();

        let cases = ["IJssel", "IJsland", "IJmuiden", "ijsselmeer", "IJzer"];

        for &word in &cases {
            let once = normy.normalize(word).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();

            assert_eq!(
                once, twice,
                "Dutch case-fold is not idempotent for `{}`: `{}` → `{}`",
                word, once, twice
            );

            // Count IJ digraphs in original and result
            let original_digraphs = count_ij_digraphs(word);
            let result_digraphs = once.matches("ij").count();

            assert_eq!(
                result_digraphs, original_digraphs,
                "Dutch `{}` should contain exactly {} `ij` digraph(s), got {}",
                word, original_digraphs, result_digraphs
            );
        }
    }

    #[test]
    fn test_turkish_dotted_i() {
        let normy = Normy::builder().lang(TUR).add_stage(CaseFold).build();
        assert_eq!(normy.normalize("İSTANBUL").unwrap(), "istanbul");
        assert_eq!(normy.normalize("ISPARTA").unwrap(), "ısparta");
    }
}
