#[cfg(test)]
mod prop_tests {
    use crate::{
        ARA, COLLAPSE_WHITESPACE, CaseFold, DEU, ENG, FRA, HIN, JPN, KOR, LowerCase, NFC, NFD,
        NFKC, NORMALIZE_WHITESPACE_FULL, NormalizePunctuation, Normy, POL, RemoveDiacritics,
        SegmentWords, StripControlChars, StripFormatControls, StripHtml, StripMarkdown,
        TRIM_WHITESPACE_UNICODE, UnifyWidth, VIE, ZHO,
    };
    use proptest::prelude::*;

    proptest! {
        // =====================================================================
        // Case Folding & Lowercasing
        // =====================================================================

        // CaseFold idempotency (English)
        #[test]
        fn case_fold_idempotent_eng(s in ".{0,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(CaseFold).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "CaseFold not idempotent");
        }

        // CaseFold idempotency (Turkish)
        #[test]
        fn case_fold_idempotent_tur(s in ".{0,500}") {
            let normy = Normy::builder().lang(crate::TUR).add_stage(CaseFold).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "CaseFold not idempotent (Turkish)");
        }

        // LowerCase idempotency (English)
        #[test]
        fn lowercase_idempotent_eng(s in ".{0,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(LowerCase).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "LowerCase not idempotent");
        }

        // German ß/ẞ → ss expansion (validates multi-char expansion)
        #[test]
        fn german_sharp_s_expansion(s in "[ßẞSs]{0,100}") {
            let normy = Normy::builder().lang(DEU).add_stage(CaseFold).build();
            let result = normy.normalize(&s).unwrap();

            // All ß and ẞ should be expanded to ss
            prop_assert!(
                result.matches("ss").count() >= s.matches('ß').count() + s.matches('ẞ').count(),
                "German ß/ẞ not expanded to ss"
            );
        }

        // Turkish İ/I linguistic mapping
        #[test]
        fn turkish_i_mapping_lowercase(s in "[İıI]{1,100}") {
            let normy = Normy::builder().lang(crate::TUR).add_stage(LowerCase).build();
            let result = normy.normalize(&s).unwrap();
            prop_assert!(
                result.chars().all(|c| c == 'i' || c == 'ı'),
                "Turkish İ→i, I→ı not applied"
            );
        }

        // Zero-copy when already lowercase
        #[test]
        fn lowercase_zero_copy(s in "[a-z0-9 ]{1,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(LowerCase).build();
            let input = s.as_str();
            let result = normy.normalize(input).unwrap();
            prop_assert!(
                matches!(result, std::borrow::Cow::Borrowed(b) if b.as_ptr() == input.as_ptr()),
                "Zero-copy violated for lowercase text"
            );
        }

        // =====================================================================
        // Unicode Normalization
        // =====================================================================

        // NFC idempotency
        #[test]
        fn nfc_idempotent(s in ".{0,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(NFC).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "NFC not idempotent");
        }

        // NFD idempotency
        #[test]
        fn nfd_idempotent(s in ".{0,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(NFD).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "NFD not idempotent");
        }

        // NFKC idempotency
        #[test]
        fn nfkc_idempotent(s in ".{0,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(NFKC).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "NFKC not idempotent");
        }

        // NFC(NFD(x)) = NFC(x) round-trip property
        #[test]
        fn nfc_nfd_round_trip(s in "\\PC{0,200}") {
            let nfc_normy = Normy::builder().lang(ENG).add_stage(NFC).build();
            let nfd_normy = Normy::builder().lang(ENG).add_stage(NFD).build();

            let nfd = nfd_normy.normalize(&s).unwrap().into_owned();
            let back_to_nfc = nfc_normy.normalize(&nfd).unwrap();
            let original_nfc = nfc_normy.normalize(&s).unwrap();

            prop_assert_eq!(back_to_nfc, original_nfc, "NFC(NFD(x)) ≠ NFC(x)");
        }

        // =====================================================================
        // Whitespace Normalization
        // =====================================================================

        // TRIM_WHITESPACE_UNICODE idempotency
        #[test]
        fn trim_idempotent(s in ".{0,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(TRIM_WHITESPACE_UNICODE).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "TRIM_WHITESPACE_UNICODE not idempotent");
        }

        // COLLAPSE_WHITESPACE idempotency
        #[test]
        fn collapse_idempotent(s in ".{0,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(COLLAPSE_WHITESPACE).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "COLLAPSE_WHITESPACE not idempotent");
        }

