//! stage/case_fold.rs – **Zero-copy, locale-accurate case folding**
//! * Turkish "İ → i / I → ı"
//! * German "ß → ss" (multi-char expansion)
//! * Dutch "IJ → ij" (two-char peek-ahead)
//! * Fast ASCII path (optional)
//! * CharMapper path **only when every mapping is 1→1 and no peek-ahead**
//! * Fully compliant with the white-paper §5.1, §5.2, §3.3

use crate::{
    context::Context,
    lang::LangEntry,
    stage::{CharMapper, FusedIterator, Stage, StageError},
};
use std::borrow::Cow;
use std::sync::Arc;

/// Public stage – zero-sized, stateless.
pub struct FoldCase;

impl Stage for FoldCase {
    fn name(&self) -> &'static str {
        "case_fold"
    }
    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        if text.chars().any(|c| ctx.lang_entry.needs_case_fold(c)) {
            return Ok(true);
        }
        if ctx.lang_entry.requires_peek_ahead() {
            let mut chars = text.chars().peekable();
            while let Some(c) = chars.next() {
                if ctx
                    .lang_entry
                    .peek_ahead_fold(c, chars.peek().copied())
                    .is_some()
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let fold_map = ctx.lang_entry.fold_map();

        // ═══════════════════════════════════════════════════════════════
        // Fast path: No language-specific rules → Unicode lowercase only
        // ═══════════════════════════════════════════════════════════════
        // Fast path: no language rules
        if fold_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if text.is_ascii() {
                let mut owned = text.into_owned();
                owned.make_ascii_lowercase();
                return Ok(Cow::Owned(owned));
            }
            return Ok(Cow::Owned(
                text.chars().flat_map(|c| c.to_lowercase()).collect(),
            ));
        }

        // ═══════════════════════════════════════════════════════════════
        // Context-sensitive path: Dutch IJ, or future multi-char sequences
        // ═══════════════════════════════════════════════════════════════
        if ctx.lang_entry.requires_peek_ahead() {
            return apply_with_peek_ahead(text, ctx);
        }

        // ═══════════════════════════════════════════════════════════════
        // Standard path: Language-specific folding without peek-ahead
        // ═══════════════════════════════════════════════════════════════
        let (foldable_count, extra_bytes) = ctx.lang_entry.count_foldable_bytes(&text);
        if foldable_count == 0 {
            return Ok(text); // Zero-copy
        }

        let capacity = text.len() + extra_bytes;
        let mut out = String::with_capacity(capacity);
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
        // Use lang.rs helpers instead of manual checks
        if ctx.lang_entry.has_one_to_one_folds() && !ctx.lang_entry.requires_peek_ahead() {
            Some(self)
        } else {
            None
        }
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        if ctx.lang_entry.has_one_to_one_folds() && !ctx.lang_entry.requires_peek_ahead() {
            Some(self)
        } else {
            None
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Peek-ahead implementation for Dutch IJ and similar sequences
// ═══════════════════════════════════════════════════════════════════════════
fn apply_with_peek_ahead<'a>(
    text: Cow<'a, str>,
    ctx: &Context,
) -> Result<Cow<'a, str>, StageError> {
    let fold_map = ctx.lang_entry.fold_map();
    let (foldable_count, extra_bytes) = ctx.lang_entry.count_foldable_bytes(&text); // Reuse helper
    let mut out = String::with_capacity(
        text.len()
            + extra_bytes
            + if ctx.lang_entry.requires_peek_ahead() {
                foldable_count
            } else {
                0
            },
    ); // Rough peek adjustment
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(target) = ctx.lang_entry.peek_ahead_fold(c, chars.peek().copied()) {
            chars.next();
            out.push_str(target);
            continue;
        }
        if let Some(m) = fold_map.iter().find(|m| m.from == c) {
            out.push_str(m.to);
        } else {
            out.extend(c.to_lowercase());
        }
    }
    Ok(Cow::Owned(out))
}

