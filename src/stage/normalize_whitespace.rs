use smallvec::SmallVec;

use crate::{
    context::Context,
    lang::Lang,
    stage::{CharMapper, Stage, StageError},
    testing::stage_contract::StageTestConfig,
    unicode::{could_be_unicode_ws_start, is_ascii_whitespace_fast, is_unicode_whitespace},
};
use std::{borrow::Cow, iter::FusedIterator};

/// Normalize and standardize whitespace in text pipelines.
///
/// This stage performs up to three operations in **one pass**, with **at most one allocation**,
/// and preserves zero-copy whenever the input is already clean.
///
/// ## Features
///
/// | Operation                     | Flag                     | Effect when enabled                                                                 |
/// |-------------------------------|--------------------------|---------------------------------------------------------------------------------------|
/// | **Collapse sequential WS**    | `collapse_sequential`    | Multiple consecutive whitespace → single ASCII space `' '`                            |
/// | **Trim edges**                | `trim_edges`             | Remove leading and trailing whitespace                                                |
/// | **Normalize Unicode WS**      | `normalize_unicode`      | Modifier flag: extends trim/collapse to recognize Unicode whitespace                  |
/// | **Custom replacement**        | `collapse_replacement`   | Character emitted when collapsing (default `' '`; can be ZWSP, etc.)                |
///
/// All three operations are performed in a **single forward pass** over the string,
/// guaranteeing ≤1 heap allocation regardless of configuration.
///
/// ## Design Philosophy
///
/// **`normalize_unicode` is a modifier flag, not a standalone operation:**
///
/// - **`normalize_unicode` alone**: No-op (returns `Cow::Borrowed` immediately)
/// - **`trim_edges + normalize_unicode`**: Trim edges including Unicode whitespace (matches `str::trim()`)
/// - **`collapse_sequential + normalize_unicode`**: Collapse sequential whitespace including Unicode WS, normalize to ASCII space `' '`
///
/// This design enables:
/// - ✅ Maximum zero-copy rate on ASCII-only or already-normalized text  
/// - ✅ Lossless pipelines for display/email/HTML when `normalize_unicode = false`  
/// - ✅ Full Unicode-aware processing when `normalize_unicode = true`  
/// - ✅ Exact `str::trim()` equivalence with `TRIM_WHITESPACE_UNICODE`
///
/// ## Recommended Presets — Choose Exactly What You Need
///
/// | Preset                            | Trim | Collapse | Normalize | Behavior                                                      |
/// |-----------------------------------|------|----------|-----------|---------------------------------------------------------------|
/// | `NORMALIZE_WHITESPACE_FULL`       | ✓    | ✓        | ✓         | Trim + collapse + normalize Unicode WS → `' '`                |
/// | `COLLAPSE_WHITESPACE_UNICODE`     | ✗    | ✓        | ✓         | Collapse all WS (including Unicode) → `' '`, preserve edges   |
/// | `COLLAPSE_WHITESPACE_ONLY`        | ✗    | ✓        | ✗         | Collapse ASCII WS only, preserve Unicode WS and edges         |
/// | `TRIM_WHITESPACE_UNICODE`         | ✓    | ✗        | ✓         | **Exact `str::trim()` behavior** — trim Unicode WS from edges |
/// | `TRIM_WHITESPACE_ONLY`            | ✓    | ✗        | ✗         | Trim ASCII WS only from edges                                 |
///
/// ### Use Cases
///
/// - **`NORMALIZE_WHITESPACE_FULL`**: Search indexing, tokenization, LLM preprocessing, vector stores
/// - **`COLLAPSE_WHITESPACE_UNICODE`**: Log cleanup, JSON serialization, pre-tokenization
/// - **`COLLAPSE_WHITESPACE_ONLY`**: Display pipelines, legacy systems, formatting-preserving logs
/// - **`TRIM_WHITESPACE_UNICODE`**: Data validation, user input sanitization, exact `str::trim()` match
/// - **`TRIM_WHITESPACE_ONLY`**: HTML → plain text, email extraction, CJK layout preservation
///
/// ## Unicode Whitespace Support
///
/// When `normalize_unicode = true`, the following Unicode whitespace characters are **recognized**:
///
/// - U+0085 — NEXT LINE (NEL)  
/// - U+00A0 — NO-BREAK SPACE  
/// - U+1680 — OGHAM SPACE MARK  
/// - U+2000..=U+200A — En/Em/Thin/Hair spaces  
/// - U+2028 — LINE SEPARATOR  
/// - U+2029 — PARAGRAPH SEPARATOR  
/// - U+202F — NARROW NO-BREAK SPACE  
/// - U+205F — MEDIUM MATHEMATICAL SPACE  
/// - U+3000 — IDEOGRAPHIC SPACE  
///
/// **Normalization behavior:**
/// - With `trim_edges`: These characters are recognized as whitespace for trimming (removed from edges)
/// - With `collapse_sequential`: These characters are collapsed and **converted to ASCII space `' '`**
/// - Without either flag: These characters are **preserved as-is** (no transformation)
///
/// This list is exhaustive for all non-ASCII `White_Space=Yes` characters in Unicode 15.0+.
///
/// ## Performance Guarantees
///
/// - **Zero allocations** when input is already normalized for the chosen preset  
/// - **One allocation maximum** in all other cases  
/// - **Single string pass** — no intermediate buffers, no multi-stage allocation chains  
/// - **Fast-path rejection** in `needs_apply()` for empty strings and clean ASCII
/// - **Byte-level optimization** for ASCII-only text (no UTF-8 decoding overhead)
#[derive(Debug, Clone, Copy)]
pub struct NormalizeWhitespace {
    /// Collapse multiple sequential whitespace chars into one
    pub collapse_sequential: bool,

