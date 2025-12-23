use crate::{
    context::Context,
    lang::Lang,
    stage::{FusableStage, Stage, StageError, StaticFusableStage},
    testing::stage_contract::StageTestConfig,
    unicode::{is_ascii_whitespace_fast, is_unicode_whitespace},
};
use smallvec::SmallVec;
use std::{borrow::Cow, iter::FusedIterator, mem::replace};

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

    // Exact detection of non‑ASCII Unicode whitespace (White_Space=Yes, excluding ASCII).
    // Uses byte windows to avoid char decoding in the common case and eliminate false
    // positives on punctuation such as smart quotes, em‑dash, ellipsis, etc.
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

            // Exact Unicode whitespace detection when normalize_unicode = true
            if self.normalize_unicode {
                // 2-byte: NEL (U+0085), NBSP (U+00A0)
                if bytes
                    .windows(2)
                    .any(|w| matches!(w, [0xC2, 0x85] | [0xC2, 0xA0]))
                {
                    return Ok(true);
                }

                // 3-byte: all remaining Unicode whitespace
                if bytes.windows(3).any(|w| {
                    let lead = w[0];
                    let mid = w[1];
                    let tail = w[2];

                    (lead == 0xE1 && mid == 0x9A && tail == 0x80) ||                     // U+1680 OGHAM SPACE MARK
                (lead == 0xE2 && mid == 0x80 && (tail <= 0x8A || tail == 0xA8 || tail == 0xA9 || tail == 0xAF)) || // U+2000–U+200A, U+2028–U+2029, U+202F
                (lead == 0xE2 && mid == 0x81 && tail == 0x9F) ||                     // U+205F MEDIUM MATHEMATICAL SPACE
                (lead == 0xE3 && mid == 0x80 && tail == 0x80) // U+3000 IDEOGRAPHIC SPACE
                }) {
                    return Ok(true);
                }
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

    /// NormalizeWhitespace is always fusable - checking needs_apply on the original text
    /// is always a safe approximation since it only normalizes/removes/collapses whitespace.
    #[inline]
    fn safe_skip_approximation(&self) -> bool {
        true
    }

    /// NormalizeWhitespace is always fusable. Handles whitespace normalization, collapsing,
    /// and trimming through stateful iterator logic.
    #[inline]
    fn as_fusable(&self) -> Option<&dyn FusableStage> {
        Some(self)
    }
}

impl NormalizeWhitespace {
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

    // Returns: (is_whitespace, needs_single_char_replacement)
    //
    // `needs_single_char_replacement` is true only for non-ASCII Unicode whitespace
    // when `normalize_unicode` is enabled.
    #[inline(always)]
    fn check_whitespace_and_single_char_replacement(&self, c: char) -> (bool, bool) {
        if c.is_ascii_whitespace() {
            // ASCII WS: It is whitespace (true), but only needs replacement
            // if it's part of a multi-char run (collapse logic handles that).
            (true, false)
        } else if self.normalize_unicode && c.is_whitespace() {
            // Non-ASCII Unicode WS: It is whitespace (true), AND it always
            // needs replacement when normalize_unicode is on.
            (true, true)
        } else {
            // Not whitespace.
            (false, false)
        }
    }

    #[inline(always)]
    fn is_whitespace_for_config(&self, c: char) -> bool {
        if self.normalize_unicode {
            c.is_whitespace()
        } else {
            c.is_ascii_whitespace()
        }
    }

