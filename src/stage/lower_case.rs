use crate::{
    CAT, DAN, DEU, ELL, ENG, FRA, ISL, ITA, LIT, NLD, NOR, POR, SPA, SWE, TUR,
    context::Context,
    lang::{Lang, LangEntry},
    stage::{Stage, StageError, StaticFusableStage},
    testing::stage_contract::StageTestConfig,
};
use std::borrow::Cow;
use std::iter::FusedIterator;

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
        let cap = (text.len() as f64 * 1.1) as usize;
        let mut out = String::with_capacity(cap);

        // Manual loop is often easier for the compiler to vectorize
        // compared to .collect() or .extend() for simple 1:1 maps.
        for c in text.chars() {
            out.push(ctx.lang_entry.apply_lowercase(c));
        }

        Ok(Cow::Owned(out))
    }
}

impl StaticFusableStage for LowerCase {
    type Adapter<'a, I>
        = LowercaseAdapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    fn supports_static_fusion(&self) -> bool {
        true
    }

    #[inline(always)]
    fn static_fused_adapter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::Adapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        LowercaseAdapter {
            input,
            lang: &ctx.lang_entry,
        }
    }
}

pub struct LowercaseAdapter<'a, I> {
    input: I,
    lang: &'a LangEntry,
}

impl<'a, I: Iterator<Item = char>> Iterator for LowercaseAdapter<'a, I> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        // Direct 1:1 mapping as per Normy's LowerCase philosophy
        self.input.next().map(|c| self.lang.apply_lowercase(c))
    }

    #[inline(always)]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.input.size_hint()
    }
}

impl<'a, I: FusedIterator<Item = char>> FusedIterator for LowercaseAdapter<'a, I> {}

impl<'a, I: ExactSizeIterator + Iterator<Item = char>> ExactSizeIterator
    for LowercaseAdapter<'a, I>
{
    #[inline(always)]
    fn len(&self) -> usize {
        self.input.len()
    }
}

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
            TUR => &["istanbul", "ısparta", "i", "ı"],
            DEU => &["groß", "straße", "fuß"],
            NLD => &["ijssel", "ĳssel"],
            ELL => &["σοφοσ", "οδοσ"],
            LIT => &["jis", "jį"],
            _ => &["hello", "world", "test123", ""],
        }
    }

    fn should_transform(lang: Lang) -> &'static [(&'static str, &'static str)] {
        match lang {
            TUR => &[
                ("İ", "i"),
                ("I", "ı"),
                ("İSTANBUL", "istanbul"),
                ("ISPARTA", "ısparta"),
            ],
            DEU => &[("ẞ", "ß"), ("GROẞ", "groß"), ("STRAẞE", "straße")],
            NLD => &[("IJssel", "ijssel"), ("Ĳssel", "ĳssel")],
            ELL => &[("ΣΟΦΟΣ", "σοφοσ"), ("ΟΔΟΣ", "οδοσ")],
            LIT => &[("JIS", "jis")],
            _ => &[("HELLO", "hello")],
        }
    }
}

// Universal contract compliance
#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;

    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(LowerCase);
    }
}

// Optional: Keep only to illustrate philosophical difference
#[cfg(test)]
mod tests {
    use super::*;
    use crate::CaseFold;

    #[test]
    fn lowercase_vs_case_fold_behavior() {
        let ctx_de = Context::new(crate::lang::data::DEU);
        let ctx_nl = Context::new(crate::lang::data::NLD);

        // German: LowerCase preserves ß, CaseFold expands
        assert_eq!(
            LowerCase.apply(Cow::Borrowed("GROẞ"), &ctx_de).unwrap(),
            "groß"
        );
        assert_eq!(
            CaseFold.apply(Cow::Borrowed("GROẞ"), &ctx_de).unwrap(),
            "gross"
        );

        // Dutch: LowerCase treats IJ per-character, CaseFold treats as digraph
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
