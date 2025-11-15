#[cfg(test)]
mod unit_tests {

    use crate::{CaseFold, DEU, ENG,NLD, JPN, Lowercase, Normy, TUR, TrimWhitespace};
    use std::borrow::Cow;
    #[test]
    fn ascii_fast_path() {
        let normy = Normy::builder().lang(ENG).add_stage(Lowercase).build();
        let input = "HELLO WORLD";
        let result = normy.normalize(input).unwrap();
        assert!(matches!(result, Cow::Owned(_)));
        assert_eq!(result, "hello world");
    }

    #[test]
    fn turkish_i() {
        let normy = Normy::builder().lang(TUR).add_stage(Lowercase).build();
        assert_eq!(normy.normalize("İSTANBUL").unwrap(), "istanbul");
        assert_eq!(normy.normalize("IĞDIR").unwrap(), "ığdır");
    }

    #[test]
    fn zero_copy_when_already_lower() {
        let normy = Normy::builder().lang(ENG).add_stage(Lowercase).build();
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
    fn length_expansion() {
        let normy = Normy::builder().lang(DEU).add_stage(CaseFold).build();
        let input = "ß";
        let result = normy.normalize(input).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result, "ss");
    }

    #[test]
    fn zero_copy_no_whitespace() {
        let normy = Normy::builder().lang(ENG).add_stage(TrimWhitespace).build();
        let input = "hello";
        let result = normy.normalize(input).unwrap();
        assert!(matches!(result, Cow::Borrowed(s) if s.as_ptr() == input.as_ptr()));
    }

    #[test]
    fn trims_all_ascii_whitespace() {
        let normy = Normy::builder().lang(ENG).add_stage(TrimWhitespace).build();
        assert_eq!(normy.normalize(" \t\n hello \r\n ").unwrap(), "hello");
    }

    #[test]
    fn full_width_spaces_japanese() {
        let normy = Normy::builder().lang(JPN).add_stage(TrimWhitespace).build();
        assert_eq!(normy.normalize("　こんにちは　").unwrap(), "こんにちは");
    }

    #[test]
    fn valid_utf8_no_alloc() {
        let normy = Normy::builder().lang(ENG).with_validation().build();
        let input = "hello 世界";
        let result = normy.normalize(input).unwrap();
        assert!(matches!(result, Cow::Borrowed(s) if s.as_ptr() == input.as_ptr()));
    }

    #[test]
    fn rejects_invalid_utf8() {
        let normy = Normy::builder().lang(ENG).with_validation().build();
        let invalid = b"hello \xFF world".to_vec();
        let input = unsafe { std::str::from_utf8_unchecked(&invalid) };
        assert!(normy.normalize(input).is_err());
    }

    #[test]
    fn simd_enabled_by_default() {
        let normy = Normy::builder().lang(ENG).with_validation().build();
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
}
