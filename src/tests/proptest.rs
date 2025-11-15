mod prop_tests {
    use crate::{CaseFold, DEU, ENG, Lowercase, Normy, TUR, TrimWhitespace,RemoveDiacritics,FRA};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn case_fold_idempotent(s in ".{0,1000}") {
            let normy = Normy::builder().lang(DEU).add_stage(CaseFold).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice);
        }

        #[test]
        fn german_sharp_s_expansion(s in "[ßSs]{0,100}") {
            let normy = Normy::builder().lang(DEU).add_stage(CaseFold).build();
            let result = normy.normalize(&s).unwrap();
            prop_assert!(result.chars().all(|c| c == 's' || c == 'S'));
            prop_assert!(result.matches("ss").count() >= s.matches("ß").count());
        }

            #[test]
        fn lowercase_idempotent(s in ".{0,1000}") {
            let normy = Normy::builder().lang(ENG).add_stage(Lowercase).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice);
        }

        #[test]
        fn turkish_i_mapping(s in "[İıI]{1,100}") {
            let normy = Normy::builder().lang(TUR).add_stage(Lowercase).build();
            let result = normy.normalize(&s).unwrap();
            prop_assert!(result.chars().all(|c| c == 'i' || c == 'ı'));
        }

        #[test]
        fn zero_copy_no_change(s in "[a-z ]{0,1000}") {
            let normy = Normy::builder().lang(ENG).add_stage(Lowercase).build();
            let input = s.as_str();
            let result = normy.normalize(input).unwrap();
            prop_assert!(matches!(result, std::borrow::Cow::Borrowed(b) if b.as_ptr() == input.as_ptr()));
        }

            #[test]
        fn trim_idempotent(s in ".{0,1000}") {
            let normy = Normy::builder().lang(ENG).add_stage(TrimWhitespace).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice);
        }

        #[test]
        fn zero_copy_when_no_whitespace(s in "[^\\s]+") {
            let normy = Normy::builder().lang(ENG).add_stage(TrimWhitespace).build();

            // Additional check: ensure string has no leading/trailing whitespace
            prop_assume!(!s.is_empty());
            prop_assume!(!s.chars().next().unwrap().is_whitespace());
            prop_assume!(!s.chars().next_back().unwrap().is_whitespace());

            let input = s.as_str();
            let result = normy.normalize(input).unwrap();
            prop_assert!(matches!(result, std::borrow::Cow::Borrowed(b) if b.as_ptr() == input.as_ptr()));
        }

        #[test]
        fn trims_all_unicode_whitespace(s in "\\p{Zs}{0,10}.*\\p{Zs}{0,10}") {
            let normy = Normy::builder().lang(ENG).add_stage(TrimWhitespace).build();
            let result = normy.normalize(&s).unwrap();
            let trimmed = s.trim();
            prop_assert_eq!(&*result, trimmed);
        }

        #[test]
        fn valid_utf8_passes(s in ".{0,1000}") {
            let normy = Normy::builder().lang(ENG).build();
            let result = normy.normalize(&s);
            prop_assert!(result.is_ok());
            let cow = result.unwrap();
            prop_assert_eq!(cow.as_ref(), s.as_str());
        }

        #[test]
        fn prop_remove_diacritics_idempotent(s in "\\PC*") {
            let n = Normy::builder().lang(FRA).add_stage(RemoveDiacritics).build();
            let once = n.normalize(&s).unwrap().into_owned();
            let twice = n.normalize(&once).unwrap().into_owned();
            prop_assert_eq![once, twice];
        }
    }
}
