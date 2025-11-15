//! stage/case_fold.rs – **Zero-copy, locale-accurate case folding**
//! * Turkish “İ → i / I → ı”
//! * German “ß → ss” (multi-char expansion)
//! * Fast ASCII path (optional)
//! * CharMapper path **only when every mapping is 1→1**
//! * Fully compliant with the white-paper §5.1, §5.2, §3.3

use crate::{
    context::Context,
    lang::{FoldMap, LocaleBehavior},
    stage::{CharMapper, FusedIterator, Stage, StageError},
};
use std::borrow::Cow;
use std::sync::Arc;

/// Public stage – zero-sized, stateless.
pub struct CaseFold;

impl Stage for CaseFold {
    fn name(&self) -> &'static str {
        "case_fold"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        let fold_map = ctx.lang.fold_map();

        // Fast-path: no language-specific folding → rely on Unicode lowercase
        if fold_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if text.is_ascii() {
                return Ok(text.bytes().any(|b| b.is_ascii_uppercase()));
            }
            return Ok(text.chars().any(|c| c.to_lowercase().next() != Some(c)));
        }

        // Language-specific folding present
        Ok(text
            .chars()
            .any(|c| fold_map.iter().any(|m| m.from == c) || c.to_lowercase().next() != Some(c)))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let fold_map = ctx.lang.fold_map();

        // No language-specific rules → use Unicode lowercase
        if fold_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if text.is_ascii() {
                let mut bytes = text.into_owned().into_bytes();
                for b in &mut bytes {
                    if b.is_ascii_uppercase() {
                        *b = b.to_ascii_lowercase();
                    }
                }
                return Ok(Cow::Owned(unsafe { String::from_utf8_unchecked(bytes) }));
            }

            return Ok(Cow::Owned(
                text.chars().flat_map(|c| c.to_lowercase()).collect(),
            ));
        }

        // Language-specific folding (may expand: ß → "ss")
        let mut out = String::with_capacity(text.len() * 2);
        for c in text.chars() {
            if let Some(m) = fold_map.iter().find(|m| m.from == c) {
                out.push_str(m.to);
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
            None
        } else {
            Some(self)
        }
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        let fold_map = ctx.lang.fold_map();
        if fold_map.is_empty() || fold_map.iter().any(|m| m.to.len() > 1) {
            None
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

        // 1→1 only: take first char of mapping
        fold_map
            .iter()
            .find(|m| m.from == c)
            .and_then(|m| m.to.chars().next())
            .unwrap_or_else(|| c.to_lowercase().next().unwrap_or(c))
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
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

// ────── UNICODE / 1→1 CASE FOLD ITERATOR ──────
struct CaseFoldIter<'a> {
    chars: std::str::Chars<'a>,
    fold_map: &'a [FoldMap],
}

impl<'a> Iterator for CaseFoldIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let c = self.chars.next()?;
        self.fold_map
            .iter()
            .find(|m| m.from == c)
            .and_then(|m| m.to.chars().next())
            .or_else(|| c.to_lowercase().next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}

impl<'a> FusedIterator for CaseFoldIter<'a> {}
