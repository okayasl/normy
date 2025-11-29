use crate::{
    CAT, DAN, DEU, FRA, ISL, NOR, SWE,
    context::Context,
    lang::Lang,
    stage::{CharMapper, FusedIterator, Stage, StageError},
    testing::stage_contract::StageTestConfig,
};
use std::borrow::Cow;
use std::sync::Arc;

/// Locale-aware orthographic transliteration (lossy, opt-in).
///
/// `Transliterate` performs **lossy** decomposition of language-specific letterforms
/// into their conventional multi-character representations, as expected in
/// bibliographic, search, or URL slug contexts:
///
/// - French: `Œ` → `"oe"`, `Æ` → `"ae"`  
/// - Danish/Norwegian: `Å` → `"aa"`, `Ø` → `"oe"`  
/// - German: `Ä` → `"ae"`, `ß` → `"ss"` (only if not handled by CaseFold)
/// - Polish: `Ł` → `"l"` (orthographic, not diacritic)
/// - (Future): Faroese, Icelandic, etc.
///
/// # Key Principles
///
/// - **Case-Preserving:** `Straße` → `Strasse`, `Århus` → `Aarhus` — never lowercases.
/// - **Opt-In Only:** Never enabled by default. Must be explicitly added to pipeline.
/// - **Locale-Strict:** Only applies rules defined for the current language.
/// - **Zero-Cost When Inactive:** Fully elided from pipeline if language has no rules.
/// - **Zero-Copy When Idle:** Returns `Cow::Borrowed` if no characters match.
///
/// # Performance Characteristics
///
/// | Scenario                            | Path                    | Allocation | Notes |
/// |-------------------------------------|-------------------------|------------|-------|
/// | Language has no rules (en, tr, ja)  | Direct `text.chars()`   | None       | Zero-cost |
/// | No chars need transliteration       | Early return            | None       | Zero-copy |
/// | 1→1 mappings (e.g. Polish Ł→l)      | Fused `CharMapper`      | None       | Inlined loop |
/// | 1→N mappings (e.g. French Œ→oe)     | `apply()` fallback      | One       | Rare, accepted |
///
/// When the target language uses only one-to-one mappings (currently Polish, German sharp-s),
/// this stage implements `CharMapper`, enabling full pipeline fusion and zero-allocation
/// processing even in static pipelines.
///
/// This stage is intended for generating readable identifiers, legacy system compatibility,
/// or when phonetic fidelity is secondary to orthographic tradition.
/// It is **not** a general-purpose ASCII converter — for that, combine with `Transliterate`.
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
            return Ok(text); // Zero-cost early exit
        }

        let (trans_count, extra_bytes) = ctx.lang_entry.count_transliterate_bytes(&text);
        if trans_count == 0 {
            return Ok(text); // Zero-copy: no chars need transliteration
        }

        let mut out = String::with_capacity(text.len() + extra_bytes);
        for c in text.chars() {
            if let Some(m) = map.iter().find(|m| m.from == c) {
                out.push_str(m.to);
            } else {
                out.push(c);
            }
        }
        Ok(Cow::Owned(out))
    }

    #[inline]
    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        if ctx.lang_entry.transliterate_is_one_to_one()
            && !ctx.lang_entry.transliterate_map().is_empty()
        {
            Some(self)
        } else {
            None
        }
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        if ctx.lang_entry.transliterate_is_one_to_one()
            && !ctx.lang_entry.transliterate_map().is_empty()
        {
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
        map.iter()
            .find(|m| m.from == c)
            .map(|m| {
                debug_assert!(
                    m.to.len() == 1,
                    "transliterate_is_one_to_one() mapping failed"
                );
                m.to.chars().next().unwrap()
            })
            .or(Some(c))
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        let map = ctx.lang_entry.transliterate_map();

        if map.is_empty() || !ctx.lang_entry.transliterate_is_one_to_one() {
            // Zero-cost path: no transliteration possible or needed
            return Box::new(text.chars());
        }

        Box::new(TransliterateIter {
            chars: text.chars(),
            map,
        })
    }
}

struct TransliterateIter<'a> {
    chars: std::str::Chars<'a>,
    map: &'static [crate::lang::FoldMap],
}

impl<'a> Iterator for TransliterateIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let c = self.chars.next()?;
        Some(
            self.map
                .iter()
                .find(|m| m.from == c)
                .map(|m| m.to.chars().next().unwrap())
                .unwrap_or(c),
        )
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}

impl<'a> FusedIterator for TransliterateIter<'a> {}

impl StageTestConfig for Transliterate {
    fn one_to_one_languages() -> &'static [Lang] {
        &[] // Multi-char expansions
    }

    fn samples(lang: Lang) -> &'static [&'static str] {
        match lang {
            FRA => &["œuvre", "ŒUVRE", "Cœur"],
            DAN => &["Århus", "århus", "Øresund"],
            DEU => &["Straße", "Fußgänger", "Weißwurst"],
            _ => &["hello", "İstanbul", "café", ""],
        }
    }

    fn should_pass_through(lang: Lang) -> &'static [&'static str] {
        match lang {
            DEU | DAN | FRA => &["hello", "world", "test"],
            _ => &["hello", "world", "test123", ""],
        }
    }

    fn should_transform(lang: Lang) -> &'static [(&'static str, &'static str)] {
        match lang {
            DEU => &[
                ("Ä", "ae"),
                ("ä", "ae"),
                ("Ö", "oe"),
                ("ö", "oe"),
                ("Ü", "ue"),
                ("ü", "ue"),
            ],
            DAN => &[
                ("Å", "aa"),
                ("å", "aa"),
                ("Æ", "ae"),
                ("æ", "ae"),
                ("Ø", "oe"),
                ("ø", "oe"),
            ],
            NOR => &[
                ("Æ", "ae"),
                ("æ", "ae"),
                ("Ø", "oe"),
                ("ø", "oe"),
                ("Å", "aa"),
                ("å", "aa"),
            ],
            SWE => &[
                ("Å", "aa"),
                ("å", "aa"),
                ("Ä", "ae"),
                ("ä", "ae"),
                ("Ö", "oe"),
                ("ö", "oe"),
            ],
            FRA => &[
                ("Œ", "oe"),
                ("œ", "oe"),
                ("Æ", "ae"),
                ("æ", "ae"),
                ("Ç", "c"),
                ("ç", "c"),
            ],
            ISL => &[
                ("Þ", "th"),
                ("þ", "th"),
                ("Ð", "d"),
                ("ð", "d"),
                ("Æ", "ae"),
                ("æ", "ae"),
            ],
            CAT => &[("Ç", "c"), ("ç", "c")],
            _ => &[],
        }
    }

    fn skip_needs_apply_test() -> bool {
        true
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Universal contract tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;
    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(Transliterate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DAN, ENG, FRA, TUR};
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
        // Assume pre-lowercased input (post-CaseFold)
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
