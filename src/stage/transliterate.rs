use crate::{
    CAT, DAN, DEU, FRA, ISL, NOR, SWE,
    context::Context,
    lang::{Lang, LangEntry},
    stage::{CharMapper, FusedIterator, Stage, StageError, StageIter},
    testing::stage_contract::StageTestConfig,
};
use std::borrow::Cow;
use std::sync::Arc;

/// Locale-aware orthographic transliteration (lossy, opt-in).
///
/// `Transliterate` performs **lossy** decomposition of language-specific letterforms
/// into their conventional multi-character representations:
///
/// - French: `Œ` → "oe", `Æ` → "ae"
/// - German: `Ä` → "ae", `ß` → "ss"
/// - Danish/Norwegian: `Å` → "aa"
/// - Polish: `Ł` → "l"
///
/// # Key Principles
///
/// - **Case-Preserving**: `Straße` → `Strasse`
/// - **Opt-In Only**: Never enabled by default
/// - **Locale-Strict**: Only rules defined for current language
/// - **Zero-Copy When Idle**: Skipped entirely if no rules or no matches
/// - **CharMapper Path**: Fully fused when only 1→1 mappings exist
///
/// When only one-to-one mappings are present (most languages), this stage runs
/// **zero-allocation** via `CharMapper`. For rare 1→N cases (e.g. `Œ` → "oe"),
/// it falls back to allocation — this is accepted and explicit.
#[derive(Debug, Default, Clone, Copy)]
pub struct Transliterate;

impl Stage for Transliterate {
    fn name(&self) -> &'static str {
        "transliterate"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        let entry = ctx.lang_entry;

        if !entry.has_transliterate_map() {
            return Ok(false);
        }

        Ok(text.chars().any(|c| entry.is_transliterable(c)))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        Ok(Cow::Owned(TransliterateIter::new(&text, ctx).collect()))
    }

    #[inline]
    fn as_char_mapper(&self, ctx: &Context) -> Option<&dyn CharMapper> {
        if ctx.lang_entry.has_one_to_one_transliterate() {
            Some(self)
        } else {
            None
        }
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        if ctx.lang_entry.has_one_to_one_transliterate() {
            Some(self)
        } else {
            None
        }
    }

    fn try_dynamic_iter<'a>(
        &self,
        text: &'a str,
        ctx: &'a Context,
    ) -> Option<Box<dyn FusedIterator<Item = char> + 'a>> {
        Some(Box::new(TransliterateIter::new(text, ctx)))
    }
}

impl CharMapper for Transliterate {
    #[inline(always)]
    fn map(&self, c: char, ctx: &Context) -> Option<char> {
        ctx.lang_entry
            .find_transliterate_map(c)
            .and_then(|to| to.chars().next())
            .or(Some(c))
    }

    #[inline(always)]
    fn bind<'a>(
        &self,
        text: &'a str,
        ctx: &'a Context,
    ) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(TransliterateIter::new(text, ctx))
    }
}

impl StageIter for Transliterate {
    type Iter<'a> = TransliterateIter<'a>;

    #[inline(always)]
    fn try_iter<'a>(&self, text: &'a str, ctx: &'a Context) -> Option<Self::Iter<'a>> {
        Some(TransliterateIter::new(text, ctx))
    }
}

/// Unified iterator — handles both 1→1 and 1→N transliteration safely and efficiently
pub struct TransliterateIter<'a> {
    chars: std::str::Chars<'a>,
    lang: &'a LangEntry,
    /// Buffer for multi-character expansions (e.g. "oe", "aa", "ss")
    pending: Option<&'a str>,
}

impl<'a> TransliterateIter<'a> {
    #[inline(always)]
    pub fn new(text: &'a str, ctx: &'a Context) -> Self {
        Self {
            chars: text.chars(),
            lang: &ctx.lang_entry,
            pending: None,
        }
    }
}

impl Iterator for TransliterateIter<'_> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        // First, emit any pending characters from a previous 1→N mapping
        if let Some(pending_str) = self.pending {
            let mut chars = pending_str.chars();
            let first = chars.next().unwrap(); // safe: pending_str is never empty when set
            if chars.as_str().is_empty() {
                self.pending = None;
            } else {
                self.pending = Some(chars.as_str());
            }
            return Some(first);
        }

        let c = self.chars.next()?;

        if let Some(to) = self.lang.find_transliterate_map(c) {
            if to.len() == 1 {
                // Fast path: 1→1 — emit directly
                return Some(to.chars().next().unwrap());
            } else {
                // 1→N path: emit first char now, store rest in pending
                let mut chars = to.chars();
                let first = chars.next().unwrap();
                let rest = chars.as_str();
                if !rest.is_empty() {
                    self.pending = Some(rest);
                }
                return Some(first);
            }
        }

        Some(c)
    }
}

impl FusedIterator for TransliterateIter<'_> {}

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
    fn test_zero_copy_when_no_replacements() {
        let stage = Transliterate;
        let ctx = Context::new(FRA);

        let input = Cow::Borrowed("Paris France");
        assert!(!stage.needs_apply("Paris France", &ctx).unwrap());

        let result = stage.apply(input.clone(), &ctx).unwrap();
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
