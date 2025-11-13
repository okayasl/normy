//! Unicode whitespace trimming – zero-allocation iterator path.
//!
//! The static path returns `&self`; the dynamic path re-uses the same `Arc`.

use crate::{
    context::Context,
    stage::{CharMapper, Stage, StageError},
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::str::Chars;
use std::sync::Arc;

/// Public stage – zero-sized.
pub struct TrimWhitespace;

impl Stage for TrimWhitespace {
    fn name(&self) -> &'static str {
        "trim_ws"
    }

    fn needs_apply(&self, text: &str, _: &Context) -> Result<bool, StageError> {
        let bytes = text.as_bytes();
        Ok(bytes.first().is_some_and(u8::is_ascii_whitespace)
            || bytes.last().is_some_and(u8::is_ascii_whitespace))
    }

    /// Allocation-aware `apply`.  Returns `Cow::Borrowed` when no trim occurs.
    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let trimmed = text.trim();
        Ok(
            if trimmed.as_ptr() == text.as_ptr() && trimmed.len() == text.len() {
                text
            } else {
                Cow::Owned(trimmed.to_string())
            },
        )
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
/* CharMapper – directly on the stage (zero-cost)                    */
/* ------------------------------------------------------------------ */
impl CharMapper for TrimWhitespace {
    #[inline(always)]
    fn map(&self, c: char, _: &Context) -> char {
        c
    }

    fn bind<'a>(&self, text: &'a str, _: &Context) -> Box<dyn Iterator<Item = char> + 'a> {
        Box::new(TrimIter {
            chars: text.chars(),
            phase: TrimPhase::Leading,
            emitted: false,
        })
    }
}

/* ------------------------------------------------------------------ */
/* Trim iterator – three-phase, zero-allocation, FusedIterator       */
/* ------------------------------------------------------------------ */
#[derive(Clone, Copy, PartialEq, Eq)]
enum TrimPhase {
    Leading,  // drop leading whitespace
    Middle,   // emit everything
    Trailing, // drop trailing whitespace (entered on first non-ws)
}

struct TrimIter<'a> {
    chars: Chars<'a>,
    phase: TrimPhase,
    emitted: bool, // true after the first non-ws character
}

impl<'a> Iterator for TrimIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let c = self.chars.next()?;
            match self.phase {
                TrimPhase::Leading if c.is_whitespace() => continue,
                TrimPhase::Leading => {
                    self.phase = TrimPhase::Middle;
                    self.emitted = true;
                    return Some(c);
                }
                TrimPhase::Middle => {
                    if c.is_whitespace() && self.chars.as_str().chars().all(char::is_whitespace) {
                        // last non-ws was just emitted → switch to trailing
                        self.phase = TrimPhase::Trailing;
                    }
                    return Some(c);
                }
                TrimPhase::Trailing => continue,
            }
        }
    }
}
impl<'a> FusedIterator for TrimIter<'a> {}
