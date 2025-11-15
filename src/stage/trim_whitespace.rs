use crate::{
    context::Context,
    stage::{CharMapper, Stage, StageError},
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::str::Chars;
use std::sync::Arc;

pub struct TrimWhitespace;

impl Stage for TrimWhitespace {
    fn name(&self) -> &'static str {
        "trim_ws"
    }

    fn needs_apply(&self, text: &str, _: &Context) -> Result<bool, StageError> {
        // Fast path for ASCII
        let b = text.as_bytes();
        if b.first().is_some_and(u8::is_ascii_whitespace)
            || b.last().is_some_and(u8::is_ascii_whitespace)
        {
            return Ok(true);
        }

        // Fallback for Unicode whitespace
        Ok(text.chars().next().is_some_and(char::is_whitespace)
            || text.chars().next_back().is_some_and(char::is_whitespace))
    }

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

    #[inline]
    fn as_char_mapper(&self, _: &Context) -> Option<&dyn CharMapper> {
        Some(self)
    }
    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, _: &Context) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }
}

impl CharMapper for TrimWhitespace {
    #[inline(always)]
    fn map(&self, c: char, _: &Context) -> char {
        c
    }

    fn bind<'a>(&self, text: &'a str, _: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(TrimIter {
            chars: text.chars(),
            phase: TrimPhase::Leading,
            emitted_non_ws: false,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TrimPhase {
    Leading,
    Middle,
    Trailing,
}

struct TrimIter<'a> {
    chars: Chars<'a>,
    phase: TrimPhase,
    emitted_non_ws: bool,
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
                    self.emitted_non_ws = true;
                    return Some(c);
                }
                TrimPhase::Middle => {
                    if self.emitted_non_ws
                        && c.is_whitespace()
                        && self.chars.as_str().chars().all(char::is_whitespace)
                    {
                        self.phase = TrimPhase::Trailing;
                        continue;
                    }
                    return Some(c);
                }
                TrimPhase::Trailing => continue,
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}
impl<'a> FusedIterator for TrimIter<'a> {}
