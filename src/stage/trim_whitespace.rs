use crate::{
    context::Context,
    stage::{CharMapper, Stage, StageError},
};
use std::{borrow::Cow, iter::FusedIterator, str::Chars};

/// Trims Unicode whitespace from the start and end of the text.
pub struct TrimWhitespace;

impl Stage for TrimWhitespace {
    fn name(&self) -> &'static str {
        "trim_ws"
    }

    /// Fast pre-check – true if the text has any leading or trailing whitespace.
    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(text.trim_start().len() != text.len() || text.trim_end().len() != text.len())
    }

    /// `apply` – zero-copy when no trimming is required.
    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let trimmed = text.trim();
        if trimmed.len() == text.len() {
            Ok(text) // no allocation
        } else {
            Ok(Cow::Owned(trimmed.to_owned()))
        }
    }

    /// **Fused, zero-allocation iterator path**
    fn char_mapper(&self) -> Option<Box<dyn CharMapper + 'static>> {
        Some(Box::new(TrimWhitespaceMapper))
    }
}

/* --------------------------------------------------------------------- */
/*                     CharMapper implementation                         */
/* --------------------------------------------------------------------- */

/// The three phases we walk through while iterating.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TrimPhase {
    Leading,  // drop leading whitespace
    Middle,   // emit everything
    Trailing, // drop trailing whitespace (entered on first non-ws)
}

struct TrimWhitespaceMapper;

impl CharMapper for TrimWhitespaceMapper {
    #[inline(always)]
    fn map(&self, c: char, _: &Context) -> char {
        c
    }

    #[inline]
    fn needs_apply(&self, text: &str, _: &Context) -> bool {
        text.trim_start().len() != text.len() || text.trim_end().len() != text.len()
    }

    fn bind<'a>(&self, text: &'a str, _: &Context) -> Box<dyn Iterator<Item = char> + 'a> {
        Box::new(TrimIter {
            chars: text.chars(),
            phase: TrimPhase::Leading,
            emitted: false,
        })
    }
}

/* --------------------------------------------------------------------- */
/*                     Stateful iterator (zero-allocation)                */
/* --------------------------------------------------------------------- */

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
                        // we just saw the last non-ws char → switch to trailing
                        self.phase = TrimPhase::Trailing;
                    }
                    return Some(c);
                }
                TrimPhase::Trailing => {
                    // drop everything after the last non-ws char
                    continue;
                }
            }
        }
    }
}

impl<'a> FusedIterator for TrimIter<'a> {}