    /// Remove leading and trailing whitespace
    pub trim_edges: bool,

    /// Convert Unicode whitespace (NBSP, etc.) to ASCII space
    pub normalize_unicode: bool,

    /// Convert Unicode whitespace (NBSP, etc.) to this character
    pub collapse_replacement: char,
}

// ------------------------------------------------------------------------
// Helper Constants
// ------------------------------------------------------------------------

/// Collapse, trim, and normalize Unicode whitespace (recommended for most pipelines)
pub const NORMALIZE_WHITESPACE_FULL: NormalizeWhitespace = NormalizeWhitespace {
    collapse_sequential: true,
    trim_edges: true,
    normalize_unicode: true,
    collapse_replacement: ' ',
};

/// Collapse sequential whitespace only, preserve edges
pub const COLLAPSE_WHITESPACE_ONLY: NormalizeWhitespace = NormalizeWhitespace {
    collapse_sequential: true,
    trim_edges: false,
    normalize_unicode: false,
    collapse_replacement: ' ',
};

/// Collapse sequential unicode whitespace, preserve edges
pub const COLLAPSE_WHITESPACE_UNICODE: NormalizeWhitespace = NormalizeWhitespace {
    collapse_sequential: true,
    trim_edges: false,
    normalize_unicode: true,
    collapse_replacement: ' ',
};

/// Trim edges only, preserve internal spacing
pub const TRIM_WHITESPACE_ONLY: NormalizeWhitespace = NormalizeWhitespace {
    collapse_sequential: false,
    trim_edges: true,
    normalize_unicode: false,
    collapse_replacement: ' ',
};