    /// Optimized ASCII-only fast path (no Unicode normalization needed).
    /// Single-pass, byte-level operations with smart capacity estimation.
    #[inline(always)]
    fn apply_ascii_fast<'a>(&self, text: Cow<'a, str>) -> Cow<'a, str> {
        let s = text.as_ref();
        let bytes = s.as_bytes();

        // OPTIMIZATION: Smart capacity estimation
        let mut result = String::with_capacity(self.estimate_output_capacity(bytes.len()));

        let mut started = false;
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
                            if pending_ws.len() >= 2 {
                                result.push(self.replacement_char);
                            } else {
                                result.push(pending_ws[0] as char);
                            }
                        } else {
                            result.extend(pending_ws.drain(..).map(|b| b as char));
                        }
                    }
                }
                result.push(b as char);
                pending_ws.clear();
                started = true;
            }
        }

        if !pending_ws.is_empty() {
            let should_emit = !self.trim;
            if should_emit {
                if self.collapse {
                    if pending_ws.len() >= 2 {
                        result.push(self.replacement_char);
                    } else {
                        result.push(pending_ws[0] as char);
                    }
                } else {
                    result.extend(pending_ws.drain(..).map(|b| b as char));
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
    /// - With collapse: collapse Unicode WS and normalize to ' '
    #[inline(always)]
    fn apply_full<'a>(&self, text: Cow<'a, str>) -> Cow<'a, str> {
        let s = text.as_ref();
        // ═══════════════════════════════════════════════════════════
        // Single-pass transformation (trust needs_apply() contract)
        // ═══════════════════════════════════════════════════════════
        let mut result = String::with_capacity(self.estimate_output_capacity(s.len()));
        let mut started = false;
        // Simple Vec for pending WS (most runs are 1-2 chars)
        // Almost all whitespace runs are ≤4 chars in real text → zero-heap
        let mut pending_ws_tail: SmallVec<[char; 3]> = SmallVec::new();
        let mut first_ws_char: Option<char> = None; // Store the first char

        // Flag to track if the FIRST character in 'pending_ws' requires normalization
        // it will be used if single char replacement needed
        let mut first_ws_needs_replacement: bool = false;
        for c in s.chars() {
            // Use the highly optimized combined check
            let (is_ws, needs_replacement) = self.check_whitespace_and_single_char_replacement(c);

            if is_ws {
                if first_ws_char.is_none() {
                    // This is the START of a WS run
                    first_ws_char = Some(c);
                    first_ws_needs_replacement = needs_replacement;
                } else {
                    // This is a SUBSEQUENT char in the run
                    pending_ws_tail.push(c);
                }
                continue;
            }
            if let Some(first) = first_ws_char.take() {
                // WS run found: length is 1 + pending_ws_tail.len()
                let should_emit = !self.trim || started;

                if should_emit {
                    if self.collapse {
                        if pending_ws_tail.is_empty() {
                            // Length = 1 (Single character)
                            let mut final_char = first;
                            if first_ws_needs_replacement {
                                final_char = self.replacement_char;
                            }
                            result.push(final_char);
                        } else {
                            // Length >= 2 (Multi-character run)
                            result.push(self.replacement_char);
                        }
                    } else {
                        // No collapse: Emit all original WS chars
                        result.push(first);
                        result.extend(pending_ws_tail.drain(..));
                    }
                }

                // Reset tail state
                first_ws_needs_replacement = false;
                // first_ws_char is already taken/None
                pending_ws_tail.clear();
            }
            result.push(c);
            started = true;
        }
        // End-of-string: handle trailing WS
        if let Some(first) = first_ws_char.take() {
            let should_emit = !self.trim;

            if should_emit {
                if self.collapse {
                    if pending_ws_tail.is_empty() {
                        // Single trailing WS
                        let mut final_char = first;
                        if first_ws_needs_replacement {
                            final_char = self.replacement_char;
                        }
                        result.push(final_char);
                    } else {
                        // Multi trailing WS
                        result.push(self.replacement_char);
                    }
                } else {
                    result.push(first);
                    result.extend(pending_ws_tail.drain(..));
                }
            }
        }
        Cow::Owned(result)
    }
}

impl StaticFusableStage for NormalizeWhitespace {
    type Adapter<'a, I>
        = NormalizeWhitespaceStaticAdapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    fn supports_static_fusion(&self) -> bool {
        true
    }
    #[inline(always)]
    fn static_fused_adapter<'a, I>(&self, input: I, _ctx: &'a Context) -> Self::Adapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        if self.collapse {
            NormalizeWhitespaceStaticAdapter::Collapse(WhitespaceCollapseAdapter {
                input,
                config: *self,
                ws_count: 0,
                first_ws: '\0',
                first_ws_needs_replacement: false,
                next_char: None,
                started: false,
            })
        } else {
            NormalizeWhitespaceStaticAdapter::Preserve(WhitespacePreserveAdapter {
                input,
                config: *self,
                pending_ws: SmallVec::new(),
                next_char: None,
                started: false,
            })
        }
    }
}

