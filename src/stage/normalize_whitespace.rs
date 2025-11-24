use crate::{
    context::Context,
    stage::{Stage, StageError},
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
/// ## Preset constructors
///
/// - `NormalizeWhitespace::default()`: All features enabled (collapse, trim, normalize).
/// - `NormalizeWhitespace::collapse_only()`: Collapse consecutive spaces only, preserves edges.
/// - `NormalizeWhitespace::trim_only()`: Trim edges only, preserves internal spacing.
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

impl Default for NormalizeWhitespace {
    fn default() -> Self {
        Self {
            collapse_sequential: true,
            trim_edges: true,
            normalize_unicode: true,
        }
    }
}

impl NormalizeWhitespace {
    /// Create a new instance with default behavior:
    /// - Collapse sequential whitespace
    /// - Trim leading/trailing whitespace
    /// - Normalize Unicode whitespace → ASCII space
    ///
    /// This is the recommended configuration for search, indexing, and display.
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Collapse only — useful when you want to preserve user formatting
    /// (e.g. code blocks, preformatted text)
    pub fn collapse_only() -> Self {
        Self {
            collapse_sequential: true,
            trim_edges: false,
            normalize_unicode: false,
        }
    }

    /// Trim only — useful when you want to preserve internal spacing
    /// (e.g. log parsing, diff tools)
    pub fn trim_only() -> Self {
        Self {
            collapse_sequential: false,
            trim_edges: true,
            normalize_unicode: false,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FoldCase, lang::data::ENG};

    #[test]
    fn test_collapse_sequential() {
        let stage = NormalizeWhitespace {
            collapse_sequential: true,
            trim_edges: false,
            normalize_unicode: false,
        };
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
        let stage = NormalizeWhitespace {
            collapse_sequential: false,
            trim_edges: true,
            normalize_unicode: false,
        };
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
        let stage = NormalizeWhitespace {
            collapse_sequential: false,
            trim_edges: false,
            normalize_unicode: true,
        };
        let c = Context::new(ENG);

        // NBSP → space
        assert_eq!(
            stage
                .apply(Cow::Borrowed("hello\u{00A0}world"), &c)
                .unwrap(),
            "hello world"
        );

        // Ideographic space → space
        assert_eq!(
            stage
                .apply(Cow::Borrowed("hello\u{3000}world"), &c)
                .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn test_default_all_features() {
        let stage = NormalizeWhitespace::default();
        let c = Context::new(ENG);

        let input = "  hello\u{00A0}\u{00A0}world  ";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();

        assert_eq!(result, "hello world");
    }

    // ------------------------------------------------------------------------
    // Preset Constructors
    // ------------------------------------------------------------------------

    #[test]
    fn test_collapse_only() {
        let stage = NormalizeWhitespace::collapse_only();
        let c = Context::new(ENG);

        let input = "  hello  world  ";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();

        // Collapses but doesn't trim
        assert_eq!(result, " hello world ");
    }

    #[test]
    fn test_trim_only() {
        let stage = NormalizeWhitespace::trim_only();
        let c = Context::new(ENG);

        let input = "  hello  world  ";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();

        // Trims but doesn't collapse
        assert_eq!(result, "hello  world");
    }

    // ------------------------------------------------------------------------
    // Edge Cases
    // ------------------------------------------------------------------------

    #[test]
    fn test_empty_string() {
        let stage = NormalizeWhitespace::default();
        let c = Context::new(ENG);

        assert!(!stage.needs_apply("", &c).unwrap());
        assert_eq!(stage.apply(Cow::Borrowed(""), &c).unwrap(), "");
    }

    #[test]
    fn test_only_whitespace() {
        let stage = NormalizeWhitespace::default();
        let c = Context::new(ENG);

        assert_eq!(stage.apply(Cow::Borrowed("   "), &c).unwrap(), "");
        assert_eq!(stage.apply(Cow::Borrowed("\t\n\r"), &c).unwrap(), "");
    }

    #[test]
    fn test_no_whitespace() {
        let stage = NormalizeWhitespace::default();
        let c = Context::new(ENG);

        let text = "helloworld";
        assert!(!stage.needs_apply(text, &c).unwrap());

        let result = stage.apply(Cow::Borrowed(text), &c).unwrap();
        assert!(matches!(result, Cow::Borrowed(_))); // Zero-copy
        assert_eq!(result, text);
    }

    #[test]
    fn test_single_word() {
        let stage = NormalizeWhitespace::default();
        let c = Context::new(ENG);

        assert_eq!(stage.apply(Cow::Borrowed("hello"), &c).unwrap(), "hello");
    }

    #[test]
    fn test_newlines_and_tabs() {
        let stage = NormalizeWhitespace {
            collapse_sequential: true,
            trim_edges: true,
            // FIX: Set to true to convert all whitespace characters (including \n, \t, \r)
            // to a standard ASCII space before collapsing/trimming.
            normalize_unicode: true,
        };
        let c = Context::new(ENG);

        let input = "\thello\n\nworld\r\n";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();

        // Tabs and newlines are now converted to spaces and collapsed
        assert_eq!(result, "hello world");
    }

    // ------------------------------------------------------------------------
    // Real-World Examples
    // ------------------------------------------------------------------------

    #[test]
    fn test_search_query_normalization() {
        let stage = NormalizeWhitespace::default();
        let c = Context::new(ENG);

        let queries = vec![
            ("  machine   learning  ", "machine learning"),
            ("rust\u{00A0}programming", "rust programming"),
            ("\tnlp\n", "nlp"),
        ];

        for (input, expected) in queries {
            assert_eq!(stage.apply(Cow::Borrowed(input), &c).unwrap(), expected);
        }
    }

    #[test]
    fn test_user_input_cleanup() {
        let stage = NormalizeWhitespace::default();
        let c = Context::new(ENG);

        // Typical messy user input
        let input = "  John   Doe  ";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();
        assert_eq!(result, "John Doe");
    }

    #[test]
    fn test_preserve_internal_whitespace_when_only_trimming() {
        let stage = NormalizeWhitespace::trim_only();
        let c = Context::new(ENG);

        let input = "  hello     world  ";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();

        // Trim edges but preserve internal multiple spaces
        assert_eq!(result, "hello     world");
    }

    // ------------------------------------------------------------------------
    // Unicode Whitespace Variants
    // ------------------------------------------------------------------------

    #[test]
    fn test_various_unicode_spaces() {
        let stage = NormalizeWhitespace {
            collapse_sequential: true,
            trim_edges: true,
            normalize_unicode: true,
        };
        let c = Context::new(ENG);

        let spaces = vec![
            '\u{00A0}', // NO-BREAK SPACE
            '\u{2000}', // EN QUAD
            '\u{2003}', // EM SPACE
            '\u{3000}', // IDEOGRAPHIC SPACE
        ];

        for space in spaces {
            let input = format!("hello{}world", space);
            let result = stage.apply(Cow::Borrowed(&input), &c).unwrap();
            assert_eq!(result, "hello world", "Failed for U+{:04X}", space as u32);
        }
    }

    #[test]
    fn test_mixed_whitespace_types() {
        let stage = NormalizeWhitespace::default();
        let c = Context::new(ENG);

        let input = "hello\t\u{00A0} \u{3000}world";
        let result = stage.apply(Cow::Borrowed(input), &c).unwrap();

        // All different whitespace types collapsed to single space
        assert_eq!(result, "hello world");
    }

    // ------------------------------------------------------------------------
    // Performance Tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_needs_apply_optimization() {
        let stage = NormalizeWhitespace::default();
        let c = Context::new(ENG);

        // Text that doesn't need normalization
        let clean_texts = vec!["hello", "hello world", "a b c d e f"];

        for text in clean_texts {
            assert!(
                !stage.needs_apply(text, &c).unwrap(),
                "Should not need normalization: '{}'",
                text
            );
        }

        // Text that needs normalization
        let dirty_texts = vec![" hello", "hello ", "hello  world", "hello\u{00A0}world"];

        for text in dirty_texts {
            assert!(
                stage.needs_apply(text, &c).unwrap(),
                "Should need normalization: '{}'",
                text
            );
        }
    }

    // ------------------------------------------------------------------------
    // Idempotency
    // ------------------------------------------------------------------------

    #[test]
    fn test_idempotency() {
        let stage = NormalizeWhitespace::default();
        let c = Context::new(ENG);

        let input = "  hello   world  ";
        let once = stage.apply(Cow::Borrowed(input), &c).unwrap();
        let twice = stage.apply(Cow::Borrowed(&once), &c).unwrap();

        assert_eq!(once, "hello world");
        assert_eq!(once, twice);
    }

    // ------------------------------------------------------------------------
    // Integration with Other Stages
    // ------------------------------------------------------------------------

    #[test]
    fn test_after_case_fold() {
        let case_fold = FoldCase;
        let whitespace = NormalizeWhitespace::default();
        let c = Context::new(ENG);

        let input = "  HELLO   WORLD  ";

        // Case fold first
        let lower = case_fold.apply(Cow::Borrowed(input), &c).unwrap();
        // Then normalize whitespace
        let result = whitespace.apply(Cow::Borrowed(&lower), &c).unwrap();

        assert_eq!(result, "hello world");
    }
}
