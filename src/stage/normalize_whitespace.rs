use crate::{
    context::Context,
    lang::Lang,
    stage::{Stage, StageError},
    testing::stage_contract::StageTestConfig,
    unicode::is_unicode_whitespace,
};
use std::borrow::Cow;

/// Normalize and standardize whitespace in text pipelines.
///
/// This stage provides flexible whitespace normalization for text processing,
/// including search, tokenization, display cleanup, and API input sanitization.
///
/// ## Features
///
/// 1. **Collapse sequential whitespace**: Multiple consecutive whitespace
///    characters (spaces, tabs, newlines, Unicode spaces) are collapsed into
///    a single ASCII space when `collapse_sequential = true`.
///
/// 2. **Trim edges**: Leading and trailing whitespace is removed when
///    `trim_edges = true`.
///
/// 3. **Normalize Unicode whitespace**: Converts all Unicode whitespace
///    characters (e.g., NBSP, EM SPACE, IDEOGRAPHIC SPACE) into standard
///    ASCII space `' '` when `normalize_unicode = true`.
///
/// 4. **Zero-copy when possible**: If no normalization is needed, returns
///    `Cow::Borrowed` to avoid unnecessary allocations.
///
/// ## Usage Scenarios
///
/// - **Search / indexing**: Ensure consistent whitespace for queries and storage.
/// - **Tokenization**: Simplify word boundary detection by normalizing spaces.
/// - **Display cleanup**: Standardize formatting for rendering or logging.
/// - **API input sanitization**: Convert messy user input into clean, predictable text.
///
/// ## Unicode Support
///
/// Supports a wide set of Unicode whitespace characters, including but not limited to:
/// - No-break space (U+00A0)
/// - Ogham space mark (U+1680)
/// - En / Em / Figure / Ideographic spaces (U+2000–U+3000)
/// - Narrow / medium / thin spaces (U+202F, U+205F, U+2009)
///
/// These characters are normalized to ASCII space `' '` when `normalize_unicode` is enabled.
#[derive(Debug, Clone, Copy)]
pub struct NormalizeWhitespace {
    /// Collapse multiple sequential whitespace chars into one
    pub collapse_sequential: bool,

    /// Remove leading and trailing whitespace
    pub trim_edges: bool,

    /// Convert Unicode whitespace (NBSP, etc.) to ASCII space
    pub normalize_unicode: bool,
}

// ------------------------------------------------------------------------
// Helper Constants
// ------------------------------------------------------------------------

/// Collapse, trim, and normalize Unicode whitespace (recommended for most pipelines)
pub const NORMALIZE_WHITESPACE_FULL: NormalizeWhitespace = NormalizeWhitespace {
    collapse_sequential: true,
    trim_edges: true,
    normalize_unicode: true,
};

/// Collapse sequential whitespace only, preserve edges
pub const COLLAPSE_WHITESPACE_ONLY: NormalizeWhitespace = NormalizeWhitespace {
    collapse_sequential: true,
    trim_edges: false,
    normalize_unicode: false,
};

/// Trim edges only, preserve internal spacing
pub const TRIM_WHITESPACE_ONLY: NormalizeWhitespace = NormalizeWhitespace {
    collapse_sequential: false,
    trim_edges: true,
    normalize_unicode: false,
};

impl Default for NormalizeWhitespace {
    fn default() -> Self {
        NORMALIZE_WHITESPACE_FULL
    }
}

impl Stage for NormalizeWhitespace {
    fn name(&self) -> &'static str {
        "normalize_whitespace"
    }

    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        if text.is_empty() {
            return Ok(false);
        }

        if self.trim_edges
            && (text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace))
        {
            return Ok(true);
        }

        if self.normalize_unicode && text.chars().any(is_unicode_whitespace) {
            return Ok(true);
        }

        if self.collapse_sequential && has_sequential_whitespace(text) {
            return Ok(true);
        }

        Ok(false)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, _ctx)? {
            return Ok(text);
        }

        let mut result = String::with_capacity(text.len());
        let mut prev_was_whitespace = false;
        let mut started = false; // Track if we've seen non-whitespace yet

        for c in text.chars() {
            let is_ws = c.is_whitespace();

            let normalized_char = if is_ws && self.normalize_unicode {
                ' '
            } else {
                c
            };

            if is_ws {
                if self.trim_edges && !started {
                    continue;
                }

                if self.collapse_sequential && prev_was_whitespace {
                    continue;
                }

                result.push(normalized_char);
                prev_was_whitespace = true;
            } else {
                result.push(normalized_char);
                prev_was_whitespace = false;
                started = true;
            }
        }

        if self.trim_edges {
            let trimmed = result.trim_end();
            if trimmed.len() != result.len() {
                result.truncate(trimmed.len());
            }
        }

        Ok(Cow::Owned(result))
    }
}