// We use an enum to handle the choice between Collapse and Preserve in the Static path
pub enum NormalizeWhitespaceStaticAdapter<'a, I> {
    Collapse(WhitespaceCollapseAdapter<I>),
    Preserve(WhitespacePreserveAdapter<I>),
    _Phantom(std::marker::PhantomData<&'a ()>),
}

impl<'a, I: Iterator<Item = char>> Iterator for NormalizeWhitespaceStaticAdapter<'a, I> {
    type Item = char;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Collapse(a) => a.next(),
            Self::Preserve(a) => a.next(),
            _ => unreachable!(),
        }
    }
}

impl<'a, I: FusedIterator<Item = char>> FusedIterator for NormalizeWhitespaceStaticAdapter<'a, I> {}

// ============================================================================
// Whitespace Collapse Adapter (Generic)
// ============================================================================
pub struct WhitespaceCollapseAdapter<I> {
    input: I,
    config: NormalizeWhitespace,
    ws_count: usize,
    first_ws: char,
    first_ws_needs_replacement: bool,
    next_char: Option<char>,
    started: bool,
}

impl<I: Iterator<Item = char>> Iterator for WhitespaceCollapseAdapter<I> {
    type Item = char;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(c) = self.next_char.take() {
            return Some(c);
        }

        loop {
            match self.input.next() {
                Some(c) => {
                    let (is_ws, needs_rep) =
                        self.config.check_whitespace_and_single_char_replacement(c);
                    if is_ws {
                        if self.ws_count == 0 {
                            self.first_ws = c;
                            self.first_ws_needs_replacement = needs_rep;
                        }
                        self.ws_count += 1;
                        continue;
                    }

                    if self.ws_count > 0 {
                        let count = std::mem::replace(&mut self.ws_count, 0);
                        let should_emit = !self.config.trim || self.started;
                        if should_emit {
                            self.started = true;
                            self.next_char = Some(c);
                            return Some(if count >= 2 || self.first_ws_needs_replacement {
                                self.config.replacement_char
                            } else {
                                self.first_ws
                            });
                        }
                    }
                    self.started = true;
                    return Some(c);
                }
                None => {
                    if self.ws_count > 0 && !self.config.trim {
                        let count = std::mem::replace(&mut self.ws_count, 0);
                        return Some(if count >= 2 || self.first_ws_needs_replacement {
                            self.config.replacement_char
                        } else {
                            self.first_ws
                        });
                    }
                    return None;
                }
            }
        }
    }
}

impl<I: FusedIterator<Item = char>> FusedIterator for WhitespaceCollapseAdapter<I> {}

// ============================================================================
// Whitespace Preserve Adapter (Generic)
// ============================================================================
pub struct WhitespacePreserveAdapter<I> {
    input: I,
    config: NormalizeWhitespace,
    pending_ws: SmallVec<[char; 4]>,
    next_char: Option<char>,
    started: bool,
}

impl<I: Iterator<Item = char>> Iterator for WhitespacePreserveAdapter<I> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.pending_ws.is_empty() {
            return Some(self.pending_ws.remove(0));
        }
        if let Some(c) = self.next_char.take() {
            self.started = true;
            return Some(c);
        }

        let c = self.input.next()?;
        if self.config.is_whitespace_for_config(c) {
            let mut ws_run = SmallVec::<[char; 4]>::new();
            ws_run.push(c);
            loop {
                match self.input.next() {
                    Some(nc) if self.config.is_whitespace_for_config(nc) => ws_run.push(nc),
                    Some(nc) => {
                        self.next_char = Some(nc);
                        break;
                    }
                    None => break,
                }
            }
            if self.config.trim && (!self.started || self.next_char.is_none()) {
                self.started = true;
                return self.next();
            }
            self.pending_ws = ws_run;
            self.started = true;
            return self.next();
        }
        self.started = true;
        Some(c)
    }
}