// ═══════════════════════════════════════════════════════════════════════════
// CharMapper implementation (zero-copy path)
// ═══════════════════════════════════════════════════════════════════════════
impl CharMapper for FoldCase {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> Option<char> {
        // Use lang.rs helper for 1→1 folding
        ctx.lang_entry.fold_char(c)
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        let fold_map = ctx.lang_entry.fold_map();

        // Fast path: no language-specific rules
        if fold_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if text.is_ascii() {
                return Box::new(AsciiFoldCaseIter {
                    bytes: text.as_bytes(),
                });
            }
            return Box::new(text.chars().flat_map(|c| c.to_lowercase()));
        }

        // Language-specific 1→1 iterator
        Box::new(FoldCaseIter {
            chars: text.chars(),
            lang: ctx.lang_entry,
        })
    }
}

// ────── ASCII FAST PATH ITERATOR ──────
#[cfg(feature = "ascii-fast")]
struct AsciiFoldCaseIter<'a> {
    bytes: &'a [u8],
}

#[cfg(feature = "ascii-fast")]
impl<'a> Iterator for AsciiFoldCaseIter<'a> {
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
impl<'a> FusedIterator for AsciiFoldCaseIter<'a> {}

struct FoldCaseIter<'a> {
    chars: std::str::Chars<'a>,
    lang: LangEntry,
}

impl<'a> Iterator for FoldCaseIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let c = self.chars.next()?;
        // Use lang.rs helper for 1→1 folding
        self.lang.fold_char(c)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}

