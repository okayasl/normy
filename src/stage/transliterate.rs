use crate::{
    CAT, DAN, DEU, FRA, ISL, NOR, SWE,
    context::Context,
    lang::{Lang, LangEntry},
    stage::{FusedIterator, Stage, StageError, StaticFusableStage},
    testing::stage_contract::StageTestConfig,
};
use std::borrow::Cow;

/// Performs locale-aware orthographic transliteration (lossy, opt-in).
///
/// This stage applies language-specific multi-character expansions using the
/// target's `transliterate` map:
///
/// - French: Œ→oe, Æ→ae, Ç→c
/// - German: Ä→ae, Ö→oe, Ü→ue
/// - Nordic (Danish/Norwegian/Swedish): Å→aa, Æ→ae, Ø→oe
/// - Icelandic: Þ→th, Ð→d, Æ→ae
///
/// Rules are applied strictly per language — foreign characters are preserved.
/// Case is preserved (maps exist for both upper and lower forms).
///
/// Zero-copy when no transliteration rules apply or no matches found.
///
/// This stage is eligible for static fusion in all supported languages.
#[derive(Debug, Default, Clone, Copy)]
pub struct Transliterate;

impl Stage for Transliterate {
    fn name(&self) -> &'static str {
        "transliterate"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        let entry = ctx.lang_entry;

        if !entry.has_transliterate_map() || text.is_ascii() {
            return Ok(false);
        }

        Ok(text.chars().any(|c| entry.is_transliterable(c)))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let entry = ctx.lang_entry;
        // Pre-calculate capacity
        let capacity = if entry.has_one_to_one_transliterate() {
            // Guaranteed to be close to or exactly text.len()
            text.len()
        } else {
            // Growth required (e.g., German Ä -> ae)
            text.len() + (text.len() >> 3)
        };

        let mut out = String::with_capacity(capacity);
        for c in text.chars() {
            if let Some(replacement) = entry.find_transliterate_map(c) {
                out.push_str(replacement);
            } else {
                out.push(c);
            }
        }
        Ok(Cow::Owned(out))
    }
}

impl StaticFusableStage for Transliterate {
    type Adapter<'a, I>
        = TransliterateAdapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    #[inline(always)]
    fn supports_static_fusion(&self) -> bool {
        true
    }

    #[inline(always)]
    fn static_fused_adapter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::Adapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        TransliterateAdapter {
            input,
            lang: &ctx.lang_entry,
            pending: None,
        }
    }
}

pub struct TransliterateAdapter<'a, I> {
    input: I,
    lang: &'a LangEntry,
    /// Buffer for multi-character expansions (e.g. "oe", "ss")
    pending: Option<&'a str>,
}

impl<'a, I: Iterator<Item = char>> Iterator for TransliterateAdapter<'a, I> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        // 1. Drain pending expansion buffer first
        if let Some(pending_str) = self.pending {
            let mut chars = pending_str.chars();
            let first = chars.next().expect("Pending string should not be empty");
            let rest = chars.as_str();

            self.pending = if rest.is_empty() { None } else { Some(rest) };
            return Some(first);
        }

        // 2. Pull next char from source
        let c = self.input.next()?;

        // 3. Look up in language-specific transliteration table
        if let Some(replacement) = self.lang.find_transliterate_map(c) {
            let mut chars = replacement.chars();
            let first = chars
                .next()
                .expect("Replacement map entries must not be empty");
            let rest = chars.as_str();

            if !rest.is_empty() {
                self.pending = Some(rest);
            }
            return Some(first);
        }

        Some(c)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lower, upper) = self.input.size_hint();
        // Calculate pending length in characters (usually 1 or 0 for your data)
        let pending_len = self.pending.map_or(0, |s| s.chars().count());

        // Lower bound: at least the remaining input + what's in pending
        let lower_bound = lower.saturating_add(pending_len);

        // Upper bound: Your mappings (Ä->ae, Þ->th) are max 1:2 expansions.
        // We use saturating_mul(2) to safely represent this maximum.
        let upper_bound = upper.map(|u| u.saturating_mul(2).saturating_add(pending_len));

        (lower_bound, upper_bound)
    }
}

impl<'a, I: FusedIterator<Item = char>> FusedIterator for TransliterateAdapter<'a, I> {}

impl StageTestConfig for Transliterate {
    fn one_to_one_languages() -> &'static [Lang] {
        &[] // Multi-char expansions common
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
    use crate::{DAN, FRA};

    #[test]
    fn test_language_isolation() {
        // Critical: Only applies rules for the context language
        let stage = Transliterate;
        let ctx = Context::new(FRA);
        let input = "ŒUVRE Århus Straße"; // French + Danish + German
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        // Only French Œ→oe applies, Danish Å and German ß unchanged
        assert_eq!(result, "oeUVRE Århus Straße");

        // Try with Danish context
        let ctx = Context::new(DAN);
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert_eq!(result, "ŒUVRE aarhus Straße"); // Only Å→aa applies
    }
}
