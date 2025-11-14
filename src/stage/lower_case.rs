// stage/lower_case.rs
use crate::{
    context::Context,
    lang::LocaleBehavior,
    stage::{CharMapper, Stage, StageError},
};
use std::sync::Arc;
use std::{borrow::Cow, iter::FusedIterator};

pub struct Lowercase;

impl Stage for Lowercase {
    fn name(&self) -> &'static str {
        "lowercase"
    }

    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        // Fast pre-check: any char that maps to different lowercase?
        Ok(text.chars().any(|c| {
            let lower = ctx
                .lang
                .case_map()
                .iter()
                .find(|m| m.from == c)
                .map(|m| m.to)
                .unwrap_or_else(|| c.to_lowercase().next().unwrap_or(c));
            lower != c
        }))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, ctx)? {
            return Ok(text);
        }

        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            if let Some(map) = ctx.lang.case_map().iter().find(|m| m.from == c) {
                out.push(map.to);
            } else {
                out.extend(c.to_lowercase());
            }
        }
        Ok(Cow::Owned(out))
    }

    #[inline]
    fn as_char_mapper(&self) -> Option<&dyn CharMapper> {
        Some(self)
    }
    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }
}

impl CharMapper for Lowercase {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> char {
        ctx.lang
            .case_map()
            .iter()
            .find(|m| m.from == c)
            .map(|m| m.to)
            .unwrap_or_else(|| c.to_lowercase().next().unwrap_or(c))
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn Iterator<Item = char> + 'a> {
        Box::new(LowercaseIter {
            chars: text.chars(),
            case_map: ctx.lang.case_map(),
        })
    }
}

struct LowercaseIter<'a> {
    chars: std::str::Chars<'a>,
    case_map: &'a [crate::lang::CaseMap],
}

impl<'a> Iterator for LowercaseIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let c = self.chars.next()?;
        Some(
            self.case_map
                .iter()
                .find(|m| m.from == c)
                .map(|m| m.to)
                .unwrap_or_else(|| c.to_lowercase().next().unwrap_or(c)),
        )
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}

impl<'a> FusedIterator for LowercaseIter<'a> {}
