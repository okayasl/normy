use crate::{
    context::Context,
    lang::{CaseMap, LocaleBehavior},
    stage::{CharMapper, Stage, StageError},
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

pub struct Lowercase;

impl Stage for Lowercase {
    fn name(&self) -> &'static str {
        "lowercase"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        let case_map = ctx.lang.case_map();

        if case_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if text.is_ascii() {
                return Ok(text.bytes().any(|b| b.is_ascii_uppercase()));
            }
            return Ok(text.chars().any(|c| c.to_lowercase().next() != Some(c)));
        }

        Ok(text
            .chars()
            .any(|c| case_map.iter().any(|m| m.from == c) || c.to_lowercase().next() != Some(c)))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let case_map = ctx.lang.case_map();

        if case_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if text.is_ascii() {
                let mut out = text.into_owned().into_bytes();
                for b in &mut out {
                    if b.is_ascii_uppercase() {
                        *b = b.to_ascii_lowercase();
                    }
                }
                return Ok(Cow::Owned(unsafe { String::from_utf8_unchecked(out) }));
            }
            return Ok(Cow::Owned(
                text.chars().flat_map(|c| c.to_lowercase()).collect(),
            ));
        }

        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            if let Some(map) = case_map.iter().find(|m| m.from == c) {
                out.push(map.to); // ← 1:1 only
            } else {
                out.extend(c.to_lowercase());
            }
        }
        Ok(Cow::Owned(out))
    }

    #[inline]
    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        let fold_map = ctx.lang.fold_map();
        if fold_map.is_empty() || fold_map.iter().any(|m| m.to.len() > 1) {
            None // ← Critical: disable for ß → "ss"
        } else {
            Some(self)
        }
    }
    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        let fold_map = ctx.lang.fold_map();
        if fold_map.is_empty() || fold_map.iter().any(|m| m.to.len() > 1) {
            None // ← Critical: disable for ß → "ss"
        } else {
            Some(self)
        }
    }
}

impl CharMapper for Lowercase {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> Option<char> {
        let case_map = ctx.lang.case_map();
        if case_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if c.is_ascii() {
                return Some(c.to_ascii_lowercase());
            }
            return Some(c.to_lowercase().next().unwrap_or(c));
        }
        Some(
            case_map
                .iter()
                .find(|m| m.from == c)
                .map(|m| m.to)
                .unwrap_or_else(|| c.to_lowercase().next().unwrap_or(c)),
        )
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        let case_map = ctx.lang.case_map();

        if case_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if text.is_ascii() {
                return Box::new(AsciiLowercaseIter {
                    bytes: text.as_bytes(),
                });
            }
            return Box::new(text.chars().flat_map(|c| c.to_lowercase()));
        }

        Box::new(LowercaseIter {
            chars: text.chars(),
            case_map,
        })
    }
}

// ────── ASCII FAST PATH ITERATOR ──────
#[cfg(feature = "ascii-fast")]
struct AsciiLowercaseIter<'a> {
    bytes: &'a [u8],
}

#[cfg(feature = "ascii-fast")]
impl<'a> Iterator for AsciiLowercaseIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let (&b, rest) = self.bytes.split_first()?;
        self.bytes = rest;
        Some(if b.is_ascii_uppercase() {
            b.to_ascii_lowercase() as char
        } else {
            b as char
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.bytes.len(), Some(self.bytes.len()))
    }
}

#[cfg(feature = "ascii-fast")]
impl<'a> FusedIterator for AsciiLowercaseIter<'a> {}

// ────── UNICODE FALLBACK ITERATOR ──────
struct LowercaseIter<'a> {
    chars: std::str::Chars<'a>,
    case_map: &'a [CaseMap],
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
