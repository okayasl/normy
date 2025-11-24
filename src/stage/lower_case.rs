use crate::{
    context::Context,
    lang::LangEntry,
    stage::{CharMapper, Stage, StageError},
};
use std::iter::FusedIterator;
use std::sync::Arc;
use std::{borrow::Cow, str::Chars};

/// Simple, locale-aware lowercase transformation.
///
/// This stage performs **strict linguistic lowercasing** using only the language’s
/// `case_map` (guaranteed 1→1) and falls back to Unicode `.to_lowercase()` for
/// unmapped characters.
///
/// # Design Philosophy
///
/// Unlike many normalization libraries that silently conflate "visual lowercase"
/// with "search equivalence", **Normy refuses to lie to you**.
///
/// `LowerCase` does **exactly one thing**: produce the correct orthographic
/// lowercase form of text in the target language — nothing more, nothing less.
/// It is intentionally **not** suitable for case-insensitive search in languages
/// with exceptional case-folding rules (e.g. Turkish, Azerbaijani, Lithuanian).
///
/// This is a deliberate, principled choice: **zero-cost wins by default**.
///
/// # Key Differences from `CaseFold`
///
/// | Aspect                  | `LowerCase`                                   | `CaseFold`                                            |
/// |-------------------------|------------------------------------------------|-------------------------------------------------------|
/// | Purpose                 | Visual / orthographic normalization           | Case-insensitive matching & search                    |
/// | Turkish `I` / `İ`       | `I` → `ı`, `İ` → `i` (correct lowercase)      | Same (uses `case_map`)                                |
/// | German `ẞ` / `ß`        | `ẞ` → `ß` (preserved)                         | `ẞ`/`ß` → `"ss"` (expanded)                           |
/// | Dutch `IJ` digraph      | `IJ` → `ij` (per-char)                        | `IJ` → `"ij"` (peek-ahead aware)                      |
/// | Multi-character output  | Never                                         | Yes (e.g. `ß` → `"ss"`)                               |
/// | Zero-allocation path    | **Always** — implements `CharMapper`          | Only when no multi-char or peek-ahead rules           |
/// | Search-safe in Turkish? | **No** — `"Istanbul"` ≠ `"İSTANBUL"`          | Yes — if no conflicting `fold:` rules (currently safe)|
///
/// # When to Use
///
/// - Display text, slugs, filenames, UI sorting
/// - Preprocessing before NFKC/NFKD
/// - Any pipeline where linguistic correctness > search recall
///
/// Use `CaseFold` when you need case-insensitive matching that works correctly
/// across all languages — including Turkish.
pub struct LowerCase;

impl Stage for LowerCase {
    fn name(&self) -> &'static str {
        "lowercase"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        Ok(text.chars().any(|c| ctx.lang_entry.needs_lowercase(c)))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let mut out = String::with_capacity(text.len());
        for c in text.chars() {
            out.push(ctx.lang_entry.lowercase_char(c));
        }
        Ok(Cow::Owned(out))
    }

    #[inline]
    fn as_char_mapper(&self, _ctx: &Context) -> Option<&dyn CharMapper> {
        Some(self)
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, _ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }
}

impl CharMapper for LowerCase {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> Option<char> {
        Some(ctx.lang_entry.lowercase_char(c))
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(LowercaseIter {
            chars: text.chars(),
            lang: ctx.lang_entry,
        })
    }
}
struct LowercaseIter<'a> {
    chars: Chars<'a>,
    lang: LangEntry,
}

impl<'a> Iterator for LowercaseIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let c = self.chars.next()?;
        Some(self.lang.lowercase_char(c))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}

impl<'a> FusedIterator for LowercaseIter<'a> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::data::{DEU, ENG, NLD, TUR};

    #[test]
    fn test_english_basic() {
        let stage = LowerCase;
        let ctx = Context::new(ENG);

        assert!(stage.needs_apply("HELLO", &ctx).unwrap());
        assert!(!stage.needs_apply("hello", &ctx).unwrap());

        let result = stage.apply(Cow::Borrowed("HELLO"), &ctx).unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_turkish_dotted_i() {
        let stage = LowerCase;
        let ctx = Context::new(TUR);

        // Turkish İ → i
        let result = stage.apply(Cow::Borrowed("İSTANBUL"), &ctx).unwrap();
        assert_eq!(result, "istanbul");

        // Turkish I → ı (not i)
        let result = stage.apply(Cow::Borrowed("ISPARTA"), &ctx).unwrap();
        assert_eq!(result, "ısparta");
    }

    #[test]
    fn test_german_eszett_not_expanded() {
        let stage = LowerCase;
        let ctx = Context::new(DEU);

        // Lowercase does NOT expand ß → ss (that's case_fold's job)
        let result = stage.apply(Cow::Borrowed("STRAẞE"), &ctx).unwrap();
        assert_eq!(result, "straße"); // ẞ → ß (lowercase), not "ss"
    }

    #[test]
    fn test_dutch_ij_no_digraph_handling() {
        let stage = LowerCase;
        let ctx = Context::new(NLD);

        // Lowercase does NOT treat IJ as digraph (that's case_fold's job)
        // Just lowercase each character independently
        let result = stage.apply(Cow::Borrowed("IJssel"), &ctx).unwrap();
        assert_eq!(result, "ijssel"); // I→i, J→j separately

        // Ligature still works (it's in fold_map but also Unicode lowercase)
        let result = stage.apply(Cow::Borrowed("Ĳssel"), &ctx).unwrap();
        assert_eq!(result, "ĳssel"); // Ĳ → ĳ (Unicode lowercase of ligature)
    }

    #[test]
    fn test_char_mapper_always_eligible() {
        let stage = LowerCase;

        // Lowercase is always 1→1, so always eligible for CharMapper
        assert!(stage.as_char_mapper(&Context::new(ENG)).is_some());
        assert!(stage.as_char_mapper(&Context::new(TUR)).is_some());
        assert!(stage.as_char_mapper(&Context::new(DEU)).is_some());
        assert!(stage.as_char_mapper(&Context::new(NLD)).is_some());
    }

    #[test]
    fn test_idempotency() {
        let stage = LowerCase;
        let ctx = Context::new(TUR);

        let text = "İSTANBUL";
        let first = stage.apply(Cow::Borrowed(text), &ctx).unwrap();
        let second = stage.apply(Cow::Borrowed(&first), &ctx).unwrap();

        assert_eq!(first, "istanbul");
        assert_eq!(first, second, "Should be idempotent");
    }

    #[test]
    fn test_difference_from_case_fold() {
        // Demonstrate the key difference between lowercase and case_fold

        // 1. German ß: lowercase preserves it, case_fold expands it
        let lowercase = LowerCase;
        let ctx = Context::new(DEU);

        let result = lowercase.apply(Cow::Borrowed("GROẞ"), &ctx).unwrap();
        assert_eq!(result, "groß"); // NOT "gross"

        // 2. Dutch IJ: lowercase treats separately, case_fold treats as digraph
        let ctx = Context::new(NLD);
        let result = lowercase.apply(Cow::Borrowed("IJssel"), &ctx).unwrap();
        assert_eq!(result, "ijssel"); // Just I→i, J→j (no digraph handling)
    }
}
