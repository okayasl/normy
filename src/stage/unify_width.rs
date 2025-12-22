use crate::{
    JPN, KOR, ZHO,
    context::Context,
    lang::Lang,
    stage::{FusableStage, Stage, StageError, StaticFusableStage},
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
        // Direct allocation path: 1:1 mapping means capacity matches exactly
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            out.push(fullwidth_to_halfwidth(c));
        }
        Ok(Cow::Owned(out))
    }

    /// UnifyWidth is always fusable - checking needs_apply on the original text
    /// is always a safe approximation since it only performs 1:1 mappings.
    #[inline]
    fn safe_skip_approximation(&self) -> bool {
        true
    }

    /// UnifyWidth is always fusable. Only performs 1:1 character mappings.
    #[inline]
    fn as_fusable(&self) -> Option<&dyn FusableStage> {
        Some(self)
    }
}

impl StaticFusableStage for UnifyWidth {
    type Adapter<'a, I>
        = UnifyWidthAdapter<I>
    where
        I: FusedIterator<Item = char> + 'a;

    #[inline(always)]
    fn supports_static_fusion(&self) -> bool {
        true
    }

    #[inline(always)]
    fn static_fused_adapter<'a, I>(&self, input: I, _ctx: &'a Context) -> Self::Adapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        UnifyWidthAdapter { input }
    }
}

pub struct UnifyWidthAdapter<I> {
    input: I,
}

impl<I: Iterator<Item = char>> Iterator for UnifyWidthAdapter<I> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.input.next().map(fullwidth_to_halfwidth)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        // 1:1 mapping preserves exact string length
        self.input.size_hint()
    }
}

impl<I: FusedIterator<Item = char>> FusedIterator for UnifyWidthAdapter<I> {}

impl FusableStage for UnifyWidth {
    fn dyn_fused_adapter<'a>(
        &self,
        input: Box<dyn FusedIterator<Item = char> + 'a>,
        _ctx: &'a Context,
    ) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(UnifyWidthAdapter { input })
    }
}

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
