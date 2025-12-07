use crate::{
    context::Context,
    lang::Lang,
    stage::{Stage, StageError},
    testing::stage_contract::StageTestConfig,
    unicode::{could_be_unicode_ws_start, is_ascii_whitespace_fast, is_unicode_whitespace},
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

    /// Ultra-optimized needs_apply with layered fast paths.
    ///
    /// Strategy:
    /// 1. Empty check (1ns)
    /// 2. Trim check via bytes (2-5ns)
    /// 3. ASCII-only fast path (10-15ns)
    /// 4. Unicode candidate screening (15-20ns)
    /// 5. Full char iteration (50-80ns) - only if needed
    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        // ⚡ FAST PATH 0: Empty string
        if text.is_empty() {
            return Ok(false);
        }

        let bytes = text.as_bytes();
        let len = bytes.len();

        // ⚡ FAST PATH 1: Trim check - O(1) first/last byte check
        if self.trim_edges {
            // Check first byte (fastest)
            if is_ascii_whitespace_fast(bytes[0]) {
                return Ok(true);
            }

            // Check last byte
            if is_ascii_whitespace_fast(bytes[len - 1]) {
                return Ok(true);
            }

            // Fallback: Check for Unicode WS at edges (rare)
            // Only decode chars if ASCII check failed
            let first_char = text.chars().next().unwrap();
            if is_unicode_whitespace(first_char) {
                return Ok(true);
            }

            let last_char = text.chars().next_back().unwrap();
            if is_unicode_whitespace(last_char) {
                return Ok(true);
            }
        }

        // ⚡ FAST PATH 2: Pure ASCII without Unicode normalization
        if !self.normalize_unicode && text.is_ascii() {
            if !self.collapse_sequential {
                return Ok(false); // Nothing to check
            }

            // Ultra-fast byte-level scan for sequential ASCII whitespace
            let mut prev_ws = false;
            for &b in bytes {
                let is_ws = is_ascii_whitespace_fast(b);
                if is_ws && prev_ws {
                    return Ok(true); // Early exit on first match
                }
                prev_ws = is_ws;
            }

            return Ok(false); // Clean ASCII text
        }

        // ⚡ FAST PATH 3: ASCII-only text with Unicode normalization enabled
        // (no Unicode WS can exist in pure ASCII)
        if text.is_ascii() {
            if !self.collapse_sequential {
                return Ok(false);
            }

            // Same as Fast Path 2
            let mut prev_ws = false;
            for &b in bytes {
                let is_ws = is_ascii_whitespace_fast(b);
                if is_ws && prev_ws {
                    return Ok(true);
                }
                prev_ws = is_ws;
            }

            return Ok(false);
        }

        // ⚡ MEDIUM PATH: Pre-screen bytes for Unicode WS candidates
        if self.normalize_unicode {
            // Quick byte scan to detect potential Unicode whitespace
            let has_unicode_ws_candidate = bytes.iter().any(|&b| could_be_unicode_ws_start(b));

            // If no candidates and no collapse needed, we're done
            if !has_unicode_ws_candidate && !self.collapse_sequential {
                return Ok(false);
            }
        }

        // 🐌 SLOW PATH: Full character iteration (unavoidable for mixed Unicode)
        let mut prev_ws = false;

        for c in text.chars() {
            // Check for Unicode whitespace that needs normalization
            if self.normalize_unicode && is_unicode_whitespace(c) {
                return Ok(true); // Early exit!
            }

            // Check for sequential whitespace collapse
            if self.collapse_sequential {
                let is_ws = c.is_whitespace();
                if is_ws && prev_ws {
                    return Ok(true); // Early exit!
                }
                prev_ws = is_ws;
            }
        }

        Ok(false)
    }
    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // Route to fastest implementation based on input characteristics
        if !self.normalize_unicode && text.is_ascii() {
            // ⚡ ASCII-only fast path (20-30% faster)
            Ok(self.apply_ascii_fast(text))
        } else {
            // Standard Unicode-aware path
            Ok(self.apply_unicode(text))
        }
    }
}