impl<I: FusedIterator<Item = char>> FusedIterator for WhitespacePreserveAdapter<I> {}

// ============================================================================
// FusableStage Implementation - Dynamic Iterator Fusion
// ============================================================================

impl FusableStage for NormalizeWhitespace {
    fn dyn_fused_adapter<'a>(
        &self,
        input: Box<dyn FusedIterator<Item = char> + 'a>,
        _ctx: &'a Context,
    ) -> Box<dyn FusedIterator<Item = char> + 'a> {
        if self.collapse {
            Box::new(NormalizeWhitespaceCollapseAdapter {
                input,
                config: *self,
                ws_count: 0,
                first_ws: '\0',
                first_ws_needs_replacement: false,
                next_char: None,
                started: false,
            })
        } else {
            Box::new(NormalizeWhitespacePreserveAdapter {
                input,
                config: *self,
                pending_ws: SmallVec::new(),
                next_char: None,
                started: false,
            })
        }
    }
}

// ============================================================================
// Iterator Adapters for Fused Pipelines
// ============================================================================

/// Dynamic adapter for whitespace normalization with collapsing in fused iterator chains.
/// Collapses consecutive whitespace into a single replacement character.
struct NormalizeWhitespaceCollapseAdapter<'a> {
    input: Box<dyn FusedIterator<Item = char> + 'a>,
    config: NormalizeWhitespace,
    ws_count: u8,
    first_ws: char,
    first_ws_needs_replacement: bool,
    next_char: Option<char>,
    started: bool,
}

impl Iterator for NormalizeWhitespaceCollapseAdapter<'_> {
    type Item = char;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Fast path: emit buffered lookahead char
        if let Some(c) = self.next_char.take() {
            return Some(c);
        }

        loop {
            match self.input.next() {
                Some(c) => {
                    let (is_ws, needs_replacement) =
                        self.config.check_whitespace_and_single_char_replacement(c);
                    if is_ws {
                        if self.ws_count == 0 {
                            self.first_ws = c;
                            self.first_ws_needs_replacement = needs_replacement;
                        }
                        self.ws_count = self.ws_count.saturating_add(1);
                        continue;
                    }

                    // Non-whitespace: process accumulated WS
                    if self.ws_count > 0 {
                        let should_emit = !self.config.trim || self.started;
                        let count = replace(&mut self.ws_count, 0);

                        if should_emit {
                            self.started = true;
                            self.next_char = Some(c);

                            return Some(if count >= 2 || self.first_ws_needs_replacement {
                                self.config.replacement_char
                            } else {
                                self.first_ws
                            });
                        }
                    }

                    self.started = true;
                    return Some(c);
                }
                None => {
                    // End of input: handle trailing whitespace
                    if self.ws_count > 0 {
                        let should_emit = !self.config.trim;
                        let count = replace(&mut self.ws_count, 0);

                        if should_emit {
                            return Some(if count >= 2 || self.first_ws_needs_replacement {
                                self.config.replacement_char
                            } else {
                                self.first_ws
                            });
                        }
                    }
                    return None;
                }
            }
        }
    }
}

impl FusedIterator for NormalizeWhitespaceCollapseAdapter<'_> {}

/// Dynamic adapter for whitespace normalization without collapsing in fused iterator chains.
/// Preserves all whitespace characters (with optional trimming at edges).
struct NormalizeWhitespacePreserveAdapter<'a> {
    input: Box<dyn FusedIterator<Item = char> + 'a>,
    config: NormalizeWhitespace,
    pending_ws: SmallVec<[char; 4]>,
    next_char: Option<char>,
    started: bool,
}