impl<'a> FusedIterator for FoldCaseIter<'a> {}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::data::{DEU, ENG, NLD, TUR};

    #[test]
    fn test_english_basic() {
        let stage = FoldCase;
        let ctx = Context::new(ENG);

        assert!(stage.needs_apply("HELLO", &ctx).unwrap());
        assert!(!stage.needs_apply("hello", &ctx).unwrap());

        let result = stage.apply(Cow::Borrowed("HELLO"), &ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_turkish_dotted_i() {
        let stage = FoldCase;
        let ctx = Context::new(TUR);

        let result = stage.apply(Cow::Borrowed("İSTANBUL"), &ctx).unwrap();
        assert_eq!(result, "istanbul");

        let result = stage.apply(Cow::Borrowed("ISPARTA"), &ctx).unwrap();
        assert_eq!(result, "ısparta"); // Turkish I → ı
    }

    #[test]
    fn test_german_eszett() {
        let stage = FoldCase;
        let ctx = Context::new(DEU);

        let result = stage.apply(Cow::Borrowed("Straße"), &ctx).unwrap();
        assert_eq!(result, "strasse");

        let result = stage.apply(Cow::Borrowed("GROẞ"), &ctx).unwrap();
        assert_eq!(result, "gross");
    }

    #[test]
    fn test_dutch_ij_uppercase() {
        let stage = FoldCase;
        let ctx = Context::new(NLD);

        // Two-char sequence "IJ"
        let result = stage.apply(Cow::Borrowed("IJssel"), &ctx).unwrap();
        assert_eq!(result, "ijssel");

        let result = stage.apply(Cow::Borrowed("IJZER"), &ctx).unwrap();
        assert_eq!(result, "ijzer");
    }

    #[test]
    fn test_dutch_ij_lowercase() {
        let stage = FoldCase;
        let ctx = Context::new(NLD);

        // Already lowercase
        let result = stage.apply(Cow::Borrowed("ijssel"), &ctx).unwrap();
        assert_eq!(result, "ijssel");
    }

    #[test]
    fn test_dutch_ij_ligature() {
        let stage = FoldCase;
        let ctx = Context::new(NLD);

        // Ligature 'Ĳ' (U+0132)
        let result = stage.apply(Cow::Borrowed("Ĳssel"), &ctx).unwrap();
        assert_eq!(result, "ijssel");
    }

    #[test]
    fn test_dutch_ij_not_sequence() {
        let stage = FoldCase;
        let ctx = Context::new(NLD);

        // "IK" should not trigger peek-ahead
        let result = stage.apply(Cow::Borrowed("IK"), &ctx).unwrap();
        assert_eq!(result, "ik");
    }

    #[test]
    fn test_dutch_ij_idempotency() {
        let stage = FoldCase;
        let ctx = Context::new(NLD);

        let text = "IJssel";
        let first = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        let second = stage.apply(Cow::Borrowed(&first), &ctx).unwrap();

        assert_eq!(first, "ijssel");
        assert_eq!(first, second, "Should be idempotent");
    }

    #[test]
    fn test_char_mapper_eligibility() {
        let stage = FoldCase;

        // English: 1→1, no peek-ahead → CharMapper eligible
        let ctx = Context::new(ENG);
        assert!(stage.as_char_mapper(&ctx).is_some());

        // Turkish: 1→1, no peek-ahead → CharMapper eligible
        let ctx = Context::new(TUR);
        assert!(stage.as_char_mapper(&ctx).is_some());

        // German: multi-char (ß→ss) → NOT eligible
        let ctx = Context::new(DEU);
        assert!(stage.as_char_mapper(&ctx).is_none());

        // Dutch: peek-ahead required → NOT eligible
        let ctx = Context::new(NLD);
        assert!(stage.as_char_mapper(&ctx).is_none());
    }

    #[test]
    #[cfg(feature = "ascii-fast")]
    fn test_ascii_fast_path() {
        let stage = FoldCase;
        let ctx = Context::new(ENG);

        let result = stage.apply(Cow::Borrowed("HELLO123"), &ctx).unwrap();
        assert_eq!(result, "hello123");
    }

    #[test]
    fn test_dutch_ij_uppercase_needs_apply() {
        let stage = FoldCase;
        let ctx = Context::new(NLD); // Dutch requires peek-ahead for "IJ"

        // "IJ" is a two-character sequence that must be folded to "ij"
        let text = "IJssel";
        assert!(
            stage.needs_apply(text, &ctx).unwrap(),
            "needs_apply must return true for 'IJssel' in Dutch"
        );
        let result = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        assert_eq!(result, "ijssel");
    }

    #[test]
    fn test_dutch_ligature_needs_apply() {
        let stage = FoldCase;
        let ctx = Context::new(NLD);
        let text = "Ĳssel"; // Ĳ (U+0132) is the single-char ligature → must fold to "ijssel"
        assert!(stage.needs_apply(text, &ctx).unwrap()); // Passes if slice contains
        let result = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        assert_eq!(result, "ijssel");
    }

    #[test]
    fn test_german_eszett_lowercase_needs_apply() {
        let stage = FoldCase;
        let ctx = Context::new(DEU);
        let text = "straße"; // All lowercase, but 'ß' → "ss" for case-fold
        assert!(stage.needs_apply(text, &ctx).unwrap()); // Passes
        let result = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        assert_eq!(result, "strasse");
    }

    #[test]
    fn test_dutch_ij_lowercase_needs_apply() {
        let stage = FoldCase;
        let ctx = Context::new(NLD);
        let text = "ijssel"; // Already "ij", but if peek_ahead_fold checks lowercase
        assert!(!stage.needs_apply(text, &ctx).unwrap()); // Should be false
    }

    #[test]
    fn test_dutch_german_charmapper_contract() {
        let ctx_nld = Context::new(NLD);
        let ctx_deu = Context::new(DEU);

        // These MUST be None or CharMapper will break
        assert!(
            FoldCase.as_char_mapper(&ctx_nld).is_none(),
            "CRITICAL: Dutch needs peek-ahead, cannot use CharMapper"
        );

        assert!(
            FoldCase.as_char_mapper(&ctx_deu).is_none(),
            "CRITICAL: German has multi-char folds, cannot use CharMapper"
        );
    }
}
