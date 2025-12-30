use crate::{
    JPN, KOR, ZHO,
    context::Context,
    lang::Lang,
    stage::{Stage, StageError, StaticFusableStage},
    testing::stage_contract::StageTestConfig,
    unicode::{fullwidth_to_halfwidth, is_fullwidth},
};
use std::borrow::Cow;
use std::iter::FusedIterator;

/// Convert full-width (wide) ASCII forms → half-width (narrow) equivalents
///
/// Maps:
/// - Full-width Latin `Ａ−Ｚａ−ｚ` → `A−Za−z`
/// - Full-width digits `０−９` → `0−9`
/// - Full-width punctuation → ASCII equivalents
/// - Ideographic space `　` → ` `
///
/// Essential for CJK ↔ Latin search equivalence.
/// Pure 1:1 → maximum fusion and zero-copy performance.
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

impl StageTestConfig for UnifyWidth {
    fn one_to_one_languages() -> &'static [Lang] {
        &[JPN, ZHO, KOR]
    }

    fn samples(lang: Lang) -> &'static [&'static str] {
        match lang {
            JPN => &["Ｈｅｌｌｏ　Ｗｏｒｌｄ！", "１２３４５円"],
            ZHO => &["你好　Ｗｏｒｌｄ", "全角１２３"],
            KOR => &["안녕　Ｗｏｒｌｄ", "１２３"],
            _ => &["Full-width ABC１２３！　", "Normal text"],
        }
    }

    fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
        &["hello world", "test123", ""]
    }

    fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
        &[
            ("ＡＢＣ", "ABC"),
            ("１２３", "123"),
            ("！＠＃", "!@#"),
            ("　", " "),
            ("Ｈｅｌｌｏ　Ｗｏｒｌｄ！", "Hello World!"),
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