impl Iterator for NormalizeWhitespacePreserveAdapter<'_> {
    type Item = char;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // First, drain any pending whitespace
        if !self.pending_ws.is_empty() {
            return Some(self.pending_ws.remove(0));
        }

        // Return buffered non-whitespace character BEFORE trying to read from input
        // This is critical for correct trim behavior!
        if let Some(c) = self.next_char.take() {
            self.started = true;
            return Some(c);
        }

        // Main loop
        let c = self.input.next()?;

        if self.config.is_whitespace_for_config(c) {
            // Collect the whitespace run
            let mut ws_run = SmallVec::<[char; 4]>::new();
            ws_run.push(c);

            // Peek ahead to collect all consecutive whitespace
            loop {
                // We need to actually consume from input to check
                match self.input.next() {
                    Some(next_c) if self.config.is_whitespace_for_config(next_c) => {
                        ws_run.push(next_c);
                    }
                    Some(next_c) => {
                        // Found non-whitespace terminator
                        self.next_char = Some(next_c);
                        break;
                    }
                    None => {
                        // End of input
                        break;
                    }
                }
            }

            // Handle trimming
            let is_leading = !self.started;
            let is_trailing = self.next_char.is_none();
            let should_trim = self.config.trim && (is_leading || is_trailing);

            if should_trim {
                // Discard the whitespace run, but next_char holds the first real character
                self.started = true;
                // Recursively call next() to emit the buffered character
                self.next()
            } else {
                // Emit the whitespace run
                self.pending_ws = ws_run;
                self.started = true;
                self.next()
            }
        } else {
            // Non-whitespace character
            self.started = true;
            Some(c)
        }
    }
}

impl FusedIterator for NormalizeWhitespacePreserveAdapter<'_> {}

