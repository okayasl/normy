use crate::{
    context::Context,
    lang::{FoldMap, LocaleBehavior},
    stage::{CharMapper, Stage, StageError},
};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

pub struct CaseFold;

impl Stage for CaseFold {
    fn name(&self) -> &'static str {
        "case_fold"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        let fold_map = ctx.lang.fold_map();

        if fold_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if text.is_ascii() {
                return Ok(text.bytes().any(|b| b.is_ascii_uppercase()));
            }
            return Ok(text.chars().any(|c| c.to_lowercase().next() != Some(c)));
        }

        Ok(text
            .chars()
            .any(|c| fold_map.iter().any(|m| m.from == c) || c.to_lowercase().next() != Some(c)))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let fold_map = ctx.lang.fold_map();

        if fold_map.is_empty() {
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

        let mut out = String::with_capacity(text.len() * 2); // worst-case expansion: ß → ss
        for c in text.chars() {
            if let Some(map) = fold_map.iter().find(|m| m.from == c) {
                out.push_str(map.to);
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

impl CharMapper for CaseFold {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> char {
        let fold_map = ctx.lang.fold_map();

        if fold_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if c.is_ascii() {
                return c.to_ascii_lowercase();
            }
            return c.to_lowercase().next().unwrap_or(c);
        }

        // Only take first char for CharMapper (multi-char handled in apply)
        fold_map
            .iter()
            .find(|m| m.from == c)
            .map(|m| m.to.chars().next().unwrap_or(c))
            .unwrap_or_else(|| c.to_lowercase().next().unwrap_or(c))
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn Iterator<Item = char> + 'a> {
        let fold_map = ctx.lang.fold_map();

        if fold_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if text.is_ascii() {
                return Box::new(AsciiCaseFoldIter {
                    bytes: text.as_bytes(),
                });
            }
            return Box::new(text.chars().flat_map(|c| c.to_lowercase()));
        }

        Box::new(CaseFoldIter {
            chars: text.chars(),
            fold_map,
        })
    }
}

// ────── ASCII FAST PATH ITERATOR ──────
#[cfg(feature = "ascii-fast")]
struct AsciiCaseFoldIter<'a> {
    bytes: &'a [u8],
}

#[cfg(feature = "ascii-fast")]
impl<'a> Iterator for AsciiCaseFoldIter<'a> {
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
impl<'a> FusedIterator for AsciiCaseFoldIter<'a> {}

// ────── UNICODE / Multi-char CASE FOLD ITERATOR ──────
struct CaseFoldIter<'a> {
    chars: std::str::Chars<'a>,
    fold_map: &'a [FoldMap],
}

impl<'a> Iterator for CaseFoldIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let c = self.chars.next()?;
        if let Some(map) = self.fold_map.iter().find(|m| m.from == c) {
            return map.to.chars().next(); // only first char, rest handled in apply()
        }
        c.to_lowercase().next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}

impl<'a> FusedIterator for CaseFoldIter<'a> {}
