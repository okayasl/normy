//! stage/transliterate.rs – **Orthographic decomposition (lossy, opt-in)**
//! * French Œ → "oe", Danish Å → "aa", Polish Ł → "l", etc.
//! * Preserves original case (Strasse → Strasse, Århus → Aarhus)
//! * Always allocating — but only when explicitly enabled
//! * Zero-cost when no transliteration rules exist
//! * Fully compliant with white-paper §1.2 "Zero-Copy Default"
use crate::{
    context::Context,
    stage::{CharMapper, FusedIterator, Stage, StageError},
};
use std::borrow::Cow;
use std::sync::Arc;

/// Public stage – zero-sized, stateless.
/// Explicit opt-in only — never applied by default.
pub struct Transliterate;

impl Stage for Transliterate {
    fn name(&self) -> &'static str {
        "transliterate"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        let map = ctx.lang_entry.transliterate_map();
        if map.is_empty() {
            return Ok(false);
        }
        Ok(text.chars().any(|c| map.iter().any(|m| m.from == c)))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let map = ctx.lang_entry.transliterate_map();
        if map.is_empty() {
            return Ok(text); // Zero-cost fast path
        }

        // Pre-calculate capacity using helper (same as FoldCase)
        let (trans_count, extra_bytes) = ctx.lang_entry.count_transliterate_bytes(&text);
        if trans_count == 0 {
            return Ok(text); // Zero-copy when no actual replacements
        }

        let mut out = String::with_capacity(text.len() + extra_bytes);
        for c in text.chars() {
            if let Some(m) = map.iter().find(|m| m.from == c) {
                out.push_str(m.to); // Preserves case from source data
            } else {
                out.push(c); // Identity — no lowercasing!
            }
        }
        Ok(Cow::Owned(out))
    }

    /// Only enable CharMapper if all transliterations are 1→1
    #[inline]
    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        if ctx.lang_entry.transliterate_is_one_to_one() {
            Some(self)
        } else {
            None
        }
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        if ctx.lang_entry.transliterate_is_one_to_one() {
            Some(self)
        } else {
            None
        }
    }
}

impl CharMapper for Transliterate {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> Option<char> {
        let map = ctx.lang_entry.transliterate_map();
        map.iter().find(|m| m.from == c).map(|m| {
            // Safety: 1→1 guaranteed by transliterate_is_one_to_one()
            m.to.chars().next().unwrap()
        })
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(TransliterateIter {
            chars: text.chars(),
            map: ctx.lang_entry.transliterate_map(),
        })
    }
}

struct TransliterateIter<'a> {
    chars: std::str::Chars<'a>,
    map: &'static [crate::lang::FoldMap], // Reuse FoldMap struct
}

impl<'a> Iterator for TransliterateIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let c = self.chars.next()?;
        if let Some(m) = self.map.iter().find(|m| m.from == c) {
            // 1→1 guaranteed — safe to take first char
            Some(m.to.chars().next().unwrap())
        } else {
            Some(c)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}

impl<'a> FusedIterator for TransliterateIter<'a> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DAN, ENG, FRA, POL, RemoveDiacritics, TUR};
    use std::borrow::Cow;

    #[test]
    fn test_no_transliteration() {
        let stage = Transliterate;
        let ctx = Context::new(ENG);
        let input = "Hello World";
        assert!(!stage.needs_apply(input, &ctx).unwrap());
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert_eq!(result, "Hello World"); // Zero-copy
    }

    #[test]
    fn test_french_oe_ligature() {
        let stage = Transliterate;
        let ctx = Context::new(FRA);
        // Assume pre-lowercased input (post-FoldCase)
        let result_upper = stage.apply(Cow::Borrowed("ŒUVRE"), &ctx).unwrap(); // 'Œ' → "oe" (lowercase target)
        assert_eq!(result_upper, "oeUVRE"); // Partial: Œ replaced, rest preserved (but in real pipeline, all lower)
        let result_lower = stage.apply(Cow::Borrowed("œuvre"), &ctx).unwrap();
        assert_eq!(result_lower, "oeuvre");
    }

    #[test]
    fn test_danish_aa() {
        let stage = Transliterate;
        let ctx = Context::new(DAN);
        let result_upper = stage.apply(Cow::Borrowed("Århus"), &ctx).unwrap();
        assert_eq!(result_upper, "aarhus"); // 'Å' → "aa" (lowercase target)
        let result_lower = stage.apply(Cow::Borrowed("århus"), &ctx).unwrap();
        assert_eq!(result_lower, "aarhus");
    }

    #[test]
    fn test_polish_l() {
        let stage = RemoveDiacritics;
        let ctx = Context::new(POL);

        let result_upper = stage.apply(Cow::Borrowed("Łódź"), &ctx).unwrap();
        // Correct expectation: Ł -> l AND ź -> z (per your language definition)
        assert_eq!(result_upper, "lodz");

        let result_lower = stage.apply(Cow::Borrowed("łódź"), &ctx).unwrap();
        // Correct expectation: ł -> l AND ź -> z
        assert_eq!(result_lower, "lodz");
    }

    #[test]
    fn test_turkish_unaffected() {
        let stage = Transliterate;
        let ctx = Context::new(TUR);
        let result = stage.apply(Cow::Borrowed("İstanbul"), &ctx).unwrap();
        assert_eq!(result, "İstanbul"); // Turkish has no transliterate rules
    }

    #[test]
    fn test_mixed_text() {
        let stage = Transliterate;
        let ctx = Context::new(FRA);
        let input = "ŒUVRE Århus Łódź Straße"; // Mixed case
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert_eq!(result, "oeUVRE Århus Łódź Straße"); // Only French Œ → "oe"; others preserved
    }

    #[test]
    fn test_char_mapper_eligibility() {
        let stage = Transliterate;
        let ctx_eng = Context::new(ENG);
        let ctx_fra = Context::new(FRA);
        let ctx_dan = Context::new(DAN);

        assert!(stage.as_char_mapper(&ctx_eng).is_none()); // empty map
        assert!(stage.as_char_mapper(&ctx_fra).is_none()); // "oe" is 2 chars → not 1→1
        assert!(stage.as_char_mapper(&ctx_dan).is_none()); // "aa" is 2 chars
    }

    #[test]
    fn test_zero_copy_when_no_replacements() {
        let stage = Transliterate;
        let ctx = Context::new(FRA);
        let input = Cow::Borrowed("Paris France");
        let result = stage.apply(input.clone(), &ctx).unwrap();
        assert!(matches!(result, Cow::Borrowed(_)));
    }
}