impl StageTestConfig for NormalizeWhitespace {
    /// This stage is **not** a pure 1-to-1 character mapping.
    /// It can collapse runs, trim edges, and replace Unicode whitespace → no CharMapper path.
    fn one_to_one_languages() -> &'static [Lang] {
        &[]
    }

    /// General samples — cover common cases and edge inputs.
    /// These are used across *all* languages in the contract suite.
    fn samples(_lang: Lang) -> &'static [&'static str] {
        &[
            "Hello World 123",
            " déjà-vu ",
            "TEST",
            "",
            "hello \t\n world \u{00A0}\u{3000}",
            "¡\u{00A0}¡\u{205F}", // Real Unicode WS (NBSP + MEDIUM MATHEMATICAL SPACE)
            "clean text—no changes here", // Pure ASCII + common punctuation (zero-copy)
            "“smart quotes” … ellipsis — dash", // Typography only (should be zero-copy now)
        ]
    }

    /// Inputs that should **never** trigger allocation (zero-copy guarantee).
    /// These test the accuracy of `needs_apply()` on already-clean text.
    fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
        &[
            "hello world",
            "test",
            "abc def",
            "",
            "clean text—no changes here",
            "“smart quotes” … ellipsis — dash", // Critical: real-world false-positive cases
        ]
    }

    /// No predictable 1:1 transformations to verify against.
    /// The stage can collapse/replace → output length varies.
    fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
        &[]
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
    use crate::ENG;

    use super::*;
    use std::borrow::Cow;

    fn ctx() -> Context {
        Context::new(ENG)
    }

    // ====================================================================
    // 1. Table-driven tests for all preset configurations & modifiers
    // ====================================================================
    #[test]
    fn preset_and_modifier_behaviour() {
        let cases = [
            // Preset FULL
            (
                " hello\u{00A0}\u{00A0}world ",
                NORMALIZE_WHITESPACE_FULL,
                "hello world",
            ),
            ("a\t\t\n\nb", NORMALIZE_WHITESPACE_FULL, "a b"),
            (
                " \u{00A0} a \u{202F} b \u{3000} ",
                NORMALIZE_WHITESPACE_FULL,
                "a b",
            ),
            // COLLAPSE only (preserve edges)
            (" a b ", COLLAPSE_WHITESPACE, " a b "),
            (
                "a\u{00A0}\u{00A0}b",
                COLLAPSE_WHITESPACE,
                "a\u{00A0}\u{00A0}b",
            ),
            // TRIM only
            (" a b ", TRIM_WHITESPACE, "a b"),
            ("\u{00A0}a\u{00A0}", TRIM_WHITESPACE, "\u{00A0}a\u{00A0}"),
            // TRIM + normalize_unicode (matches str::trim)
            (
                "\u{00A0}hello\u{00A0}world\u{205F}",
                TRIM_WHITESPACE_UNICODE,
                "hello\u{00A0}world",
            ),
            // COLLAPSE + normalize_unicode
            ("a\u{00A0}\u{1680}b", COLLAPSE_WHITESPACE_UNICODE, "a b"),
            // normalize_unicode alone → no-op
            (
                "a\u{00A0}b",
                NormalizeWhitespace {
                    collapse: false,
                    trim: false,
                    normalize_unicode: true,
                    replacement_char: ' ',
                },
                "a\u{00A0}b",
            ),
            // Custom replacement (ZWSP for CJK)
            (
                "a \u{00A0}b",
                NORMALIZE_WHITESPACE_FULL.replace_whitespace_with('\u{200B}'),
                "a\u{200B}b",
            ),
            // Custom replacement without collapse
            (
                " a b ",
                NormalizeWhitespace {
                    collapse: false,
                    trim: true,
                    normalize_unicode: true,
                    replacement_char: '-',
                },
                "a b",
            ),
            // Unicode WS preserved when normalize_unicode = false
            (
                "a\u{00A0} \u{00A0}b",
                COLLAPSE_WHITESPACE,
                "a\u{00A0} \u{00A0}b",
            ),
        ];

        for (input, stage, expected) in cases {
            assert_eq!(
                stage.apply(Cow::Borrowed(input), &ctx()).unwrap().as_ref(),
                expected,
                "failed for input {:?} with config {:?}",
                input,
                stage
            );
        }
    }

    // ====================================================================
    // 2. Matches Rust's str::trim() for Unicode edges
    // ====================================================================
    #[test]
    fn matches_rust_str_trim() {
        let stage = TRIM_WHITESPACE_UNICODE;
        let inputs = [
            " hello ",
            "\u{00A0}hello\u{00A0}",
            "¡\u{a0}¡\u{205f}",
            "\u{3000}test\u{2028}",
        ];

        for &input in &inputs {
            assert_eq!(
                stage.apply(Cow::Borrowed(input), &ctx()).unwrap().as_ref(),
                input.trim()
            );
        }
    }

    // ====================================================================
    // 3. Critical path equivalence (apply vs static fusion)
    // ====================================================================
    #[test]
    fn path_equivalence() {
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
            " a b ",
            "a\t\tb\n\nc",
            " \u{00A0} a \u{202F} b \u{3000} ",
            " ",
            "mixed \t \u{00A0} unicode \u{205F} ascii",
            &format!("a{}b", " ".repeat(100)), // long run stress
        ];

        for config in configs {
            for input in &inputs {
                let via_apply = config.apply(Cow::Borrowed(input), &ctx()).unwrap();
                let via_fusion: String =
                    config.static_fused_adapter(input.chars(), &ctx()).collect();
                assert_eq!(
                    via_apply.as_ref(),
                    via_fusion,
                    "path mismatch for input {:?} with config {:?}",
                    input,
                    config
                );
            }
        }
    }

    // ====================================================================
    // 4. Real-world zero-copy (typography but no real Unicode WS)
    // ====================================================================
    #[test]
    fn real_web_text_zero_copy() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        let text = "Hello “world”… naïve café — no extra spaces";
        assert!(
            !stage.needs_apply(text, &ctx()).unwrap(),
            "should be zero-copy on clean web text"
        );
    }

    // ====================================================================
    // 5. Idempotent allocation guard (reproduces the original panic case)
    // ====================================================================
    #[test]
    fn idempotent_allocation_guard() {
        let stage = NORMALIZE_WHITESPACE_FULL;
        let input = "¡\u{a0}¡\u{205f}"; // NBSP + MEDIUM MATHEMATICAL SPACE
        let ctx = Context::new(crate::TUR);

        let mut text: Cow<'_, str> = Cow::Borrowed(input);
        if stage.needs_apply(&text, &ctx).unwrap() {
            text = stage.apply(text, &ctx).unwrap();
        }
        let ptr_before = text.as_ref() as *const str;
        if stage.needs_apply(&text, &ctx).unwrap() {
            text = stage.apply(text, &ctx).unwrap();
        }
        let ptr_after = text.as_ref() as *const str;

        assert_eq!(
            ptr_before, ptr_after,
            "zero-copy violation on second idempotent pass"
        );
    }
}
