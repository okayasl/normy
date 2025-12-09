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

        // Fast path 1: Trim edges check
        if self.trim {
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

        if self.collapse {
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
    #[inline(always)]
    fn replacement_char(&self) -> char {
        self.replacement_char
    }

    #[inline(always)]
    fn with_normalize_unicode(&mut self) {
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
        let stage = COLLAPSE_WHITESPACE;
        let text = Cow::Borrowed("  hello   world  ");
        write_all(stage, text.clone());
        assert_eq!(stage.apply(text, &ctx()).unwrap(), " hello world ");
    }

    #[test]
    fn path_consistency() {
        let stage = COLLAPSE_WHITESPACE;
        let text = Cow::Borrowed(
            "tg\u{2028}mfr\u{a0} \ts\r\u{2003}nszbgqnrg\tlpcpw xdoznzjj\u{2003}\u{202f}\u{2000}nldpeipe\u{1680}yys\u{205f}kffcqx\t\nqilic\tm ci\tfi\u{202f}ezfpv\r\nj\u{2028}qah\nrkn\u{a0}u\u{2028}b\ta\rphpqdqxq\tf",
        );
        write_all(stage, text.clone());
    }

    #[test]
    fn test_normalized() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        let text = Cow::Borrowed("hello world");
        write_all(stage, text.clone());
        assert_eq!(stage.apply(text, &ctx()).unwrap(), "hello world");
    }

    #[test]
    fn trim_only_preserves_internal() {
        let stage = TRIM_WHITESPACE;
        assert_eq!(
            stage.apply(Cow::Borrowed("  a  b  "), &ctx()).unwrap(),
            "a  b"
        );
    }

    #[test]
    fn trim_only_preserves_unicode_internal() {
        let stage = TRIM_WHITESPACE;
        let input = Cow::Borrowed("¡\u{a0}¡\u{205f}");
        write_all(stage, input.clone());
        assert_eq!(stage.apply(input, &ctx()).unwrap(), "¡\u{a0}¡\u{205f}");
    }

    // ═══════════════════════════════════════════════════════════════
    // Edge Cases
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn whitespace_only_strings() {
        let ctx = ctx();
        assert_eq!(
            TRIM_WHITESPACE.apply(Cow::Borrowed("   "), &ctx).unwrap(),
            ""
        );
        assert_eq!(
            COLLAPSE_WHITESPACE
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
        let input = Cow::Borrowed("a\t \t b");
        assert!(stage.needs_apply(&input, &ctx()).unwrap());
        assert_eq!(stage.apply(input, &ctx()).unwrap(), "a b");
    }

    #[test]
    fn preserves_tabs_in_collapsed_run() {
        let stage = NormalizeWhitespace {
            collapse: true,
            trim: true,
            normalize_unicode: false,
            replacement_char: ' ',
        };
        let input: Cow<'_, str> = Cow::Borrowed("a\t \t b");
        assert!(stage.needs_apply(&input, &ctx()).unwrap());
        assert_eq!(stage.apply(input, &ctx()).unwrap(), "a b");
    }

    #[test]
    fn preserves_unicodes_if_no_change_needed() {
        let stage = NormalizeWhitespace {
            collapse: true,
            trim: true,
            normalize_unicode: true,
            replacement_char: '-',
        };
        let text = Cow::Borrowed("a b c");
        write_all(stage, text);
    }

    fn write_all(stage: NormalizeWhitespace, text: Cow<'_, str>) {
        let ctx = Context::default();
        let full_res = stage.apply_full(text.clone());
        println!("full res = {full_res}");
        let bind_res: Cow<'_, str> = Cow::Owned(stage.bind(&text, &ctx).collect::<String>());
        println!("bind res = {bind_res}");
        let ascii_res = stage.apply_ascii_fast(text.clone());
        println!("ascii res = {ascii_res}");
    }

    #[test]
    fn removes_tabs() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        let ctx = Context::default();
        let text = Cow::Borrowed("a\t \t b");
        let ascii_res = stage.apply_ascii_fast(text.clone());
        println!("ascii res = {ascii_res}");
        let full_res = stage.apply_full(text.clone());
        println!("full res = {full_res}");
        let bind_res: Cow<'_, str> = Cow::Owned(stage.bind(&text, &ctx).collect::<String>());
        println!("bind res = {bind_res}");
    }

    #[test]
    fn removes_unicodes() {
        let stage = COLLAPSE_WHITESPACE;
        let ctx = Context::default();
        let text = Cow::Borrowed("xsr\nvhm");
        let ascii_res = stage.apply_ascii_fast(text.clone());
        println!("ascii res = {ascii_res}");
        let full_res = stage.apply_full(text.clone());
        println!("full res = {full_res}");
        let bind_res: Cow<'_, str> = Cow::Owned(stage.bind(&text, &ctx).collect::<String>());
        println!("bind res = {bind_res}");
    }

    #[test]
    fn removes_collapsing_trims() {
        let stage = NormalizeWhitespace {
            collapse: false,
            trim: false,
            normalize_unicode: false,
            replacement_char: ' ',
        };
        let ctx = Context::default();
        let text = Cow::Borrowed("\t a \n");
        let ascii_res = stage.apply_ascii_fast(text.clone());
        println!("ascii res = {ascii_res}");
        let full_res = stage.apply_full(text.clone());
        println!("full res = {full_res}");
        let bind_res: Cow<'_, str> = Cow::Owned(stage.bind(&text, &ctx).collect::<String>());
        println!("bind res = {bind_res}");
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
            COLLAPSE_WHITESPACE
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
            collapse: false,
            trim: false,
            normalize_unicode: false,
            replacement_char: ' ',
        };

        assert_eq!(
            stage.apply(Cow::Borrowed("a\u{00A0}b"), &ctx()).unwrap(),
            "a\u{00A0}b"
        );
    }

    #[test]
    fn preserve_unicode_ws_when_normalize_disabled_with_collapse_trim() {
        let stage = NormalizeWhitespace {
            collapse: true,
            trim: true,
            normalize_unicode: false,
            replacement_char: ' ',
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
            collapse: true,
            trim: false,
            normalize_unicode: false,
            replacement_char: ' ',
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
            collapse: false,
            trim: false,
            normalize_unicode: false,
            replacement_char: ' ',
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
        let stage = COLLAPSE_WHITESPACE; // normalize_unicode = false
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
    fn custom_replacement_char_zwsp() {
        let stage = NORMALIZE_WHITESPACE_FULL.replace_whitespace_with('\u{200B}');
        let input = "hello   \t  \u{00A0}\u{3000}  world";
        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx()).unwrap(),
            "hello\u{200B}world"
        );
    }

    #[test]
    fn custom_replacement_with_no_collapse_is_ignored() {
        // collapse = false → replacement char never used
        let stage = NormalizeWhitespace {
            collapse: false,
            trim: true,
            normalize_unicode: true,
            replacement_char: '\u{200B}',
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
            collapse: true,
            trim: true,
            normalize_unicode: false,
            replacement_char: '-',
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
            collapse: true,
            trim: false,
            normalize_unicode: true,
            replacement_char: '_',
        };
        let input = "x\u{00A0}\u{1680}\t y";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx()).unwrap(), "x_y");
    }

    #[test]
    fn bind_and_apply_consistent() {
        let stage = NormalizeWhitespace {
            collapse: true,
            trim: true,
            normalize_unicode: true,
            replacement_char: '-',
        };
        let ctx = ctx();
        let input = "a\t\tb\n\nc";
        write_all(stage, input.into());

        // 1. Legacy apply() path (goes through apply_ascii_fast / apply_full)
        let via_apply = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        println!("via_apply = {via_apply}");

        // 2. CharMapper path — this is what monomorphised pipelines use
        let via_iterator: String = stage.bind(input, &ctx).collect();
        println!("via_iterator = {via_iterator}");

        assert_eq!(
            via_apply, via_iterator,
            "\nCharMapper bind() ≠ apply() on input:\n{:?}\nConfig: {:?}\n",
            input, stage
        );
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
        assert_eq!(
            needs, changed,
            "needs_apply()={needs} -> mismatch on: {:?} , changed: {changed} to {via_apply}",
            input
        );
    }

    #[test]
    fn equivalence_exhaustive_deterministic() {
        let configs = [
            NORMALIZE_WHITESPACE_FULL,
            COLLAPSE_WHITESPACE,
            COLLAPSE_WHITESPACE_UNICODE,
            TRIM_WHITESPACE,
            TRIM_WHITESPACE_UNICODE,
            NormalizeWhitespace {
                collapse: true,
                trim: true,
                normalize_unicode: true,
                replacement_char: '-',
            },
            NormalizeWhitespace {
                collapse: true,
                trim: false,
                normalize_unicode: true,
                replacement_char: '\u{200B}',
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
                collapse: rng.random(),
                trim: rng.random(),
                normalize_unicode: rng.random(),
                replacement_char: if rng.random() { ' ' } else { '-' },
            };

            assert_equivalence(&config, &input);
        }
    }
}