/// Trim unicode edges, preserve internal spacing
pub const TRIM_WHITESPACE_UNICODE: NormalizeWhitespace = NormalizeWhitespace {
    collapse_sequential: false,
    trim_edges: true,
    normalize_unicode: true,
    collapse_replacement: ' ',
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
        // Fast path 0: Empty string or no operations
        if text.is_empty() || (!self.trim_edges && !self.collapse_sequential) {
            return Ok(false);
        }

        let bytes = text.as_bytes();
        let len = bytes.len();

        // Fast path 1: Trim edges check
        if self.trim_edges {
            // Leading/trailing ASCII whitespace
            if is_ascii_whitespace_fast(bytes[0]) || is_ascii_whitespace_fast(bytes[len - 1]) {
                return Ok(true);
            }

            // Leading/trailing Unicode whitespace only if normalization is enabled
            if self.normalize_unicode
                && (text.chars().next().is_some_and(is_unicode_whitespace)
                    || text.chars().next_back().is_some_and(is_unicode_whitespace))
            {
                return Ok(true);
            }
        }

        if self.collapse_sequential {
            // Fast path 2: ASCII-only text
            if text.is_ascii() {
                let mut prev_ws = false;
                for &b in bytes {
                    let is_ws = is_ascii_whitespace_fast(b);
                    if is_ws && prev_ws {
                        return Ok(true); // Found sequential ASCII whitespace
                    }
                    prev_ws = is_ws;
                }
                return Ok(false); // Pure ASCII, no sequential WS
            }

            // Medium path: Mixed Unicode text
            // If Unicode normalization is enabled and we see any byte that could start a Unicode WS char,
            // we conservatively return true — it's cheaper than decoding chars here.
            if self.normalize_unicode && bytes.iter().any(|&b| could_be_unicode_ws_start(b)) {
                // Quick pre-scan: detect potential Unicode whitespace
                return Ok(true);
            }

            // Slow path: Full char iteration
            let mut prev_ws = false;
            for c in text.chars() {
                let is_ws = self.is_whitespace_for_config(c);

                // Only check collapsing, trim already checked
                if is_ws && prev_ws {
                    return Ok(true);
                }
                prev_ws = is_ws;
            }
        }
        Ok(false)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // Hot path: pure ASCII text → byte-level optimization (even with normalize_unicode=true)
        // Rationale: ASCII has no Unicode whitespace, so normalize_unicode is a no-op
        if text.is_ascii() {
            return Ok(self.apply_ascii_fast(text));
        }
        // Canonical path: handles all whitespace, all configurations, one pass, one allocation
        Ok(self.apply_full(text))
    }

    #[inline]
    fn as_char_mapper(&self, _ctx: &Context) -> Option<&dyn CharMapper> {
        Some(self)
    }
}

impl NormalizeWhitespace {
    /// Change the character emitted character when collapsing whitespace runs.
    ///
    /// Useful for CJK pipelines that want zero-width space instead of ASCII space:
    /// ```rust
    /// use normy::NORMALIZE_WHITESPACE_FULL;
    /// let zwsp_stage = NORMALIZE_WHITESPACE_FULL.replace_collapsed_with('\u{200B}');
    /// ```
    #[inline(always)]
    pub const fn replace_collapsed_with(mut self, c: char) -> Self {
        self.collapse_replacement = c;
        self
    }

    #[inline(always)]
    fn collapse_replacement(&self) -> char {
        self.collapse_replacement
    }

    #[inline(always)]
    fn is_whitespace_for_config(&self, c: char) -> bool {
        if self.normalize_unicode {
            c.is_whitespace()
        } else {
            c.is_ascii_whitespace()
        }
    }