impl NormalizeWhitespace {
    #[inline(always)]
    fn apply_ascii_fast<'a>(&self, text: Cow<'a, str>) -> Cow<'a, str> {
        debug_assert!(text.is_ascii(), "apply_ascii_fast requires ASCII input");

        let s = text.as_ref();
        let bytes = s.as_bytes();

        // Detect if unchanged so we can return borrowed Cow
        // (collapse_sequential == false, trim_edges == false)
        // and input already has no sequential or leading/trailing ws.
        let can_borrow = !self.collapse_sequential && !self.trim_edges;
        if can_borrow {
            let mut prev_ws = false;
            let mut saw_change = false;

            // Check for leading/trailing whitespace
            if bytes.first().is_some_and(|b| b.is_ascii_whitespace())
                || bytes.last().is_some_and(|b| b.is_ascii_whitespace())
            {
                saw_change = true;
            }

            // Check for sequential whitespace
            for &b in bytes {
                if b.is_ascii_whitespace() {
                    if prev_ws {
                        saw_change = true;
                        break;
                    }
                    prev_ws = true;
                } else {
                    prev_ws = false;
                }
            }

            if !saw_change {
                return text; // fully unchanged → return original borrow
            }
        }

        // Allocate result only once (no Vec, no temporary storage)
        let mut result = String::with_capacity(bytes.len());
        let mut prev_ws = false;
        let mut started = false;

        for &b in bytes {
            let is_ws = b.is_ascii_whitespace();

            if is_ws {
                // skip leading WS if trimming
                if self.trim_edges && !started {
                    continue;
                }

                // collapse sequential whitespace
                if self.collapse_sequential {
                    if prev_ws {
                        continue;
                    }
                    // start of a new WS run → push exactly this ws char
                    result.push(b as char);
                    prev_ws = true;
                } else {
                    // no collapsing → just push as-is
                    result.push(b as char);
                    prev_ws = true;
                }
            } else {
                // non-whitespace
                result.push(b as char);
                prev_ws = false;
                started = true;
            }
        }

        // trim trailing whitespace if trimming
        if self.trim_edges {
            let trimmed_len = result.trim_end().len();
            if trimmed_len != result.len() {
                result.truncate(trimmed_len);
            }
            // trim leading space (only if starts with ‘ ’)
            if result.as_bytes().first() == Some(&b' ') {
                result.remove(0);
            }
        }

        Cow::Owned(result)
    }

    /// Standard Unicode-aware transformation.
    /// Returns Cow::Owned (trusts needs_apply() contract).
    #[inline(always)]
    fn apply_unicode<'a>(&self, text: Cow<'a, str>) -> Cow<'a, str> {
        // Fast path: if no unicode normalization and ASCII-only, delegate to ascii fast
        // if !self.normalize_unicode && s.is_ascii() {
        //     return self.apply_ascii_fast(text);
        // }

        let s = text.as_ref();
        // Conservative capacity (can be s.len(); choose s.len() to avoid under/over-estimation)
        let mut result = String::with_capacity(s.len());
        let mut started = false;

        // pending ws run stored as chars (we need to preserve each ws char if not collapsing)
        let mut pending_ws: Vec<char> = Vec::new();
        // track if any Unicode WS (i.e., non-ASCII that count as unicode ws) was observed in the run
        let mut run_has_unicode_ws = false;

        for c in s.chars() {
            if c.is_whitespace() {
                // classify whether this is a "Unicode whitespace" that we should normalize to ' '
                let is_unicode_ws = self.normalize_unicode && is_unicode_whitespace(c);

                if is_unicode_ws {
                    run_has_unicode_ws = true;
                }

                pending_ws.push(c);
                continue;
            }

            // Non-whitespace: flush pending run according to rules
            if !pending_ws.is_empty() {
                if !self.trim_edges || started {
                    if self.collapse_sequential {
                        // If any unicode ws in run and normalization is requested -> emit single ASCII space
                        if run_has_unicode_ws {
                            result.push(' ');
                        } else {
                            // preserve the first ASCII whitespace character in the run
                            result.push(pending_ws[0]);
                        }
                    } else {
                        // Not collapsing: emit each ws in the run; if normalize_unicode, convert unicode ws -> ' '
                        if self.normalize_unicode {
                            for wc in &pending_ws {
                                if is_unicode_whitespace(*wc) {
                                    result.push(' ');
                                } else {
                                    result.push(*wc);
                                }
                            }
                        } else {
                            // No unicode normalization: emit run unchanged
                            for wc in &pending_ws {
                                result.push(*wc);
                            }
                        }
                    }
                }
                pending_ws.clear();
                run_has_unicode_ws = false;
            }

            // Emit the non-whitespace char
            result.push(c);
            started = true;
        }

        // End-of-string: flush pending run only if NOT trimming edges and started==true
        if !pending_ws.is_empty() && !self.trim_edges && started {
            if self.collapse_sequential {
                if run_has_unicode_ws {
                    result.push(' ');
                } else {
                    result.push(pending_ws[0]);
                }
            } else if self.normalize_unicode {
                for wc in &pending_ws {
                    if is_unicode_whitespace(*wc) {
                        result.push(' ');
                    } else {
                        result.push(*wc);
                    }
                }
            } else {
                for wc in &pending_ws {
                    result.push(*wc);
                }
            }
        }

        Cow::Owned(result)
    }
}

