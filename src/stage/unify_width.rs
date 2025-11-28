use crate::{
    JPN, KOR, ZHO,
    context::Context,
    lang::Lang,
    stage::{CharMapper, Stage, StageError},
    testing::stage_contract::StageTestConfig,
    unicode::{fullwidth_to_halfwidth, is_fullwidth},
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

/// Convert full-width (wide) ASCII forms → half-width (narrow) equivalents
///
/// Maps:
/// - Full-width Latin letters `Ａ−Ｚａ−ｚ` → `A−Za−z`
/// - Full-width digits `０−９` → `0−9`
/// - Full-width punctuation `！＂＃＄％＆＇（）＊＋，－．／：；＜＝＞？＠［＼］＾＿｀｛｜｝～` → ASCII
///
/// Essential for CJK ↔ Latin search equivalence and input normalization.
///
/// ### Use Cases
/// - Japanese/Chinese search queries
/// - User input from mobile IMEs
/// - Cross-platform text alignment
///
/// Zero-copy when no full-width present. **Pure 1→1 CharMapper** → maximum performance.
pub struct UnifyWidth;

impl Stage for UnifyWidth {
    fn name(&self) -> &'static str {
        "unifyWidth"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(text.chars().any(is_fullwidth))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, _ctx)? {
            return Ok(text);
        }
        Ok(Cow::Owned(
            text.chars().map(fullwidth_to_halfwidth).collect(),
        ))
    }

    #[inline]
    fn as_char_mapper(&self, _ctx: &Context) -> Option<&dyn CharMapper> {
        Some(self)
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, _ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }
}

impl CharMapper for UnifyWidth {
    #[inline(always)]
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        let converted = fullwidth_to_halfwidth(c);
        if converted == c {
            Some(c)
        } else {
            Some(converted)
        }
    }

    fn bind<'a>(&self, text: &'a str, _ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(text.chars().map(fullwidth_to_halfwidth))
    }
}

impl StageTestConfig for UnifyWidth {
    fn one_to_one_languages() -> &'static [Lang] {
        &[JPN, ZHO, KOR] // Critical for East Asian search
    }

    fn samples(lang: Lang) -> &'static [&'static str] {
        match lang {
            JPN => &["Ｈｅｌｌｏ　Ｗｏｒｌｄ！", "１２３４５円"],
            ZHO => &["你好　Ｗｏｒｌｄ", "全角１２３"],
            _ => &["Full-width ABC１２３！", "Normal text"],
        }
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;
    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(UnifyWidth);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::data::ENG;
    use std::borrow::Cow;

    #[test]
    fn test_needs_apply_detects_fullwidth() {
        let stage = UnifyWidth;
        assert!(stage.needs_apply("ＡＢＣ", &Context::new(ENG)).unwrap());
    }

    #[test]
    fn test_needs_apply_false_for_ascii() {
        let stage = UnifyWidth;
        assert!(!stage.needs_apply("ABC 123 !?", &Context::new(ENG)).unwrap());
    }

    #[test]
    fn test_apply_fullwidth_latin() {
        let stage = UnifyWidth;
        let result = stage
            .apply(Cow::Borrowed("Ｈｅｌｌｏ Ｗｏｒｌｄ"), &Context::new(ENG))
            .unwrap();
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_apply_fullwidth_digits_punctuation() {
        let stage = UnifyWidth;
        let result = stage
            .apply(Cow::Borrowed("１２３４５！＠＃"), &Context::new(ENG))
            .unwrap();
        assert_eq!(result, "12345!@#");
    }

    #[test]
    fn test_apply_when_no_changes_returns_borrowed() {
        let stage = UnifyWidth;
        let text = Cow::Borrowed("Plain ASCII text");
        let result = stage.apply(text.clone(), &Context::new(ENG)).unwrap();

        match result {
            Cow::Borrowed(_) => {} // OK: zero-copy
            _ => panic!("Expected Cow::Borrowed for unchanged ASCII"),
        }
    }

    #[test]
    fn test_char_mapper_map_fullwidth() {
        let stage = UnifyWidth;
        let mapper: &dyn CharMapper = &stage;

        assert_eq!(mapper.map('Ａ', &Context::new(ENG)), Some('A'));
        assert_eq!(mapper.map('９', &Context::new(ENG)), Some('9'));
        assert_eq!(mapper.map('！', &Context::new(ENG)), Some('!'));

        // unchanged ASCII remains ASCII
        assert_eq!(mapper.map('x', &Context::new(ENG)), Some('x'));
    }

    #[test]
    fn test_char_mapper_bind_iterates_normalized() {
        let stage = UnifyWidth;
        let mapper: &dyn CharMapper = &stage;

        let iter = mapper.bind("ＡＢＣ １２３！", &Context::new(ENG));
        let collected: String = iter.collect();

        assert_eq!(collected, "ABC 123!");
    }

    #[test]
    fn test_fullwidth_replace_sanity() {
        let stage = UnifyWidth;
        let text = Cow::Borrowed("Ｔｅｘｔ： １００％ full-width");
        let result = stage.apply(text, &Context::new(ENG)).unwrap();

        assert_eq!(result, "Text: 100% full-width");
    }

    #[test]
    fn test_replace_fullwidth() {
        let stage = UnifyWidth;
        let text = Cow::Borrowed("Ｈｅｌｌｏ　Ｗｏｒｌｄ！");
        let result = stage.apply(text, &Context::new(ENG)).unwrap();
        assert_eq!(result, "Hello World!");
    }

    #[test]
    fn test_non_fullwidth_passthrough() {
        assert_eq!(fullwidth_to_halfwidth('A'), 'A');
        assert_eq!(fullwidth_to_halfwidth(' '), ' ');
        assert_eq!(fullwidth_to_halfwidth('中'), '中');
    }

    #[test]
    fn test_fullwidth_ascii() {
        assert_eq!(fullwidth_to_halfwidth('Ａ'), 'A');
        assert_eq!(fullwidth_to_halfwidth('ｚ'), 'z');
        assert_eq!(fullwidth_to_halfwidth('５'), '5');
        assert_eq!(fullwidth_to_halfwidth('！'), '!');
    }
}
