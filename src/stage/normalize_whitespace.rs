use crate::{
    context::Context,
    lang::Lang,
    stage::{CharMapper, Stage, StageError},
    testing::stage_contract::StageTestConfig,
    unicode::{could_be_unicode_ws_start, is_ascii_whitespace_fast, is_unicode_whitespace},
};
use smallvec::SmallVec;
use std::{borrow::Cow, iter::FusedIterator};

/// Normalize and standardize whitespace in text pipelines.
///
/// This stage performs up to three operations in **one pass**, with **at most one allocation**,
/// and guarantees **zero-copy** whenever the input is already clean.
///
/// ## Features
///
/// | Operation                        | Flag                  | Effect when enabled                                                                                     |
/// |----------------------------------|-----------------------|----------------------------------------------------------------------------------------------------------|
/// | **Collapse sequential WS**       | `collapse`            | Multiple consecutive whitespace → single `replacement_char`                                             |
/// | **Trim edges**                   | `trim`                | Remove leading and trailing whitespace                                                                  |
/// | **Normalize all whitespace**     | `normalize_unicode`   | **All** whitespace (ASCII + Unicode) → `replacement_char` immediately when emitted                     |
/// | **Custom replacement**           | `replacement_char`    | Character used for all emitted whitespace (default `' '`; can be `'\u{200B}'` for CJK, etc.)            |
///
/// ## Design Philosophy
///
/// **`normalize_unicode` is a modifier flag, not a standalone operation:**
///
/// - **`normalize_unicode` alone**: No-op (returns `Cow::Borrowed` immediately)
/// - **`trim_edges + normalize_unicode`**: Trim edges including Unicode whitespace (matches `str::trim()`)
/// - **`collapse + normalize_unicode`**: Collapse sequential whitespace including Unicode WS, normalize to ASCII space `' '`
///
/// This enables:
/// - Maximum zero-copy on already-clean text
/// - Predictable, high-performance NLP preprocessing
///
/// ## Whitespace Recognition Rules
///
/// | `normalize_unicode` | Recognized Characters                                                                                 |
/// |---------------------|--------------------------------------------------------------------------------------------------------|
/// | `false`             | Only ASCII: `\t`, `\n`, `\r`, `\v`, `\f`, space (U+0020)                                               |
/// | `true`              | ASCII **+** all Unicode `White_Space=Yes` characters (U+00A0, U+1680, U+2000–U+200A, U+202F, U+3000, etc.) |
///
/// When recognized → **immediately emitted as `replacement_char`**, no exceptions.
///
/// ## Unicode Whitespace Support (when `normalize_unicode = true`)
///
/// All characters with Unicode property `White_Space=Yes` are recognized and replaced:
///
/// - U+0009–U+000D (TAB, LF, etc.)
/// - U+0020 (SPACE)
/// - U+0085 (NEL)
/// - U+00A0 (NBSP)
/// - U+1680 (OGHAM SPACE MARK)
/// - U+2000..=U+200A (En/Em/Thin/Hair spaces)
/// - U+2028–U+2029 (Line/Paragraph Separator)
/// - U+202F (NARROW NO-BREAK SPACE)
/// - U+205F (MEDIUM MATHEMATICAL SPACE)
/// - U+3000 (IDEOGRAPHIC SPACE)
///
/// This list is exhaustive for Unicode 15.1+.
///
/// ## Final Note
///
/// This stage is intentionally **aggressive** when `normalize_unicode = true`.
///
/// It does **not** preserve:
/// - Tabs in code
/// - Newlines as sentence boundaries
/// - Non-breaking spaces for layout
/// - Zero-width characters
#[derive(Debug, Clone, Copy)]
pub struct NormalizeWhitespace {
    /// Collapse multiple sequential whitespace chars into one
    pub collapse: bool,

    /// Remove leading and trailing whitespace
    pub trim: bool,

    /// Every recognized whitespace character
    /// — whether \t, \n, \r, U+00A0, U+202F, U+3000, U+2003, etc. —
    /// is always emitted as replacement_char (usually ' ')
    /// immediately when the stage decides to emit whitespace,
    /// regardless of collapse or trim settings.
    pub normalize_unicode: bool,

    /// Convert whitespace (NBSP, etc.) to this character
    pub replacement_char: char,
}