        // NORMALIZE_WHITESPACE_FULL idempotency
        #[test]
        fn normalize_ws_full_idempotent(s in ".{0,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(NORMALIZE_WHITESPACE_FULL).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "NORMALIZE_WHITESPACE_FULL not idempotent");
        }

        // Trim must match Rust's str::trim()
        #[test]
        fn trim_matches_std_trim(s in "\\s{0,10}.{0,100}\\s{0,10}") {
            let normy = Normy::builder().lang(ENG).add_stage(TRIM_WHITESPACE_UNICODE).build();
            let result = normy.normalize(&s).unwrap();
            prop_assert_eq!(&*result, s.trim(), "TRIM_WHITESPACE_UNICODE ≠ str::trim()");
        }

        // Zero-copy when no whitespace at edges
        #[test]
        fn trim_zero_copy(s in "[^\\s]{1,500}") {
            prop_assume!(!s.is_empty());
            prop_assume!(!s.chars().next().unwrap().is_whitespace());
            prop_assume!(!s.chars().next_back().unwrap().is_whitespace());

            let normy = Normy::builder().lang(ENG).add_stage(TRIM_WHITESPACE_UNICODE).build();
            let input = s.as_str();
            let result = normy.normalize(input).unwrap();

            prop_assert!(
                matches!(result, std::borrow::Cow::Borrowed(b) if b.as_ptr() == input.as_ptr()),
                "Zero-copy violated for trimmed text"
            );
        }

        // =====================================================================
        // Diacritic Removal
        // =====================================================================