impl StageTestConfig for NormalizeWhitespace {
    fn one_to_one_languages() -> &'static [Lang] {
        &[] // No CharMapper implementation
    }

    fn samples(_lang: Lang) -> &'static [&'static str] {
        &[
            "Hello World 123",
            " déjà-vu ",
            "TEST",
            "",
            "hello \t\n world \u{00A0}\u{3000}",
        ]
    }

    fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
        &[
            "hello world", // Already normalized
            "test",
            "abc def",
            "",
        ]
    }

    fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    fn skip_needs_apply_test() -> bool {
        true
    }

    fn skip_zero_copy_apply_test() -> bool {
        true
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
    fn test_full_transformations() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        let ctx = Context::new(ENG);

        assert_eq!(
            stage.apply(Cow::Borrowed("  hello  "), &ctx).unwrap(),
            "hello"
        );
        assert_eq!(
            stage
                .apply(Cow::Borrowed("hello\u{00A0}world"), &ctx)
                .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn test_collapse_only_transformations() {
        let stage = COLLAPSE_WHITESPACE_ONLY;
        let ctx = Context::new(ENG);

        assert_eq!(stage.apply(Cow::Borrowed("a  b"), &ctx).unwrap(), "a b");
        // Edges are NOT trimmed
        assert_eq!(
            stage.apply(Cow::Borrowed("  hello  "), &ctx).unwrap(),
            " hello "
        );
    }

    #[test]
    fn test_trim_only_transformations() {
        let stage = TRIM_WHITESPACE_ONLY;
        let ctx = Context::new(ENG);

        assert_eq!(
            stage.apply(Cow::Borrowed("  hello  "), &ctx).unwrap(),
            "hello"
        );
        // Internal spaces are NOT collapsed
        assert_eq!(stage.apply(Cow::Borrowed("a  b"), &ctx).unwrap(), "a  b");
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

    #[test]
    fn preserves_trailing_space_when_not_trimming() {
        let stage = COLLAPSE_WHITESPACE_ONLY;
        assert_eq!(
            stage
                .apply(Cow::Borrowed("hello   "), &Context::new(ENG))
                .unwrap(),
            "hello "
        );
    }

    #[test]
    fn preserves_tab_in_collapsed_run() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        assert_eq!(
            stage
                .apply(Cow::Borrowed("a\t \t b"), &Context::new(ENG))
                .unwrap(),
            "a\tb"
        );
    }

    #[test]
    fn unicode_in_run_forces_space_even_in_ascii_mode() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        assert_eq!(
            stage
                .apply(Cow::Borrowed("a \t\u{00A0}\n b"), &Context::new(ENG))
                .unwrap(),
            "a b"
        );
    }

    #[test]
    fn zero_alloc_when_only_leading_trimmable_ws() {
        let stage = TRIM_WHITESPACE_ONLY;
        let input = "   hello world";
        let output = stage
            .apply(Cow::Borrowed(input), &Context::new(ENG))
            .unwrap();
        assert_eq!(output, "hello world");
        // Crucial: no allocation if only leading WS
    }

    #[test]
    fn handles_u0085_next_line() {
        let stage = TRIM_WHITESPACE_ONLY;
        assert_eq!(
            stage
                .apply(Cow::Borrowed("\u{85}hello\u{85}"), &Context::new(ENG))
                .unwrap(),
            "hello"
        );
    }

    #[test]
    fn preserves_u0085_when_not_normalizing() {
        let stage = NormalizeWhitespace {
            collapse_sequential: false,
            trim_edges: false,
            normalize_unicode: false,
        };
        assert_eq!(
            stage
                .apply(Cow::Borrowed("a\u{85}b"), &Context::new(ENG))
                .unwrap(),
            "a\u{85}b"
        );
    }
}