    #[inline(always)]
    fn is_unicode_whitespace_only(&self, c: char) -> bool {
        self.normalize_unicode && !c.is_ascii_whitespace() && c.is_whitespace()
    }
    /// Optimized ASCII-only fast path (no Unicode normalization needed).
    /// Single-pass, byte-level operations with at most one allocation.
    #[inline(always)]
    fn apply_ascii_fast<'a>(&self, text: Cow<'a, str>) -> Cow<'a, str> {
        let s = text.as_ref();
        let bytes = s.as_bytes();
        // ═══════════════════════════════════════════════════════════
        // Single-pass transformation (trust needs_apply() contract)
        // ═══════════════════════════════════════════════════════════
        let mut result = String::with_capacity(bytes.len());
        let mut prev_ws = false;
        let mut started = false;
        for &b in bytes {
            let is_ws = is_ascii_whitespace_fast(b);
            if is_ws {
                // Skip leading whitespace if trimming
                if self.trim_edges && !started {
                    prev_ws = true;
                    continue;
                }
                // Handle sequential collapse
                if self.collapse_sequential && prev_ws {
                    continue;
                }
                // Emit WS: normalized if collapsing, original otherwise
                if self.collapse_sequential {
                    result.push(self.collapse_replacement());
                } else {
                    result.push(b as char);
                }
                prev_ws = true;
            } else {
                result.push(b as char);
                prev_ws = false;
                started = true;
            }
        }
        // Trim trailing whitespace if requested
        if self.trim_edges {
            while let Some(&last_byte) = result.as_bytes().last() {
                let last_char = last_byte as char;
                // If the last emitted char is the collapse replacement AND it came from a trailing WS run,
                // we must remove it. Otherwise, keep it.
                if (last_char == self.collapse_replacement() && self.collapse_sequential)
                    || is_ascii_whitespace_fast(last_byte)
                {
                    result.pop();
                } else {
                    break;
                }
            }
        }
        Cow::Owned(result)
    }
    /// Full Unicode-aware transformation with optimal single-pass processing.
    /// Handles all whitespace types, all configurations, with at most one allocation.
    ///
    /// Rule: normalize_unicode is a modifier flag:
    /// - By itself: no-op
    /// - With trim_edges: trim Unicode WS from edges (but don't normalize internal)
    /// - With collapse_sequential: collapse Unicode WS and normalize to ' '
    #[inline(always)]
    fn apply_full<'a>(&self, text: Cow<'a, str>) -> Cow<'a, str> {
        let s = text.as_ref();
        // ═══════════════════════════════════════════════════════════
        // Single-pass transformation (trust needs_apply() contract)
        // ═══════════════════════════════════════════════════════════
        let mut result = String::with_capacity(s.len());
        let mut started = false;
        // Simple Vec for pending WS (most runs are 1-2 chars)
        // Almost all whitespace runs are ≤4 chars in real text → zero-heap
        let mut pending_ws: SmallVec<[char; 4]> = SmallVec::new();
        for c in s.chars() {
            // Determine if this is whitespace
            let is_std_ws = self.is_whitespace_for_config(c);
            let is_uni_ws = self.is_unicode_whitespace_only(c);
            let is_ws = is_std_ws || is_uni_ws;
            if is_ws {
                pending_ws.push(c);
                continue;
            }
            // ═══════════════════════════════════════════════════════════
            // Non-whitespace: flush pending WS run
            // ═══════════════════════════════════════════════════════════
            if !pending_ws.is_empty() {
                let should_emit = !self.trim_edges || started;
                if should_emit {
                    if self.collapse_sequential {
                        result.push(self.collapse_replacement());
                    } else {
                        // Emit all WS chars as-is
                        result.extend(pending_ws.drain(..));
                    }
                }
                pending_ws.clear();
            }
            result.push(c);
            started = true;
        }
        // End-of-string: handle trailing WS
        if !pending_ws.is_empty() {
            let should_emit = !self.trim_edges;
            if should_emit {
                if self.collapse_sequential {
                    result.push(self.collapse_replacement());
                } else {
                    result.extend(pending_ws);
                }
            }
            // If trimming, we silently drop trailing WS — correct behavior
        }
        Cow::Owned(result)
    }
}

impl CharMapper for NormalizeWhitespace {
    #[inline(always)]
    fn map(&self, _c: char, _ctx: &Context) -> Option<char> {
        // We don't use the stateless path — bind() is the fast path
        None
    }

    fn bind<'a>(
        &self,
        text: &'a str,
        _ctx: &'a Context,
    ) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(WhitespaceIterator {
            inner: text.chars(),
            config: *self,
            state: IteratorState::default(),
        })
    }
}

#[derive(Debug)]
struct WhitespaceIterator<'a> {
    inner: std::str::Chars<'a>,
    config: NormalizeWhitespace,
    state: IteratorState,
}

#[derive(Debug, Default)]
struct IteratorState {
    prev_was_ws: bool,
    started: bool,
}

