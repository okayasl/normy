use crate::{
    context::Context,
    lang::LangEntry,
    stage::{CharMapper, FusedIterator, Stage, StageError},
};
use std::borrow::Cow;
use std::sync::Arc;

/// Locale-sensitive case folding for search and comparison.
///
/// `CaseFold` performs full Unicode case folding with language-specific rules,
/// including:
/// - Multi-character expansions (e.g. German `ß` → `"ss"`, `ẞ` → `"ss"`)
/// - Context-sensitive mappings via peek-ahead (e.g. Dutch `IJ` → `"ij"`)
/// - Locale-aware lowercase mapping using `case_map` (e.g. Turkish `İ` → `i`, `I` → `ı`)
/// - Fallback to Unicode full case folding (`.to_lowercase()` + compatibility mappings)
///
/// This stage is intended for information retrieval, search indexing, and any
/// scenario requiring case-insensitive matching that respects linguistic norms.
/// It is stronger than simple lowercasing but weaker than NFKC/NFKD.
///
/// When the target language has only one-to-one mappings and no peek-ahead rules,
/// this stage implements `CharMapper`, enabling zero-allocation pipeline fusion.
pub struct CaseFold;

impl Stage for CaseFold {
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
        if ctx.lang_entry.requires_peek_ahead() {
            return apply_with_peek_ahead(text, ctx);
        }
        let (_foldable_count, extra_bytes) = ctx.lang_entry.count_foldable_bytes(&text);
        let mut out = String::with_capacity(text.len() + extra_bytes);
        for c in text.chars() {
            match ctx.lang_entry.fold_char(c) {
                Some(ch) => out.push(ch),
                None => {
                    let expanded = ctx
                        .lang_entry
                        .fold_map()
                        .iter()
                        .find(|m| m.from == c)
                        .expect("inconsistent fold_map: missing multi-char expansion")
                        .to;
                    out.push_str(expanded);
                }
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
    );
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

impl CharMapper for CaseFold {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> Option<char> {
        ctx.lang_entry.fold_char(c)
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(CaseFoldIter {
            chars: text.chars(),
            lang: ctx.lang_entry,
        })
    }
}

struct CaseFoldIter<'a> {
    chars: std::str::Chars<'a>,
    lang: LangEntry,
}

impl<'a> Iterator for CaseFoldIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let c = self.chars.next()?;
        self.lang.fold_char(c)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}

impl<'a> FusedIterator for CaseFoldIter<'a> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::data::{DEU, ENG, FRA, NLD, TUR};

    #[test]
    fn test_english_basic() {
        let stage = CaseFold;
        let ctx = Context::new(ENG);
        assert!(stage.needs_apply("HELLO", &ctx).unwrap());
        assert!(!stage.needs_apply("hello", &ctx).unwrap());
        let result = stage.apply(Cow::Borrowed("HELLO"), &ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_french_basic() {
        let stage = CaseFold;
        let ctx = Context::new(FRA);
        assert!(stage.needs_apply("Café", &ctx).unwrap());
        let result = stage.apply(Cow::Borrowed("Café"), &ctx).unwrap();
        assert_eq!(result, "café");
    }

    #[test]
    fn test_german_eszett() {
        let stage = CaseFold;
        let ctx = Context::new(DEU);
        let result = stage.apply(Cow::Borrowed("Straße"), &ctx).unwrap();
        assert_eq!(result, "strasse");
        let result = stage.apply(Cow::Borrowed("GROẞ"), &ctx).unwrap();
        assert_eq!(result, "gross");
    }

    #[test]
    fn test_dutch_ij_uppercase() {
        let stage = CaseFold;
        let ctx = Context::new(NLD);
        // Two-char sequence "IJ"
        let result = stage.apply(Cow::Borrowed("IJssel"), &ctx).unwrap();
        assert_eq!(result, "ijssel");
        let result = stage.apply(Cow::Borrowed("IJZER"), &ctx).unwrap();
        assert_eq!(result, "ijzer");
    }

    #[test]
    fn test_dutch_ij_lowercase() {
        let stage = CaseFold;
        let ctx = Context::new(NLD);
        // Already lowercase
        let result = stage.apply(Cow::Borrowed("ijssel"), &ctx).unwrap();
        assert_eq!(result, "ijssel");
    }

    #[test]
    fn test_dutch_ij_ligature() {
        let stage = CaseFold;
        let ctx = Context::new(NLD);
        // Ligature 'Ĳ' (U+0132)
        let result = stage.apply(Cow::Borrowed("Ĳssel"), &ctx).unwrap();
        assert_eq!(result, "ijssel");
    }

    #[test]
    fn test_dutch_ij_not_sequence() {
        let stage = CaseFold;
        let ctx = Context::new(NLD);
        // "IK" should not trigger peek-ahead
        let result = stage.apply(Cow::Borrowed("IK"), &ctx).unwrap();
        assert_eq!(result, "ik");
    }

    #[test]
    fn test_dutch_ij_idempotency() {
        let stage = CaseFold;
        let ctx = Context::new(NLD);
        let text = "IJssel";
        let first = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        let second = stage.apply(Cow::Borrowed(&first), &ctx).unwrap();
        assert_eq!(first, "ijssel");
        assert_eq!(first, second, "Should be idempotent");
    }

    #[test]
    fn test_char_mapper_eligibility() {
        let stage = CaseFold;
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
    fn test_dutch_ij_uppercase_needs_apply() {
        let stage = CaseFold;
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
        let stage = CaseFold;
        let ctx = Context::new(NLD);
        let text = "Ĳssel"; // Ĳ (U+0132) is the single-char ligature → must fold to "ijssel"
        assert!(stage.needs_apply(text, &ctx).unwrap()); // Passes if slice contains
        let result = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        assert_eq!(result, "ijssel");
    }

    #[test]
    fn test_german_eszett_lowercase_needs_apply() {
        let stage = CaseFold;
        let ctx = Context::new(DEU);
        let text = "straße"; // All lowercase, but 'ß' → "ss" for case-fold
        assert!(stage.needs_apply(text, &ctx).unwrap()); // Passes
        let result = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        assert_eq!(result, "strasse");
    }

    #[test]
    fn test_dutch_ij_lowercase_needs_apply() {
        let stage = CaseFold;
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
            CaseFold.as_char_mapper(&ctx_nld).is_none(),
            "CRITICAL: Dutch needs peek-ahead, cannot use CharMapper"
        );
        assert!(
            CaseFold.as_char_mapper(&ctx_deu).is_none(),
            "CRITICAL: German has multi-char folds, cannot use CharMapper"
        );
    }
}
