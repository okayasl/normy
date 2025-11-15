#[cfg(test)]
mod integration_tests {

    use crate::{
        ARA, CaseFold, DEU, Lowercase, Normy, TUR, TrimWhitespace,
        stage::remove_diacritics::RemoveDiacritics,
    };

    #[test]
    fn production_pipeline_turkish() {
        let normy = Normy::builder()
            .lang(TUR)
            .add_stage(TrimWhitespace)
            .add_stage(Lowercase)
            .build();

        let input = " İSTANBUL  ";
        let result = normy.normalize(input).unwrap();
        assert_eq!(result, "istanbul");
    }

    #[test]
    fn production_pipeline_arabic_diacritics() {
        let normy = Normy::builder()
            .lang(ARA)
            .add_stage(TrimWhitespace)
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
            .add_stage(TrimWhitespace)
            .add_stage(CaseFold)
            .build();

        let input = "  Weißstraße  ";
        let result = normy.normalize(input).unwrap();
        assert_eq!(result, "weissstrasse");
    }
}