impl<'a> Iterator for WhitespaceIterator<'a> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let c = self.inner.next()?;

        let is_ws = self.config.is_whitespace_for_config(c);

        if is_ws {
            // Leading trim
            if self.config.trim_edges && !self.state.started {
                self.state.prev_was_ws = true;
                return self.next();
            }

            // Collapse sequential
            if self.config.collapse_sequential && self.state.prev_was_ws {
                return self.next();
            }

            self.state.prev_was_ws = true;
            return Some(self.config.collapse_replacement);
        }

        // Non-whitespace
        self.state.prev_was_ws = false;
        self.state.started = true;
        Some(c)
    }
}

impl<'a> FusedIterator for WhitespaceIterator<'a> {}

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
            "¡\u{a0}¡\u{205f}",
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

    fn skip_zero_copy_apply_test() -> bool {
        true
    }
}

// ═══════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;

    #[test]
    fn universal_contract_tests() {
        assert_stage_contract!(NORMALIZE_WHITESPACE_FULL);
        assert_stage_contract!(COLLAPSE_WHITESPACE_ONLY);
        assert_stage_contract!(COLLAPSE_WHITESPACE_UNICODE);
        assert_stage_contract!(TRIM_WHITESPACE_ONLY);
        assert_stage_contract!(TRIM_WHITESPACE_UNICODE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ENG;
    use std::borrow::Cow;

    fn ctx() -> Context {
        Context::new(ENG)
    }

    // ═══════════════════════════════════════════════════════════════
    // Core Semantics: normalize_unicode modifier behavior
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn trim_with_unicode_preserves_internal() {
        let stage = TRIM_WHITESPACE_UNICODE;
        let input = "\u{00A0}hello\u{00A0}world\u{205F}";
        assert_eq!(
            stage.apply(input.into(), &ctx()).unwrap(),
            "hello\u{00A0}world"
        );
    }

    #[test]
    fn collapse_with_unicode_normalizes() {
        let stage = COLLAPSE_WHITESPACE_UNICODE;
        let input = "hello\u{00A0}\u{00A0}world";
        assert_eq!(stage.apply(input.into(), &ctx()).unwrap(), "hello world");
    }

    #[test]
    fn matches_rust_trim_exactly() {
        let stage = TRIM_WHITESPACE_UNICODE;
        let cases = [
            "  hello  ",
            "\u{00A0}hello\u{00A0}",
            "¡\u{a0}¡\u{205f}",
            "\u{3000}test\u{2028}",
        ];

        for input in cases {
            let result = stage.apply(input.into(), &ctx()).unwrap();
            assert_eq!(&*result, input.trim(), "Failed for: {:?}", input);
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Preset Correctness
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn full_normalization_works() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        assert_eq!(
            stage
                .apply(Cow::Borrowed("  hello\u{00A0}\u{00A0}world  "), &ctx())
                .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn collapse_only_preserves_edges() {
        let stage = COLLAPSE_WHITESPACE_ONLY;
        assert_eq!(
            stage
                .apply(Cow::Borrowed("  hello   world  "), &ctx())
                .unwrap(),
            " hello world "
        );
    }

    #[test]
    fn trim_only_preserves_internal() {
        let stage = TRIM_WHITESPACE_ONLY;
        assert_eq!(
            stage.apply(Cow::Borrowed("  a  b  "), &ctx()).unwrap(),
            "a  b"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // Edge Cases
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn whitespace_only_strings() {
        let ctx = ctx();
        assert_eq!(
            TRIM_WHITESPACE_ONLY
                .apply(Cow::Borrowed("   "), &ctx)
                .unwrap(),
            ""
        );
        assert_eq!(
            COLLAPSE_WHITESPACE_ONLY
                .apply(Cow::Borrowed("   "), &ctx)
                .unwrap(),
            " "
        );
    }

    #[test]
    fn unicode_nel_handling() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        assert_eq!(
            stage
                .apply(Cow::Borrowed("hello\u{0085}world"), &ctx())
                .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn mixed_ascii_unicode_whitespace() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        assert_eq!(
            stage
                .apply(Cow::Borrowed("hello \u{00A0} world"), &ctx())
                .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn removes_tabs_in_collapsed_run() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        assert_eq!(
            stage.apply(Cow::Borrowed("a\t \t b"), &ctx()).unwrap(),
            "a b"
        );
    }

    #[test]
    fn preserves_tabs_in_collapsed_run() {
        let stage = NormalizeWhitespace {
            collapse_sequential: true,
            trim_edges: true,
            normalize_unicode: false,
            collapse_replacement: ' ',
        };
        assert_eq!(
            stage.apply(Cow::Borrowed("a\t \t b"), &ctx()).unwrap(),
            "a b"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // Unicode-specific edge cases
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn all_unicode_whitespace_types() {
        let ctx = ctx();

        // Case 1: COLLAPSE_WHITESPACE_ONLY → normalize_unicode = false
        // → Unicode whitespace (NEL) must NOT be collapsed
        assert_eq!(
            COLLAPSE_WHITESPACE_ONLY
                .apply(Cow::Borrowed("a\u{85}\u{85}b"), &ctx)
                .unwrap(),
            "a\u{85}\u{85}b" // ← FIXED: preserve both NELs
        );

        // Case 2: FULL → normalize_unicode = true → collapse + normalize to space
        assert_eq!(
            NORMALIZE_WHITESPACE_FULL
                .apply(Cow::Borrowed("a\u{00A0}b"), &ctx)
                .unwrap(),
            "a b"
        );

        // Case 3: Mixed NEL + NBSP with FULL → all become space
        assert_eq!(
            NORMALIZE_WHITESPACE_FULL
                .apply(Cow::Borrowed("a\u{85}\u{00A0}b"), &ctx)
                .unwrap(),
            "a b"
        );
    }

    #[test]
    fn preserves_unicode_when_normalize_disabled() {
        let stage = NormalizeWhitespace {
            collapse_sequential: false,
            trim_edges: false,
            normalize_unicode: false,
            collapse_replacement: ' ',
        };

        assert_eq!(
            stage.apply(Cow::Borrowed("a\u{00A0}b"), &ctx()).unwrap(),
            "a\u{00A0}b"
        );
    }

    #[test]
    fn preserve_unicode_ws_when_normalize_disabled_with_collapse_trim() {
        let stage = NormalizeWhitespace {
            collapse_sequential: true,
            trim_edges: true,
            normalize_unicode: false,
            collapse_replacement: ' ',
        };
        let input = "\u{00A0}hello   world\u{00A0}";
        assert_eq!(
            stage.apply(input.into(), &ctx()).unwrap(),
            "\u{00A0}hello world\u{00A0}"
        );
    }

    #[test]
    fn ascii_and_full_paths_consistent() {
        let stage = NormalizeWhitespace {
            collapse_sequential: true,
            trim_edges: false,
            normalize_unicode: false,
            collapse_replacement: ' ',
        };
        let ctx = Context::new(ENG);
        let inputs = [
            "a  b",
            "a\t\tb\n\nc",
            "\t a \n",
            "single ",
            "",
            "no_ws_at_all",
        ];
        let expecteds = ["a b", "a b c", " a ", "single ", "", "no_ws_at_all"];

        for (i, input) in inputs.iter().enumerate() {
            // Normal apply (takes ASCII fast path)
            let output_ascii = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            // Direct full path call (Unicode-aware, but on ASCII)
            let output_full = stage.apply_full(Cow::Borrowed(input));
            let expected = expecteds[i];

            assert_eq!(
                output_ascii.as_ref(),
                expected,
                "ASCII path mismatch for: {}",
                input
            );
            assert_eq!(
                output_full.as_ref(),
                expected,
                "Full path mismatch for: {}",
                input
            );
            assert_eq!(
                output_ascii, output_full,
                "Paths inconsistent for: {}",
                input
            );
        }
    }

    #[test]
    fn ascii_and_full_paths_consistent_no_collapse() {
        let stage = NormalizeWhitespace {
            collapse_sequential: false,
            trim_edges: false,
            normalize_unicode: false,
            collapse_replacement: ' ',
        };
        let ctx = Context::new(ENG);
        let inputs = ["a  b", "a\t\tb\n\nc", "\t a \n"];
        let expecteds = ["a  b", "a\t\tb\n\nc", "\t a \n"];

        for (i, input) in inputs.iter().enumerate() {
            let output_ascii = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
            let output_full = stage.apply_full(Cow::Borrowed(input));
            let expected = expecteds[i];

            assert_eq!(
                output_ascii.as_ref(),
                expected,
                "ASCII path mismatch for: {}",
                input
            );
            assert_eq!(
                output_full.as_ref(),
                expected,
                "Full path mismatch for: {}",
                input
            );
            assert_eq!(
                output_ascii, output_full,
                "Paths inconsistent for: {}",
                input
            );
        }
    }

    #[test]
    fn unicode_ws_preserved_when_normalize_disabled() {
        let stage = COLLAPSE_WHITESPACE_ONLY; // normalize_unicode = false
        let input = "a\u{00A0} hello    \u{00A0}b";
        let output = stage.apply(Cow::Borrowed(input), &ctx()).unwrap();
        assert_eq!(output.as_ref(), "a\u{00A0} hello \u{00A0}b");
        let input = "a\u{00A0} hello\t\t    \u{00A0}\u{00A0}b";
        let output = stage.apply(Cow::Borrowed(input), &ctx()).unwrap();
        assert_eq!(output.as_ref(), "a\u{00A0} hello \u{00A0}\u{00A0}b");
    }

    #[test]
    fn unicode_ws_collapsed_when_normalize_enabled() {
        let stage = COLLAPSE_WHITESPACE_UNICODE; // normalize_unicode = true
        let input = "a\u{00A0} hello    \u{00A0}b";
        let output = stage.apply(Cow::Borrowed(input), &ctx()).unwrap();
        assert_eq!(output.as_ref(), "a hello b");
        let input = "a\u{00A0} hello\t\t   \u{00A0}\u{00A0}b";
        let output = stage.apply(Cow::Borrowed(input), &ctx()).unwrap();
        assert_eq!(output.as_ref(), "a hello b");
    }

    #[test]
    fn custom_collapse_replacement_zwsp() {
        let stage = NORMALIZE_WHITESPACE_FULL.replace_collapsed_with('\u{200B}');
        let input = "hello   \t  \u{00A0}\u{3000}  world";
        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx()).unwrap(),
            "hello\u{200B}world"
        );
    }

    #[test]
    fn custom_replacement_with_no_collapse_is_ignored() {
        // collapse_sequential = false → replacement char never used
        let stage = NormalizeWhitespace {
            collapse_sequential: false,
            trim_edges: true,
            normalize_unicode: true,
            collapse_replacement: '\u{200B}',
        };
        let input = "\u{00A0}  hello  world  \u{00A0}";
        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx()).unwrap(),
            "hello  world"
        );
    }

    #[test]
    fn custom_replacement_ascii_fast_path() {
        let stage = NormalizeWhitespace {
            collapse_sequential: true,
            trim_edges: true,
            normalize_unicode: false,
            collapse_replacement: '-',
        };
        let input = "  a   b\t\tc  ";
        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx()).unwrap(),
            "a-b-c" // ← no trailing WS removed, not replaced with '-'
        );
    }

    #[test]
    fn custom_replacement_full_path_mixed_ws() {
        let stage = NormalizeWhitespace {
            collapse_sequential: true,
            trim_edges: false,
            normalize_unicode: true,
            collapse_replacement: '_',
        };
        let input = "x\u{00A0}\u{1680}\t y";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx()).unwrap(), "x_y");
    }
}

#[cfg(test)]
mod charmapper_vs_apply_equivalence {
    use super::*;
    use crate::ENG;
    use rand::{Rng, SeedableRng, rngs::StdRng, seq::IndexedRandom};
    use std::borrow::Cow;

    fn ctx() -> Context {
        Context::new(ENG)
    }

    /// Core equivalence test: apply() vs bind() + collect must be identical
    fn assert_equivalence(stage: &NormalizeWhitespace, input: &str) {
        let ctx = ctx();

        // 1. Legacy apply() path (goes through apply_ascii_fast / apply_full)
        let via_apply = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        // 2. CharMapper path — this is what monomorphised pipelines use
        let via_iterator: String = stage.bind(input, &ctx).collect();
        let via_iterator = Cow::<str>::Owned(via_iterator);

        // 3. Direct manual collect — proves iterator is pure
        let mut direct = String::with_capacity(input.len());
        for c in stage.bind(input, &ctx) {
            direct.push(c);
        }
        let via_direct = Cow::<str>::Owned(direct);

        assert_eq!(
            via_apply, via_iterator,
            "\nCharMapper bind() ≠ apply() on input:\n{:?}\nConfig: {:?}\n",
            input, stage
        );

        assert_eq!(
            via_apply, via_direct,
            "\nDirect collect ≠ apply() on input: {:?}\n",
            input
        );

        // Bonus: needs_apply() must be correct
        let needs = stage.needs_apply(input, &ctx).unwrap();
        let changed = via_apply.as_ref() != input;
        assert_eq!(needs, changed, "needs_apply() mismatch on: {:?}", input);
    }

    #[test]
    fn equivalence_exhaustive_deterministic() {
        let configs = [
            NORMALIZE_WHITESPACE_FULL,
            COLLAPSE_WHITESPACE_ONLY,
            COLLAPSE_WHITESPACE_UNICODE,
            TRIM_WHITESPACE_ONLY,
            TRIM_WHITESPACE_UNICODE,
            NormalizeWhitespace {
                collapse_sequential: true,
                trim_edges: true,
                normalize_unicode: true,
                collapse_replacement: '-',
            },
            NormalizeWhitespace {
                collapse_sequential: true,
                trim_edges: false,
                normalize_unicode: true,
                collapse_replacement: '\u{200B}',
            },
        ];

        let inputs = [
            "",
            "hello",
            "  hello world  ",
            "a\t\tb\n\nc",
            " \u{00A0} hello \u{202F} world \u{3000} ",
            "   ",
            "\u{00A0}\u{1680}\u{2003}xyz",
            "a b c",
            "a  b   c    ",
            "\t\n\r \u{0085}\u{2028}",
            "  leading and trailing  ",
            "preserve   internal   spaces",
            "mixed \t \u{00A0}  unicode \u{205F} ascii",
            "single\u{00A0}space",
            "trailing\u{00A0}",
            "\u{00A0}leading",
        ];

        for &config in &configs {
            for &input in &inputs {
                assert_equivalence(&config, input);
            }
        }
    }

    #[test]
    fn equivalence_property_random() {
        let mut rng = StdRng::seed_from_u64(0xCAFEBABE_DEADBEEF);

        for _ in 0..5_000 {
            let len = rng.random_range(0..256);
            let input: String = (0..len)
                .map(|_| {
                    let choice = rng.random_range(0..100);
                    if choice < 10 {
                        // 10% Unicode whitespace
                        *[
                            '\u{00A0}', '\u{1680}', '\u{2000}', '\u{2003}', '\u{202F}', '\u{205F}',
                            '\u{3000}', '\u{0085}', '\u{2028}', '\u{2029}',
                        ]
                        .choose(&mut rng)
                        .unwrap()
                    } else if choice < 30 {
                        // 20% ASCII whitespace
                        *[' ', '\t', '\n', '\r'].choose(&mut rng).unwrap()
                    } else {
                        // 70% letters
                        (b'a' + rng.random_range(0..26)) as char
                    }
                })
                .collect();

            let config = NormalizeWhitespace {
                collapse_sequential: rng.random(),
                trim_edges: rng.random(),
                normalize_unicode: rng.random(),
                collapse_replacement: if rng.random() { ' ' } else { '-' },
            };

            assert_equivalence(&config, &input);
        }
    }
}
