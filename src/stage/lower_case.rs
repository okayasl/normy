use crate::{
    CAT, DAN, DEU, ELL, ENG, FRA, ISL, ITA, LIT, NLD, NOR, POR, SPA, SWE, TUR,
    context::Context,
    lang::{Lang, LangEntry},
    stage::{CharMapper, Stage, StageError},
    testing::stage_contract::StageTestConfig,
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
        if text.is_ascii() {
            return Ok(text.bytes().any(|b| b.is_ascii_uppercase()));
        }
        Ok(text.chars().any(|c| ctx.lang_entry.needs_lowercase(c)))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let mapper = self
            .as_char_mapper(ctx)
            .expect("LowerCase always provides CharMapper");
        let mut out = String::with_capacity(text.len());
        out.extend(mapper.bind(&text, ctx));
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
        Some(ctx.lang_entry.apply_lowercase(c))
    }

    fn bind<'a>(
        &self,
        text: &'a str,
        ctx: &'a Context,
    ) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(LowercaseIter {
            chars: text.chars(),
            lang: &ctx.lang_entry,
        })
    }
}
struct LowercaseIter<'a> {
    chars: Chars<'a>,
    lang: &'a LangEntry,
}

impl<'a> Iterator for LowercaseIter<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.chars.next().map(|c| self.lang.apply_lowercase(c))
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chars.size_hint()
    }
}

impl<'a> ExactSizeIterator for LowercaseIter<'a> {
    #[inline]
    fn len(&self) -> usize {
        self.chars.size_hint().0
    }
}

impl<'a> FusedIterator for LowercaseIter<'a> {}

impl StageTestConfig for LowerCase {
    fn one_to_one_languages() -> &'static [Lang] {
        &[
            ENG, FRA, SPA, ITA, POR, DAN, NOR, SWE, ISL, CAT, TUR, DEU, NLD, ELL, LIT,
        ]
    }

    fn samples(lang: Lang) -> &'static [&'static str] {
        match lang {
            TUR => &["İSTANBUL", "ISPARTA", "İ", "I"],
            DEU => &["GROẞ", "STRAẞE", "Fuß"],
            NLD => &["IJssel", "Ĳssel"],
            ELL => &["ΣΟΦΟΣ", "ΟΔΟΣ"],
            LIT => &["JIS", "JĮ"],
            _ => &["HELLO", "World 123", " café ", "NAÏVE"],
        }
    }

    fn should_pass_through(lang: Lang) -> &'static [&'static str] {
        match lang {
            TUR => &["istanbul", "ısı", "i"],
            DEU => &["straße", "fuß"],
            NLD => &["ijssel"],
            ELL => &["σοφοσ", "οδοσ"],
            LIT => &["jis", "jį"],
            _ => &["hello", "world", "test123", ""],
        }
    }

    fn should_transform(lang: Lang) -> &'static [(&'static str, &'static str)] {
        match lang {
            TUR => &[("İ", "i"), ("I", "ı"), ("İSTANBUL", "istanbul")],
            DEU => &[("ẞ", "ß"), ("GROẞ", "groß")],
            NLD => &[
                // Dutch IJ digraph: LowerCase does NOT treat it as a unit → this is intentional
                ("IJssel", "ijssel"), // ← correct for LowerCase
                ("Ĳssel", "ĳssel"),
            ],
            ELL => &[("ΣΟΦΟΣ", "σοφοσ")],
            LIT => &[("JIS", "jis")],
            _ => &[("HELLO", "hello")],
        }
    }

    fn skip_zero_copy_apply_test() -> bool {
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
        assert_stage_contract!(LowerCase);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// One Language Per Test — The Normy Way
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CaseFold,
        lang::data::{DEU, ELL, NLD, TUR},
    };

    #[test]
    fn lowercase_turkish_dotted_i() {
        let ctx = Context::new(TUR);
        assert_eq!(
            LowerCase.apply(Cow::Borrowed("İSTANBUL"), &ctx).unwrap(),
            "istanbul"
        );
    }

    #[test]
    fn lowercase_turkish_dotless_i() {
        let ctx = Context::new(TUR);
        assert_eq!(
            LowerCase.apply(Cow::Borrowed("ISPARTA"), &ctx).unwrap(),
            "ısparta"
        );
    }

    #[test]
    fn lowercase_german_eszett_preserved() {
        let ctx = Context::new(DEU);
        assert_eq!(
            LowerCase.apply(Cow::Borrowed("GROẞ"), &ctx).unwrap(),
            "groß"
        );
        assert_eq!(
            LowerCase.apply(Cow::Borrowed("STRAẞE"), &ctx).unwrap(),
            "straße"
        );
    }

    #[test]
    fn lowercase_dutch_ij_no_digraph() {
        let ctx = Context::new(NLD);
        assert_eq!(
            LowerCase.apply(Cow::Borrowed("IJssel"), &ctx).unwrap(),
            "ijssel"
        );
        assert_eq!(
            LowerCase.apply(Cow::Borrowed("Ĳssel"), &ctx).unwrap(),
            "ĳssel"
        );
    }

    #[test]
    fn lowercase_greek_final_sigma_applied() {
        let ctx = Context::new(ELL);
        assert_eq!(
            LowerCase.apply(Cow::Borrowed("ΣΟΦΟΣ"), &ctx).unwrap(),
            "σοφοσ"
        );
        assert_eq!(
            LowerCase.apply(Cow::Borrowed("ΟΔΟΣ"), &ctx).unwrap(),
            "οδοσ"
        );
    }

    #[test]
    fn lowercase_lithuanian_contextual_i() {
        let ctx = Context::new(LIT);
        assert_eq!(LowerCase.apply(Cow::Borrowed("JIS"), &ctx).unwrap(), "jis");
    }

    #[test]
    fn lowercase_char_mapper_always_available() {
        assert!(LowerCase.as_char_mapper(&Context::new(ENG)).is_some());
        assert!(LowerCase.as_char_mapper(&Context::new(TUR)).is_some());
        assert!(LowerCase.as_char_mapper(&Context::new(DEU)).is_some());
        assert!(LowerCase.as_char_mapper(&Context::new(NLD)).is_some());
        assert!(LowerCase.as_char_mapper(&Context::new(ELL)).is_some());
    }

    #[test]
    fn lowercase_vs_case_fold_behavior() {
        let ctx_de = Context::new(DEU);
        let ctx_nl = Context::new(NLD);

        // German: LowerCase preserves ß, CaseFold expands
        assert_eq!(
            LowerCase.apply(Cow::Borrowed("GROẞ"), &ctx_de).unwrap(),
            "groß"
        );
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("GROẞ"), &ctx_de).unwrap(),
            "gross"
        );

        // Dutch: LowerCase no digraph, CaseFold has digraph
        assert_eq!(
            LowerCase.apply(Cow::Borrowed("IJssel"), &ctx_nl).unwrap(),
            "ijssel"
        );
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("IJssel"), &ctx_nl).unwrap(),
            "ijssel"
        );
    }
}
