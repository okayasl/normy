//! stage/replace_fullwidth.rs
//! Convert full-width Latin, digits, and punctuation → half-width ASCII equivalents
//! Essential for CJK ↔ Latin search equivalence (e.g. "ＡＢＣ" → "ABC")
//! Zero-copy when no full-width chars present
//! CharMapper path: pure 1→1 mapping

use crate::{
    context::Context,
    stage::{CharMapper, Stage, StageError},
    unicode::{fullwidth_to_halfwidth, is_fullwidth},
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

pub struct ReplaceFullwidth;

impl Stage for ReplaceFullwidth {
    fn name(&self) -> &'static str {
        "replace_fullwidth"
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

impl CharMapper for ReplaceFullwidth {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::ENG;
    use std::borrow::Cow;

    fn make_context() -> Context {
        Context { lang: ENG }
    }

    #[test]
    fn test_needs_apply_detects_fullwidth() {
        let stage = ReplaceFullwidth;
        assert!(stage.needs_apply("ＡＢＣ", &make_context()).unwrap());
    }

    #[test]
    fn test_needs_apply_false_for_ascii() {
        let stage = ReplaceFullwidth;
        assert!(!stage.needs_apply("ABC 123 !?", &make_context()).unwrap());
    }

    #[test]
    fn test_apply_fullwidth_latin() {
        let stage = ReplaceFullwidth;
        let result = stage
            .apply(Cow::Borrowed("Ｈｅｌｌｏ Ｗｏｒｌｄ"), &make_context())
            .unwrap();
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_apply_fullwidth_digits_punctuation() {
        let stage = ReplaceFullwidth;
        let result = stage
            .apply(Cow::Borrowed("１２３４５！＠＃"), &make_context())
            .unwrap();
        assert_eq!(result, "12345!@#");
    }

    #[test]
    fn test_apply_when_no_changes_returns_borrowed() {
        let stage = ReplaceFullwidth;
        let text = Cow::Borrowed("Plain ASCII text");
        let result = stage.apply(text.clone(), &make_context()).unwrap();

        match result {
            Cow::Borrowed(_) => {} // OK: zero-copy
            _ => panic!("Expected Cow::Borrowed for unchanged ASCII"),
        }
    }

    #[test]
    fn test_char_mapper_map_fullwidth() {
        let stage = ReplaceFullwidth;
        let mapper: &dyn CharMapper = &stage;

        assert_eq!(mapper.map('Ａ', &make_context()), Some('A'));
        assert_eq!(mapper.map('９', &make_context()), Some('9'));
        assert_eq!(mapper.map('！', &make_context()), Some('!'));

        // unchanged ASCII remains ASCII
        assert_eq!(mapper.map('x', &make_context()), Some('x'));
    }

    #[test]
    fn test_char_mapper_bind_iterates_normalized() {
        let stage = ReplaceFullwidth;
        let mapper: &dyn CharMapper = &stage;

        let iter = mapper.bind("ＡＢＣ １２３！", &make_context());
        let collected: String = iter.collect();

        assert_eq!(collected, "ABC 123!");
    }

    #[test]
    fn test_fullwidth_replace_sanity() {
        let stage = ReplaceFullwidth;
        let text = Cow::Borrowed("Ｔｅｘｔ： １００％ full-width");
        let result = stage.apply(text, &make_context()).unwrap();

        assert_eq!(result, "Text: 100% full-width");
    }

    #[test]
    fn test_replace_fullwidth() {
        let stage = ReplaceFullwidth;
        let text = Cow::Borrowed("Ｈｅｌｌｏ　Ｗｏｒｌｄ！");
        let result = stage.apply(text, &make_context()).unwrap();
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