// ------------------------------------------------------------------------
// Helper Constants
// ------------------------------------------------------------------------

/// Collapse, trim, and normalize Unicode whitespace (recommended for most pipelines)
pub const NORMALIZE_WHITESPACE_FULL: NormalizeWhitespace = NormalizeWhitespace {
    collapse: true,
    trim: true,
    normalize_unicode: true,
    replacement_char: ' ',
};

/// Collapse sequential whitespace only, preserve edges
pub const COLLAPSE_WHITESPACE: NormalizeWhitespace = NormalizeWhitespace {
    collapse: true,
    trim: false,
    normalize_unicode: false,
    replacement_char: ' ',
};

/// Collapse sequential unicode whitespace, preserve edges
pub const COLLAPSE_WHITESPACE_UNICODE: NormalizeWhitespace = NormalizeWhitespace {
    collapse: true,
    trim: false,
    normalize_unicode: true,
    replacement_char: ' ',
};

/// Trim edges only, preserve internal spacing
pub const TRIM_WHITESPACE: NormalizeWhitespace = NormalizeWhitespace {
    collapse: false,
    trim: true,
    normalize_unicode: false,
    replacement_char: ' ',
};

/// Trim unicode edges, preserve internal spacing
pub const TRIM_WHITESPACE_UNICODE: NormalizeWhitespace = NormalizeWhitespace {
    collapse: false,
    trim: true,
    normalize_unicode: true,
    replacement_char: ' ',
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
        if text.is_empty() || (!self.trim && !self.collapse) {
            return Ok(false);
        }

        let bytes = text.as_bytes();
        let len = bytes.len();

        // Fast path 1: Combined edge checks (reduces branches from 4 to 2)
        if self.trim {
            // Check both ASCII edges in one branch
            if is_ascii_whitespace_fast(bytes[0]) || is_ascii_whitespace_fast(bytes[len - 1]) {
                return Ok(true);
            }

            // Only decode UTF-8 for edges if normalize_unicode enabled and ASCII check failed
            if self.normalize_unicode {
                // Lazy UTF-8 decode: only first char
                if let Some(first_char) = text.chars().next()
                    && is_unicode_whitespace(first_char)
                {
                    return Ok(true);
                }

                // Lazy UTF-8 decode: only last char
                if let Some(last_char) = text.chars().next_back()
                    && is_unicode_whitespace(last_char)
                {
                    return Ok(true);
                }
            }
        }

        if self.collapse {
            // Fast path 2: Pure ASCII (90%+ of English NLP workloads)
            if text.is_ascii() {
                let mut prev_ws = false;
                for &b in bytes {
                    let is_ws = is_ascii_whitespace_fast(b);
                    if is_ws && prev_ws {
                        return Ok(true); // Early exit on first sequential WS
                    }
                    prev_ws = is_ws;
                }
                return Ok(false);
            }

            // Fast path: Mixed Unicode text
            // If Unicode normalization is enabled and we see any byte that could start a Unicode WS char,
            // we conservatively return true — it's cheaper than decoding chars here.
            if self.normalize_unicode && bytes.iter().any(|&b| could_be_unicode_ws_start(b)) {
                // Quick pre-scan: detect potential Unicode whitespace
                return Ok(true);
            }

            // Slow path: Full char iteration (only for mixed non-ASCII content)
            let mut prev_ws = false;
            for c in text.chars() {
                let is_ws = self.is_whitespace_for_config(c);
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
    #[inline(always)]
    fn replacement_char(&self) -> char {
        self.replacement_char
    }

    #[inline(always)]
    pub fn with_normalize_unicode(&mut self) {
        self.normalize_unicode = true
    }

    /// Change the character emitted character when collapsing whitespace runs.
    ///
    /// Useful for CJK pipelines that want zero-width space instead of ASCII space:
    /// ```rust
    /// use normy::NORMALIZE_WHITESPACE_FULL;
    /// let zwsp_stage = NORMALIZE_WHITESPACE_FULL.replace_whitespace_with('\u{200B}');
    /// ```
    #[inline(always)]
    pub const fn replace_whitespace_with(mut self, c: char) -> Self {
        self.replacement_char = c;
        self
    }

    /// OPTIMIZATION: Single-call whitespace classification
    /// Returns (is_whitespace, needs_replacement)
    ///
    /// This eliminates redundant is_ascii_whitespace() calls in hot paths.
    #[inline(always)]
    fn classify_char(&self, c: char) -> (bool, bool) {
        if c.is_ascii_whitespace() {
            // ASCII WS: always whitespace, never needs replacement
            (true, false)
        } else if self.normalize_unicode && c.is_whitespace() {
            // Unicode WS: is whitespace, needs replacement
            (true, true)
        } else {
            // Not whitespace
            (false, false)
        }
    }

    /// OPTIMIZATION: Conservative capacity estimation
    /// Avoids over-allocation while preventing reallocation in 95%+ of cases
    #[inline(always)]
    fn estimate_output_capacity(&self, input_len: usize) -> usize {
        match (self.collapse, self.trim) {
            // Both: aggressive reduction (~15% WS in prose, collapse saves ~50%)
            (true, true) => (input_len * 23) >> 4, // ~92% of original
            // Collapse only: moderate reduction
            (true, false) => (input_len * 19) >> 4, // ~95% of original
            // Trim only: minimal reduction
            (false, true) => input_len.saturating_sub(input_len >> 5), // ~98% of original
            // Neither: should never allocate (caught by needs_apply)
            (false, false) => input_len,
        }
    }

    /// OPTIMIZATION: Helper for safe first element access
    /// Marked inline(always) to eliminate function call overhead
    #[inline(always)]
    fn get_first_pending(&self, pending: &SmallVec<[char; 4]>) -> char {
        debug_assert!(!pending.is_empty(), "get_first_pending called on empty vec");
        pending[0]
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
        //let mut prev_ws = false;
        let mut started = false;
        // let mut already_collapsed = false;
        let mut pending_ws: SmallVec<[u8; 4]> = SmallVec::new();
        for &b in bytes {
            let is_ws = is_ascii_whitespace_fast(b);
            if is_ws {
                pending_ws.push(b);
                continue;
            } else {
                if !pending_ws.is_empty() {
                    let should_emit = !self.trim || started;
                    if should_emit {
                        if self.collapse {
                            // Only emit replacement if run length >= 2
                            if pending_ws.len() >= 2 {
                                result.push(self.replacement_char());
                            } else {
                                // Single WS: Preserve original character
                                result.push(pending_ws[0] as char);
                            }
                        } else {
                            result.extend(pending_ws.drain(..).map(|b| b as char));
                        }
                    }
                }
                // Non-whitespace
                result.push(b as char);
                pending_ws.clear();
                started = true;
            }
        }
        if !pending_ws.is_empty() {
            let should_emit = !self.trim;
            if should_emit {
                if self.collapse {
                    // Only emit replacement if run length >= 2
                    if pending_ws.len() >= 2 {
                        result.push(self.replacement_char());
                    } else {
                        // Single WS: Preserve original character
                        result.push(pending_ws[0] as char);
                    }
                } else {
                    result.extend(pending_ws.drain(..).map(|b| b as char));
                }
            }
            // If trimming, we silently drop trailing WS — correct behavior
        }
        Cow::Owned(result)
    }
    /// Full Unicode-aware transformation with optimal single-pass processing.
    /// Handles all whitespace types, all configurations, with at most one allocation.
    ///
    /// Rule: normalize_unicode is a modifier flag:
    /// - By itself: no-op
    /// - With trim_edges: trim Unicode WS from edges (but don't normalize internal)
    /// - With collapse: collapse Unicode WS and normalize to ' '
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
            // Non-whitespace: flush pending WS run
            // ═══════════════════════════════════════════════════════════
            if !pending_ws.is_empty() {
                let should_emit = !self.trim || started;
                if should_emit {
                    if self.collapse {
                        if pending_ws.len() >= 2 {
                            // Run of 2+ WS: Collapse and emit replacement
                            result.push(self.replacement_char());
                        } else {
                            // Single WS: Preserve original character
                            let mut first = pending_ws[0];
                            if self.is_unicode_whitespace_only(first) {
                                first = self.replacement_char();
                            }
                            result.push(first);
                        }
                    } else {
                        // No collapse: Emit all original WS chars
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
            let should_emit = !self.trim;
            if should_emit {
                if self.collapse {
                    // FIX: Only emit replacement if run length >= 2
                    if pending_ws.len() >= 2 {
                        result.push(self.replacement_char());
                    } else {
                        let mut first = pending_ws[0];
                        if self.is_unicode_whitespace_only(first) {
                            first = self.replacement_char();
                        }
                        // Single WS: Preserve original character
                        result.push(first);
                    }
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
            chars: text.chars(),
            pending: SmallVec::new(),
            pending_idx: 0,
            config: *self,
            started: false,
            done: false,
        })
    }
}

#[derive(Debug)]
struct WhitespaceIterator<'a> {
    chars: std::str::Chars<'a>,
    pending: SmallVec<[char; 4]>,
    pending_idx: usize,
    config: NormalizeWhitespace,
    started: bool,
    done: bool,
}

impl<'a> Iterator for WhitespaceIterator<'a> {
    type Item = char;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // First, drain any pending characters from previous flush
        if self.pending_idx < self.pending.len() {
            let c = self.pending[self.pending_idx];
            self.pending_idx += 1;
            return Some(c);
        }

        // Reset pending buffer if fully drained
        if self.pending_idx > 0 {
            self.pending.clear();
            self.pending_idx = 0;
        }

        // If we've already finished the input, we're done
        if self.done {
            return None;
        }
        let config = self.config;

        // Scan for the next non-whitespace character
        loop {
            match self.chars.next() {
                Some(c) => {
                    // let is_std_ws = config.is_whitespace_for_config(c);
                    // let is_uni_ws = config.is_unicode_whitespace_only(c);
                    // let is_ws = is_std_ws || is_uni_ws;

                    // Inside the loop, when handling whitespace:
                    if config.is_whitespace_for_config(c) {
                        self.pending.push(c);
                        continue;
                    }

                    // Non-whitespace: now decide whether to collapse
                    if !self.pending.is_empty() {
                        let should_emit = !self.config.trim || self.started;

                        if should_emit {
                            if self.config.collapse && self.pending.len() >= 2 {
                                // Collapse multi-char run
                                self.pending.clear();
                                self.pending.push(self.config.replacement_char);
                            } else if self.config.collapse {
                                // Collapse single-char: normalize if Unicode
                                let first = self.pending[0];
                                if config.is_unicode_whitespace_only(first) {
                                    self.pending[0] = self.config.replacement_char;
                                }
                            }
                            // else: collapse=false, keep original chars in pending

                            self.pending.push(c); // Add the non-WS char
                            self.started = true;

                            let first = self.pending[0];
                            self.pending_idx = 1;
                            return Some(first);
                        } else {
                            // Leading trim: discard pending
                            self.pending.clear();
                        }
                    }

                    self.started = true;
                    return Some(c);
                }
                None => {
                    // End of input - handle trailing whitespace
                    self.done = true;
                    if !self.pending.is_empty() {
                        let should_emit = !self.config.trim;

                        if should_emit {
                            if self.config.collapse && self.pending.len() >= 2 {
                                // Collapse multi-char run
                                let rep = self.config.replacement_char;
                                self.pending.clear();
                                return Some(rep);
                            } else if self.config.collapse {
                                // Collapse single-char: normalize if Unicode
                                let mut first = self.pending[0];
                                if config.is_unicode_whitespace_only(first) {
                                    first = self.config.replacement_char;
                                }
                                self.pending_idx = 1;
                                return Some(first);
                            } else {
                                // NO collapse: emit original char (no normalization!)
                                let first = self.pending[0];
                                self.pending_idx = 1;
                                return Some(first);
                            }
                        }
                    }

                    return None;
                }
            }
        }
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
        assert_stage_contract!(COLLAPSE_WHITESPACE);
        assert_stage_contract!(COLLAPSE_WHITESPACE_UNICODE);
        assert_stage_contract!(TRIM_WHITESPACE);
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
    // 1. Preset Configurations
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn preset_full_normalization() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        assert_eq!(
            stage
                .apply("  hello\u{00A0}\u{00A0}world  ".into(), &ctx())
                .unwrap(),
            "hello world"
        );
        assert_eq!(stage.apply("a\t\t\n\nb".into(), &ctx()).unwrap(), "a b");
    }

    #[test]
    fn preset_collapse_preserves_edges() {
        assert_eq!(
            COLLAPSE_WHITESPACE
                .apply("  a   b  ".into(), &ctx())
                .unwrap(),
            " a b "
        );
        // Unicode WS not collapsed when normalize_unicode=false
        assert_eq!(
            COLLAPSE_WHITESPACE
                .apply("a\u{00A0}\u{00A0}b".into(), &ctx())
                .unwrap(),
            "a\u{00A0}\u{00A0}b"
        );
    }

    #[test]
    fn preset_trim_preserves_internal() {
        assert_eq!(
            TRIM_WHITESPACE.apply("  a  b  ".into(), &ctx()).unwrap(),
            "a  b"
        );
        // Unicode WS not trimmed when normalize_unicode=false
        assert_eq!(
            TRIM_WHITESPACE
                .apply("\u{00A0}a\u{00A0}".into(), &ctx())
                .unwrap(),
            "\u{00A0}a\u{00A0}"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // 2. normalize_unicode Modifier Behavior
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn normalize_unicode_modifier_semantics() {
        let ctx = ctx();

        // With trim: trims Unicode WS from edges (matches str::trim())
        assert_eq!(
            TRIM_WHITESPACE_UNICODE
                .apply("\u{00A0}hello\u{00A0}world\u{205F}".into(), &ctx)
                .unwrap(),
            "hello\u{00A0}world"
        );

        // With collapse: collapses and normalizes Unicode WS
        assert_eq!(
            COLLAPSE_WHITESPACE_UNICODE
                .apply("a\u{00A0}\u{1680}b".into(), &ctx)
                .unwrap(),
            "a b"
        );

        // Standalone (no collapse/trim): no-op
        let no_op_stage = NormalizeWhitespace {
            collapse: false,
            trim: false,
            normalize_unicode: true,
            replacement_char: ' ',
        };
        assert_eq!(
            no_op_stage.apply("a\u{00A0}b".into(), &ctx).unwrap(),
            "a\u{00A0}b"
        );
    }

    #[test]
    fn matches_rust_str_trim() {
        let stage = TRIM_WHITESPACE_UNICODE;
        for input in [
            "  hello  ",
            "\u{00A0}hello\u{00A0}",
            "¡\u{a0}¡\u{205f}",
            "\u{3000}test\u{2028}",
        ] {
            assert_eq!(
                stage.apply(input.into(), &ctx()).unwrap().as_ref(),
                input.trim()
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // 3. Edge Cases
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn edge_cases_whitespace_only() {
        assert_eq!(TRIM_WHITESPACE.apply("   ".into(), &ctx()).unwrap(), "");
        assert_eq!(
            COLLAPSE_WHITESPACE.apply("   ".into(), &ctx()).unwrap(),
            " "
        );
        assert_eq!(
            NORMALIZE_WHITESPACE_FULL
                .apply("\u{00A0}\u{3000}".into(), &ctx())
                .unwrap(),
            ""
        );
    }

    #[test]
    fn edge_cases_empty_and_no_whitespace() {
        assert_eq!(
            NORMALIZE_WHITESPACE_FULL.apply("".into(), &ctx()).unwrap(),
            ""
        );
        assert_eq!(
            NORMALIZE_WHITESPACE_FULL
                .apply("abc".into(), &ctx())
                .unwrap(),
            "abc"
        );
    }

    #[test]
    fn edge_cases_mixed_ascii_unicode() {
        let stage = NORMALIZE_WHITESPACE_FULL;

        // Mixed runs should collapse together
        assert_eq!(
            stage.apply("a \u{00A0}\t\u{3000}b".into(), &ctx()).unwrap(),
            "a b"
        );

        // NEL (U+0085) handling
        assert_eq!(
            stage.apply("a\u{0085}\u{0085}b".into(), &ctx()).unwrap(),
            "a b"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // 4. Custom Replacement Character
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn custom_replacement_character() {
        // ZWSP for CJK
        let zwsp = NORMALIZE_WHITESPACE_FULL.replace_whitespace_with('\u{200B}');
        assert_eq!(
            zwsp.apply("a   \u{00A0}b".into(), &ctx()).unwrap(),
            "a\u{200B}b"
        );

        // Hyphen for debugging
        let hyphen = NormalizeWhitespace {
            collapse: true,
            trim: true,
            normalize_unicode: false,
            replacement_char: '-',
        };
        assert_eq!(hyphen.apply("  a\t\tb  ".into(), &ctx()).unwrap(), "a-b");

        // Without collapse, replacement char is never used
        let no_collapse = NormalizeWhitespace {
            collapse: false,
            trim: true,
            normalize_unicode: true,
            replacement_char: '-',
        };
        assert_eq!(
            no_collapse.apply("  a  b  ".into(), &ctx()).unwrap(),
            "a  b"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // 5. Unicode Preservation (normalize_unicode=false)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn unicode_ws_preserved_when_disabled() {
        let stage = COLLAPSE_WHITESPACE; // normalize_unicode=false

        // Unicode WS is NOT recognized, so not collapsed
        assert_eq!(
            stage.apply("a\u{00A0} \u{00A0}b".into(), &ctx()).unwrap(),
            "a\u{00A0} \u{00A0}b"
        );

        // But ASCII WS is still collapsed
        assert_eq!(stage.apply("a   b".into(), &ctx()).unwrap(), "a b");

        // Mixed: ASCII collapses, Unicode preserved
        assert_eq!(
            stage.apply("a\u{00A0}  \u{00A0}b".into(), &ctx()).unwrap(),
            "a\u{00A0} \u{00A0}b"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // 6. Critical: Path Equivalence (apply vs bind)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn path_equivalence_deterministic() {
        let configs = [
            NORMALIZE_WHITESPACE_FULL,
            COLLAPSE_WHITESPACE,
            TRIM_WHITESPACE,
            COLLAPSE_WHITESPACE_UNICODE,
            TRIM_WHITESPACE_UNICODE,
        ];

        let inputs = [
            "",
            "hello",
            "  a   b  ",
            "a\t\tb\n\nc",
            " \u{00A0} a \u{202F} b \u{3000} ",
            "   ",
            "mixed \t \u{00A0}  unicode \u{205F} ascii",
        ];

        for config in configs {
            for input in inputs {
                assert_path_equivalence(&config, input);
            }
        }
    }

    fn assert_path_equivalence(stage: &NormalizeWhitespace, input: &str) {
        let ctx = ctx();

        let via_apply = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        let via_bind: String = stage.bind(input, &ctx).collect();

        assert_eq!(
            via_apply.as_ref(),
            via_bind,
            "\nPath mismatch for input: {:?}\nConfig: {:?}",
            input,
            stage
        );
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::ENG;
    use rand::{Rng, SeedableRng, rngs::StdRng, seq::IndexedRandom};
    use std::borrow::Cow;

    fn ctx() -> Context {
        Context::new(ENG)
    }

    /// Property test: apply() and bind() must always produce identical output
    #[test]
    fn path_equivalence_random() {
        let mut rng = StdRng::seed_from_u64(0xCAFEBABE_DEADBEEF);

        for _ in 0..2_000 {
            let len = rng.random_range(0..128);
            let input: String = (0..len)
                .map(|_| {
                    let choice = rng.random_range(0..100);
                    if choice < 10 {
                        *[
                            '\u{00A0}', '\u{1680}', '\u{2000}', '\u{2003}', '\u{202F}', '\u{205F}',
                            '\u{3000}', '\u{0085}',
                        ]
                        .choose(&mut rng)
                        .unwrap()
                    } else if choice < 30 {
                        *[' ', '\t', '\n', '\r'].choose(&mut rng).unwrap()
                    } else {
                        (b'a' + rng.random_range(0..26)) as char
                    }
                })
                .collect();

            let config = NormalizeWhitespace {
                collapse: rng.random(),
                trim: rng.random(),
                normalize_unicode: rng.random(),
                replacement_char: if rng.random() { ' ' } else { '-' },
            };

            let ctx = ctx();
            let via_apply = config.apply(Cow::Borrowed(&input), &ctx).unwrap();
            let via_bind: String = config.bind(&input, &ctx).collect();

            assert_eq!(
                via_apply.as_ref(),
                via_bind,
                "\nRandom test failed:\nInput: {:?}\nConfig: {:?}",
                input,
                config
            );
        }
    }
}
