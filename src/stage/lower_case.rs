//! Turkish-aware lower-casing.
//!
//! Implements both static (`&self`) and dynamic (`Arc`) paths so that a
//! monomorphised pipeline fuses completely into a single `for c in ...` loop.

use crate::{
    context::Context,
    lang::Lang,
    stage::{CharMapper, Stage, StageError},
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

/// Public stage struct – zero-sized.
pub struct Lowercase;

impl Stage for Lowercase {
    fn name(&self) -> &'static str {
        "lowercase"
    }

    /// Fast pre-check – skip if no uppercase chars (or Turkish specials).
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        Ok(match ctx.lang {
            Lang::Turkish => text.contains(['I', 'İ']),
            _ => text.chars().any(char::is_uppercase),
        })
    }

    /// Allocation-aware `apply`.  Used when the iterator path is unavailable.
    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, ctx)? {
            return Ok(text);
        }

        let result: String = if ctx.lang == Lang::Turkish {
            let mut out = String::with_capacity(text.len());
            for c in text.chars() {
                match c {
                    'I' => out.push('ı'),
                    'İ' => out.push('i'),
                    _ => out.extend(c.to_lowercase()),
                }
            }
            out
        } else {
            text.to_lowercase()
        };
        Ok(Cow::Owned(result))
    }

    // ────── STATIC PATH ──────
    #[inline]
    fn as_char_mapper(&self) -> Option<&dyn CharMapper> {
        Some(self)
    }

    // ────── DYNAMIC PATH ──────
    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }
}

/* ------------------------------------------------------------------ */
/* CharMapper – implemented directly on the stage (zero-cost)        */
/* ------------------------------------------------------------------ */
impl CharMapper for Lowercase {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> char {
        match (c, ctx.lang) {
            ('I', Lang::Turkish) => 'ı',
            ('İ', Lang::Turkish) => 'i',
            _ => c.to_lowercase().next().unwrap_or(c),
        }
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn Iterator<Item = char> + 'a> {
        Box::new(LowercaseIter {
            chars: text.chars(),
            lang: ctx.lang,
        })
    }
}

/* ------------------------------------------------------------------ */
/* Stateful iterator – zero allocation, FusedIterator                */
/* ------------------------------------------------------------------ */
struct LowercaseIter<'a> {
    chars: std::str::Chars<'a>,
    lang: Lang,
}

impl<'a> Iterator for LowercaseIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let c = self.chars.next()?;
        Some(match (c, self.lang) {
            ('I', Lang::Turkish) => 'ı',
            ('İ', Lang::Turkish) => 'i',
            _ => c.to_lowercase().next().unwrap_or(c),
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}
impl<'a> FusedIterator for LowercaseIter<'a> {}
