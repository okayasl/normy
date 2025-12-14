use crate::{
    JPN, KOR, ZHO,
    context::Context,
    lang::Lang,
    stage::{Stage, StageError, StaticStageIter},
    testing::stage_contract::StageTestConfig,
    unicode::{fullwidth_to_halfwidth, is_fullwidth},
};
use std::borrow::Cow;
use std::iter::FusedIterator;

/// Convert full-width (wide) ASCII forms → half-width (narrow) equivalents
///
/// Maps:
/// - Full-width Latin letters `Ａ−Ｚａ−ｚ` → `A−Za−z`
/// - Full-width digits `０−９` → `0−9`
/// - Full-width punctuation `！＂＃＄％＆＇（）＊＋，－．／：；＜＝＞？＠［＼］＾＿｀｛｜｝～` → ASCII
///
/// Essential for CJK ↔ Latin search equivalence and input normalization.
///
/// Zero-copy when no full-width present. Pure 1→1 CharMapper → maximum performance.
/// Language-agnostic. Idempotent.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnifyWidth;

impl Stage for UnifyWidth {
    fn name(&self) -> &'static str {
        "unify_width"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        // Extremely fast scan — full-width chars are rare in most text
        Ok(text.chars().any(is_fullwidth))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // We are here → full-width chars exist → allocate once, convert perfectly
        Ok(Cow::Owned(UnifyWidthIter::new(&text).collect()))
    }

    fn try_dynamic_iter<'a>(
        &self,
        text: &'a str,
        _ctx: &'a Context,
    ) -> Option<Box<dyn FusedIterator<Item = char> + 'a>> {
        Some(Box::new(UnifyWidthIter::new(text)))
    }
}

impl StaticStageIter for UnifyWidth {
    type Iter<'a> = UnifyWidthIter<'a>;

    #[inline(always)]
    fn try_static_iter<'a>(&self, text: &'a str, _ctx: &'a Context) -> Option<Self::Iter<'a>> {
        Some(UnifyWidthIter::new(text))
    }
}

/// Pure 1→1 iterator — no heap, no closure, no overhead
pub struct UnifyWidthIter<'a> {
    chars: std::str::Chars<'a>,
}

impl<'a> UnifyWidthIter<'a> {
    #[inline(always)]
    pub fn new(text: &'a str) -> Self {
        Self {
            chars: text.chars(),
        }
    }
}

impl Iterator for UnifyWidthIter<'_> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.chars.next().map(fullwidth_to_halfwidth)
    }
}

impl FusedIterator for UnifyWidthIter<'_> {}

impl StageTestConfig for UnifyWidth {
    fn one_to_one_languages() -> &'static [Lang] {
        &[JPN, ZHO, KOR]
    }

    fn samples(lang: Lang) -> &'static [&'static str] {
        match lang {
            JPN => &["Ｈｅｌｌｏ　Ｗｏｒｌｄ！", "１２３４５円"],
            ZHO => &["你好　Ｗｏｒｌｄ", "全角１２３"],
            _ => &["Full-width ABC１２３！", "Normal text"],
        }
    }

    fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
        &[
            "hello world", // Already half-width
            "test123",
            "",
        ]
    }

    fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
        &[
            ("Ａ", "A"),       // Full-width A → half-width
            ("１２３", "123"), // Full-width digits
            ("！", "!"),       // Full-width punctuation
            ("　", " "),       // Ideographic space
        ]
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