#[inline]
fn has_sequential_whitespace(text: &str) -> bool {
    let mut prev_was_ws = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if prev_was_ws {
                return true;
            }
            prev_was_ws = true;
        } else {
            prev_was_ws = false;
        }
    }
    false
}

impl StageTestConfig for NormalizeWhitespace {
    fn one_to_one_languages() -> &'static [Lang] {
        &[] // No CharMapper implementation
    }

    fn skip_needs_apply_test() -> bool {
        true // needs_apply() detects whitespace patterns, not case changes
    }

    fn samples(_lang: Lang) -> &'static [&'static str] {
        &[
            "Hello World 123",
            " déjà-vu ",
            "TEST",
            "",
            "hello \t\n world \u{00A0}\u{3000}", // Heavy whitespace sample
        ]
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::{ENG, assert_stage_contract};

    #[test]
    fn universal_contract_tests() {
        assert_stage_contract!(NORMALIZE_WHITESPACE_FULL);
        assert_stage_contract!(COLLAPSE_WHITESPACE_ONLY);
        assert_stage_contract!(TRIM_WHITESPACE_ONLY);
    }

    // —————————————————————————————————————————————————
    // Surgical, doctrine-compliant specific tests
    // —————————————————————————————————————————————————

    #[test]
    fn ascii_tab_is_not_normalized_to_space() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        let input = "hello\tworld";
        let output = stage
            .apply(Cow::Borrowed(input), &Context::new(ENG))
            .unwrap();
        assert_eq!(output, "hello\tworld"); // ← This is correct!
    }

    #[test]
    fn unicode_whitespace_is_normalized_to_space() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        let input = "hello\u{00A0}\u{3000}world";
        let output = stage
            .apply(Cow::Borrowed(input), &Context::new(ENG))
            .unwrap();
        assert_eq!(output, "hello world");
    }

    #[test]
    fn sequential_ascii_whitespace_is_collapsed_when_enabled() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        let input = "a   \t \n  b";
        let output = stage
            .apply(Cow::Borrowed(input), &Context::new(ENG))
            .unwrap();
        assert_eq!(output, "a b");
    }

    #[test]
    fn zero_copy_when_only_ascii_spaces_and_no_collapse_needed() {
        let stage = COLLAPSE_WHITESPACE_ONLY;
        let input = "hello world";
        let output = stage
            .apply(Cow::Borrowed(input), &Context::new(ENG))
            .unwrap();
        assert!(matches!(output, Cow::Borrowed(_)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::data::ENG;
    use std::borrow::Cow;

    #[test]
    fn test_collapse_sequential() {
        let stage = COLLAPSE_WHITESPACE_ONLY;
        let c = Context::new(ENG);

        assert_eq!(
            stage.apply(Cow::Borrowed("hello  world"), &c).unwrap(),
            "hello world"
        );
        assert_eq!(
            stage.apply(Cow::Borrowed("a   b    c"), &c).unwrap(),
            "a b c"
        );
    }

    #[test]
    fn test_trim_edges() {
        let stage = TRIM_WHITESPACE_ONLY;
        let c = Context::new(ENG);

        assert_eq!(
            stage.apply(Cow::Borrowed("  hello  "), &c).unwrap(),
            "hello"
        );
        assert_eq!(
            stage.apply(Cow::Borrowed("\thello\n"), &c).unwrap(),
            "hello"
        );
    }

    #[test]
    fn test_normalize_unicode() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        let c = Context::new(ENG);

        assert_eq!(
            stage
                .apply(Cow::Borrowed("hello\u{00A0}world"), &c)
                .unwrap(),
            "hello world"
        );
        assert_eq!(
            stage
                .apply(Cow::Borrowed("hello\u{3000}world"), &c)
                .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn test_default_all_features() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        let c = Context::new(ENG);

        let input = "  hello\u{00A0}\u{00A0}world  ";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();

        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_edge_cases() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        let c = Context::new(ENG);

        assert_eq!(stage.apply(Cow::Borrowed("   "), &c).unwrap(), "");
        assert_eq!(stage.apply(Cow::Borrowed("\t\n\r"), &c).unwrap(), "");
        let text = "helloworld";
        let result = stage.apply(Cow::Borrowed(text), &c).unwrap();
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, text);
    }

    #[test]
    fn test_mixed_whitespace_types() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        let c = Context::new(ENG);

        let input = "hello\t\u{00A0} \u{3000}world";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();

        assert_eq!(result, "hello world");
    }
}