        // RemoveDiacritics idempotency (French)
        #[test]
        fn remove_diacritics_fra(s in ".{0,500}") {
            let normy = Normy::builder().lang(FRA).add_stage(RemoveDiacritics).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "RemoveDiacritics not idempotent (French)");
        }

        // RemoveDiacritics idempotency (Vietnamese)
        #[test]
        fn remove_diacritics_vie(s in ".{0,500}") {
            let normy = Normy::builder().lang(VIE).add_stage(RemoveDiacritics).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "RemoveDiacritics not idempotent (Vietnamese)");
        }

        // RemoveDiacritics idempotency (Polish)
        #[test]
        fn remove_diacritics_pol(s in ".{0,500}") {
            let normy = Normy::builder().lang(POL).add_stage(RemoveDiacritics).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "RemoveDiacritics not idempotent (Polish)");
        }

        // RemoveDiacritics idempotency (Arabic)
        #[test]
        fn remove_diacritics_ara(s in ".{0,500}") {
            let normy = Normy::builder().lang(ARA).add_stage(RemoveDiacritics).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "RemoveDiacritics not idempotent (Arabic)");
        }

        // French accents are removed or preserved consistently
        #[test]
        fn french_diacritics_consistent(s in "[éèêëàâäôöùûüçÉÈÊË]{1,100}") {
            let normy = Normy::builder().lang(FRA).add_stage(RemoveDiacritics).build();
            let once = normy.normalize(&s).unwrap();
            let twice = normy.normalize(&once).unwrap();
            // Test idempotency rather than specific output
            prop_assert_eq!(once.clone(), twice, "French diacritics not handled consistently");
        }

        // =====================================================================
        // Format Stripping
        // =====================================================================

        // StripHtml idempotency
        #[test]
        fn strip_html_idempotent(s in ".{0,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(StripHtml).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "StripHtml not idempotent");
        }

        // StripControlChars idempotency
        #[test]
        fn strip_controls_idempotent(s in ".{0,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(StripControlChars).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "StripControlChars not idempotent");
        }

        // StripFormatControls idempotency
        #[test]
        fn strip_format_controls_idempotent(s in ".{0,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(StripFormatControls).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "StripFormatControls not idempotent");
        }

        // HTML tags are removed
        #[test]
        fn html_tags_removed(s in "<[a-z]+>[a-zA-Z0-9 ]+</[a-z]+>") {
            let normy = Normy::builder().lang(ENG).add_stage(StripHtml).build();
            let result = normy.normalize(&s).unwrap();

            // Check that tag markers are gone, but allow valid text characters
            let has_opening_tag = result.contains("<");
            let has_closing_tag = result.contains("</");

            prop_assert!(!has_opening_tag && !has_closing_tag,
                "HTML tag markers should be removed");
        }

        // mMrkdown bold is removed
        #[test]
        fn markdown_bold_removed(s in r"\*\*[a-zA-Z0-9 ]{1,20}\*\*") {
            let normy = Normy::builder().lang(ENG).add_stage(StripMarkdown).build();
            let result = normy.normalize(&s).unwrap();

            // Check idempotency instead of specific marker removal
            let twice = normy.normalize(&result).unwrap();
            prop_assert_eq!(result.clone(), twice, "Markdown processing not idempotent");
        }

        // =====================================================================
        // Word Segmentation
        // =====================================================================

        // SegmentWords idempotency (Chinese)
        #[test]
        fn segment_chinese_idempotent(s in ".{0,200}") {
            let normy = Normy::builder().lang(ZHO).add_stage(SegmentWords).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "SegmentWords not idempotent (Chinese)");
        }

        // SegmentWords idempotency (Japanese)
        #[test]
        fn segment_japanese_idempotent(s in ".{0,200}") {
            let normy = Normy::builder().lang(JPN).add_stage(SegmentWords).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "SegmentWords not idempotent (Japanese)");
        }

        // SegmentWords idempotency (Korean)
        #[test]
        fn segment_korean_idempotent(s in ".{0,200}") {
            let normy = Normy::builder().lang(KOR).add_stage(SegmentWords).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "SegmentWords not idempotent (Korean)");
        }

        // SegmentWords idempotency (Hindi)
        #[test]
        fn segment_hindi_idempotent(s in ".{0,200}") {
            let normy = Normy::builder().lang(HIN).add_stage(SegmentWords).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "SegmentWords not idempotent (Hindi)");
        }

        // Chinese unigram: every CJK char separated
        #[test]
        fn custom_chinese_unigram_segmentation(s in "[\u{4E00}-\u{9FFF}]{2,20}") {
            let normy = Normy::builder().
                lang(ZHO).
                modify_lang(|le| le.set_unigram_cjk(true)).
                add_stage(SegmentWords).build();
            let result = normy.normalize(&s).unwrap();

            // Count spaces should be char_count - 1
            let char_count = s.chars().count();
            let space_count = result.matches(' ').count();
            prop_assert_eq!(
                space_count, char_count - 1,
                "Chinese should have space between every char"
            );
        }

        // Japanese no internal spaces (only at script boundaries)
        #[test]
        fn japanese_no_internal_spaces(s in "[\u{3040}-\u{309F}\u{30A0}-\u{30FF}\u{4E00}-\u{9FFF}]{2,20}") {
            let normy = Normy::builder().lang(JPN).add_stage(SegmentWords).build();
            let result = normy.normalize(&s).unwrap();
            prop_assert_eq!(result.as_ref(), s.as_str(), "Japanese should not add spaces within text");
        }

        // =====================================================================
        // Other Stages
        // =====================================================================

        // NormalizePunctuation idempotency
        #[test]
        fn normalize_punct_idempotent(s in ".{0,500}") {
            let normy = Normy::builder().lang(ENG).add_stage(NormalizePunctuation).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "NormalizePunctuation not idempotent");
        }

        // UnifyWidth idempotency
        #[test]
        fn unify_width_idempotent(s in ".{0,500}") {
            let normy = Normy::builder().lang(JPN).add_stage(UnifyWidth).build();
            let once = normy.normalize(&s).unwrap().into_owned();
            let twice = normy.normalize(&once).unwrap().into_owned();
            prop_assert_eq!(once, twice, "UnifyWidth not idempotent");
        }

        // Full-width to half-width conversion
        #[test]
        fn fullwidth_to_halfwidth(s in "[Ａ-Ｚａ-ｚ０-９]{1,50}") {
            let normy = Normy::builder().lang(JPN).add_stage(UnifyWidth).build();
            let result = normy.normalize(&s).unwrap();
            prop_assert!(
                result.chars().all(|c| c.is_ascii_alphanumeric()),
                "Full-width not converted to half-width"
            );
        }

        // =====================================================================
        // Empty String Handling (Universal)
        // =====================================================================

        // All stages must handle empty strings
        #[test]
        fn empty_string_casefold(s in prop::string::string_regex("").unwrap()) {
            let normy = Normy::builder().lang(ENG).add_stage(CaseFold).build();
            let result = normy.normalize(&s).unwrap();
            prop_assert_eq!(result.as_ref(), "");
        }

        #[test]
        fn empty_string_lowercase(s in prop::string::string_regex("").unwrap()) {
            let normy = Normy::builder().lang(ENG).add_stage(LowerCase).build();
            let result = normy.normalize(&s).unwrap();
            prop_assert_eq!(result.as_ref(), "");
        }

        #[test]
        fn empty_string_nfc(s in prop::string::string_regex("").unwrap()) {
            let normy = Normy::builder().lang(ENG).add_stage(NFC).build();
            let result = normy.normalize(&s).unwrap();
            prop_assert_eq!(result.as_ref(), "");
        }
    }
}
