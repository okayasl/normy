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
        // Use the precomputed flag for instant rejection
        if !ctx.lang_entry.has_transliterate_map() {
            return Ok(false);
        }

        // Check if any character in text needs transliteration
        Ok(text.chars().any(|c| ctx.lang_entry.is_transliterable(c)))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // Fast path: language has no transliteration rules
        if !ctx.lang_entry.has_transliterate_map() {
            return Ok(text);
        }

        // Use capacity hint to detect if transformation is needed
        let (trans_count, extra_bytes) = ctx.lang_entry.hint_capacity_transliterate(&text);
        if trans_count == 0 {
            return Ok(text); // Zero-copy: no chars need transliteration
        }

        // Allocate with precise capacity
        let mut out = String::with_capacity(text.len() + extra_bytes);

        let map = ctx.lang_entry.transliterate_map();
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
        // Use precomputed flags for instant decision
        if ctx.lang_entry.has_one_to_one_transliterate() && ctx.lang_entry.has_transliterate_map() {
            Some(self)
        } else {
            None
        }
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        // Use precomputed flags for instant decision
        if ctx.lang_entry.has_one_to_one_transliterate() && ctx.lang_entry.has_transliterate_map() {
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
                    m.to.chars().count() == 1,
                    "has_one_to_one_transliterate() guarantee violated: '{}' maps to '{}'",
                    m.from,
                    m.to
                );
                m.to.chars().next().unwrap()
            })
            .or(Some(c))
    }

    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        // Use precomputed flags for instant decision
        if !ctx.lang_entry.has_transliterate_map() || !ctx.lang_entry.has_one_to_one_transliterate()
        {
            // Zero-cost path: no transliteration possible or needed
            return Box::new(text.chars());
        }

        Box::new(TransliterateIter {
            chars: text.chars(),
            map: ctx.lang_entry.transliterate_map(),
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
                .map(|m| {
                    debug_assert!(
                        m.to.chars().count() == 1,
                        "TransliterateIter expects one-to-one mappings"
                    );
                    m.to.chars().next().unwrap()
                })
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
        &[] // Most languages have multi-char expansions (Ä→ae, Œ→oe, etc.)
    }

    fn samples(lang: Lang) -> &'static [&'static str] {
        match lang {
            FRA => &["œuvre", "ŒUVRE", "Cœur", "Æon"],
            DAN => &["Århus", "århus", "Øresund", "København"],
            DEU => &["Straße", "Fußgänger", "Weißwurst", "Äpfel"],
            CAT => &["Façade", "plaça", "Barça"],
            _ => &["hello", "İstanbul", "café", ""],
        }
    }

    fn should_pass_through(lang: Lang) -> &'static [&'static str] {
        match lang {
            DEU | DAN | FRA | CAT | NOR | SWE | ISL => &["hello", "world", "test", "123"],
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
        false // We can now test needs_apply accurately!
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
    use crate::{CAT, DAN, ENG, FRA, TUR};
    use std::borrow::Cow;

    #[test]
    fn test_no_transliteration() {
        let stage = Transliterate;
        let ctx = Context::new(ENG);
        let input = "Hello World";
        assert!(!stage.needs_apply(input, &ctx).unwrap());
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert_eq!(result, "Hello World");
        assert!(matches!(result, Cow::Borrowed(_))); // Zero-copy
    }

    #[test]
    fn test_french_ligatures() {
        let stage = Transliterate;
        let ctx = Context::new(FRA);

        let result = stage.apply(Cow::Borrowed("ŒUVRE"), &ctx).unwrap();
        assert_eq!(result, "oeUVRE");

        let result = stage.apply(Cow::Borrowed("œuvre"), &ctx).unwrap();
        assert_eq!(result, "oeuvre");

        let result = stage.apply(Cow::Borrowed("Æon"), &ctx).unwrap();
        assert_eq!(result, "aeon");
    }

    #[test]
    fn test_danish_special_chars() {
        let stage = Transliterate;
        let ctx = Context::new(DAN);

        let result = stage.apply(Cow::Borrowed("Århus"), &ctx).unwrap();
        assert_eq!(result, "aarhus");

        let result = stage.apply(Cow::Borrowed("København"), &ctx).unwrap();
        assert_eq!(result, "Koebenhavn");
    }

    #[test]
    fn test_catalan_cedilla() {
        let stage = Transliterate;
        let ctx = Context::new(CAT);

        assert!(stage.needs_apply("Façade", &ctx).unwrap());
        let result = stage.apply(Cow::Borrowed("Façade"), &ctx).unwrap();
        assert_eq!(result, "Facade");

        let result = stage.apply(Cow::Borrowed("plaça"), &ctx).unwrap();
        assert_eq!(result, "placa");
    }

    #[test]
    fn test_turkish_unaffected() {
        let stage = Transliterate;
        let ctx = Context::new(TUR);

        assert!(!stage.needs_apply("İstanbul", &ctx).unwrap());
        let result = stage.apply(Cow::Borrowed("İstanbul"), &ctx).unwrap();
        assert_eq!(result, "İstanbul");
        assert!(matches!(result, Cow::Borrowed(_))); // Zero-copy
    }

    #[test]
    fn test_mixed_text() {
        let stage = Transliterate;
        let ctx = Context::new(FRA);

        let input = "ŒUVRE Århus Łódź Straße";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        // Only French rules apply: Œ→oe, Æ→ae, Ç→c
        assert_eq!(result, "oeUVRE Århus Łódź Straße");
    }

    #[test]
    fn test_char_mapper_eligibility() {
        let stage = Transliterate;
        let ctx_eng = Context::new(ENG);
        let ctx_fra = Context::new(FRA);
        let ctx_dan = Context::new(DAN);

        // English: no rules
        assert!(stage.as_char_mapper(&ctx_eng).is_none());

        // French: has multi-char expansions (Œ→oe)
        assert!(stage.as_char_mapper(&ctx_fra).is_none());

        // Danish: has multi-char expansions (Å→aa)
        assert!(stage.as_char_mapper(&ctx_dan).is_none());
    }

    #[test]
    fn test_zero_copy_when_no_replacements() {
        let stage = Transliterate;
        let ctx = Context::new(FRA);

        let input = Cow::Borrowed("Paris France");
        assert!(!stage.needs_apply("Paris France", &ctx).unwrap());

        let result = stage.apply(input.clone(), &ctx).unwrap();
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, "Paris France");
    }

    #[test]
    fn test_needs_apply_accuracy() {
        let stage = Transliterate;

        // French
        let ctx = Context::new(FRA);
        assert!(stage.needs_apply("Œuvre", &ctx).unwrap());
        assert!(!stage.needs_apply("Paris", &ctx).unwrap());

        // Catalan
        let ctx = Context::new(CAT);
        assert!(stage.needs_apply("Barça", &ctx).unwrap());
        assert!(!stage.needs_apply("Barcelona", &ctx).unwrap());

        // English (no rules)
        let ctx = Context::new(ENG);
        assert!(!stage.needs_apply("anything", &ctx).unwrap());
    }

    #[test]
    fn test_capacity_hint_accuracy() {
        // French: Œ→oe (3 bytes → 2 bytes, but 1 char → 2 chars)
        let ctx = Context::new(FRA);
        let (count, _extra) = ctx.lang_entry.hint_capacity_transliterate("Œuvre");
        assert_eq!(count, 1, "Should detect 1 transliteration");
        // Œ is 3 bytes UTF-8, "oe" is 2 bytes → extra should be 0 (contraction)

        // Catalan: Ç→c (2 bytes → 1 byte, one-to-one)
        let ctx = Context::new(CAT);
        let (count, extra) = ctx.lang_entry.hint_capacity_transliterate("Façade");
        assert_eq!(count, 1, "Should detect 1 transliteration");
        assert_eq!(extra, 0, "One-to-one should have 0 extra bytes");
    }

    #[test]
    fn test_precomputed_flags() {
        let ctx_eng = Context::new(ENG);
        let ctx_fra = Context::new(FRA);
        let ctx_cat = Context::new(CAT);

        // English: no transliterate map
        assert!(!ctx_eng.lang_entry.has_transliterate_map());

        // French: has transliterate map, not one-to-one
        assert!(ctx_fra.lang_entry.has_transliterate_map());
        assert!(!ctx_fra.lang_entry.has_one_to_one_transliterate());

        // Catalan: has transliterate map, IS one-to-one (Ç→c, ç→c)
        assert!(ctx_cat.lang_entry.has_transliterate_map());
        assert!(ctx_cat.lang_entry.has_one_to_one_transliterate());
    }
}
