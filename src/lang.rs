pub mod data;

use crate::ENG;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lang {
    pub code: &'static str,
    pub name: &'static str,
}

impl Lang {
    #[inline(always)]
    pub const fn code(&self) -> &'static str {
        self.code
    }
    #[inline(always)]
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

pub const DEFAULT_LANG: Lang = ENG;

#[derive(Clone, Copy, Debug)]
pub struct CaseMap {
    pub from: char,
    pub to: char,
}

#[derive(Clone, Copy, Debug)]
pub struct FoldMap {
    pub from: char,
    pub to: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct StripMap {
    pub from: char,
    pub to: char,
}

pub type DiacriticSet = &'static [char];

#[derive(Clone, Copy, Debug)]
pub struct PeekPair {
    pub a: char,
    pub b: char,
    pub to: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentRule {
    WesternToScript,
    ScriptToWestern,
    CJKIdeographUnigram,
}

#[derive(Clone, Copy, Debug)]
pub struct LangEntry {
    // === Precomputed Boolean Flags (Hot Path - First Cache Line) ===
    pub has_case_map: bool,
    pub has_fold_map: bool,
    pub has_transliterate_map: bool,
    pub has_strip_map: bool,
    pub has_diacritics: bool,
    pub has_peek_pairs: bool,
    pub has_segment_rules: bool,

    // Derived properties
    pub has_one_to_one_folds: bool,
    pub has_one_to_one_transliterate: bool,

    // Already boolean from definition
    pub needs_segmentation: bool,
    pub requires_peek_ahead: bool,
    pub unigram_cjk: bool,

    // === Data Arrays (Second Cache Line+) ===
    pub code: &'static str,
    pub case_map: &'static [CaseMap],
    pub fold_map: &'static [FoldMap],
    pub transliterate_map: &'static [FoldMap],
    pub strip_map: &'static [StripMap],
    pub strip_char_slice: &'static [char],
    pub diacritics: Option<&'static [char]>,
    pub diacritic_slice: Option<&'static [char]>,
    pub fold_char_slice: &'static [char],
    pub transliterate_char_slice: &'static [char],
    pub peek_pairs: &'static [PeekPair],
    pub segment_rules: &'static [SegmentRule],
}

impl LangEntry {
    // ============================================================
    // CATEGORY 1: Boolean Queries - Pattern: has_*
    // ============================================================

    #[inline(always)]
    pub fn has_case_map(&self) -> bool {
        self.has_case_map
    }

    #[inline(always)]
    pub fn has_fold_map(&self) -> bool {
        self.has_fold_map
    }

    #[inline(always)]
    pub fn has_transliterate_map(&self) -> bool {
        self.has_transliterate_map
    }

    #[inline(always)]
    pub fn has_strip_map(&self) -> bool {
        self.has_strip_map
    }

    #[inline(always)]
    pub fn has_diacritics(&self) -> bool {
        self.has_diacritics
    }

    #[inline(always)]
    pub fn has_peek_pairs(&self) -> bool {
        self.has_peek_pairs
    }

    #[inline(always)]
    pub fn has_segment_rules(&self) -> bool {
        self.has_segment_rules
    }

    #[inline(always)]
    pub fn has_one_to_one_folds(&self) -> bool {
        self.has_one_to_one_folds
    }

    #[inline(always)]
    pub fn has_one_to_one_transliterate(&self) -> bool {
        self.has_one_to_one_transliterate
    }

    #[inline(always)]
    pub fn has_strip_or_diacritics(&self) -> bool {
        self.has_strip_map || self.has_diacritics
    }

    // Semantic queries (keep existing names)
    #[inline(always)]
    pub fn needs_segmentation(&self) -> bool {
        self.needs_segmentation
    }

    #[inline(always)]
    pub fn needs_unigram_cjk(&self) -> bool {
        self.unigram_cjk
    }

    #[inline(always)]
    pub fn requires_peek_ahead(&self) -> bool {
        self.requires_peek_ahead
    }

    // ============================================================
    // CATEGORY 2: Character Checks - Pattern: is_*
    // ============================================================

    #[inline(always)]
    pub fn is_diacritic(&self, c: char) -> bool {
        self.diacritic_slice
            .map(|slice| slice.contains(&c))
            .unwrap_or(false)
    }

    #[inline(always)]
    pub fn is_foldable(&self, c: char) -> bool {
        self.fold_char_slice.contains(&c)
    }

    #[inline(always)]
    pub fn is_transliterable(&self, c: char) -> bool {
        self.transliterate_char_slice.contains(&c)
    }

    #[inline(always)]
    pub fn is_strippable(&self, c: char) -> bool {
        self.strip_char_slice.contains(&c)
    }

    // ============================================================
    // CATEGORY 3: Text Analysis - Pattern: needs_*
    // ============================================================

    #[inline(always)]
    pub fn needs_case_fold(&self, c: char) -> bool {
        self.fold_char_slice.contains(&c)
            || self.case_map.iter().any(|m| m.from == c)
            || c.to_lowercase().next() != Some(c)
    }

    #[inline(always)]
    pub fn needs_lowercase(&self, c: char) -> bool {
        // Check language-specific case_map first
        if self.case_map.iter().any(|m| m.from == c) {
            return true;
        }
        // Fallback to Unicode lowercase check
        c.to_lowercase().next() != Some(c)
    }

    #[inline(always)]
    pub fn needs_diacritic_removal(&self, text: &str) -> bool {
        if self.has_strip_map {
            text.chars().any(|c| self.strip_char_slice.contains(&c))
        } else {
            self.diacritics
                .map(|diacs| text.chars().any(|c| diacs.contains(&c)))
                .unwrap_or(false)
        }
    }

    // ============================================================
    // CATEGORY 4: Data Accessors - Pattern: direct_name()
    // ============================================================

    #[inline(always)]
    pub fn case_map(&self) -> &'static [CaseMap] {
        self.case_map
    }

    #[inline(always)]
    pub fn fold_map(&self) -> &'static [FoldMap] {
        self.fold_map
    }

    #[inline(always)]
    pub fn transliterate_map(&self) -> &'static [FoldMap] {
        self.transliterate_map
    }

    #[inline(always)]
    pub fn strip_map(&self) -> &'static [StripMap] {
        self.strip_map
    }

    #[inline(always)]
    pub fn diacritics(&self) -> Option<DiacriticSet> {
        self.diacritics
    }

    #[inline(always)]
    pub fn peek_pairs(&self) -> &'static [PeekPair] {
        self.peek_pairs
    }

    #[inline(always)]
    pub fn segment_rules(&self) -> &'static [SegmentRule] {
        self.segment_rules
    }

    #[inline(always)]
    pub fn fold_char_slice(&self) -> &'static [char] {
        self.fold_char_slice
    }

    #[inline(always)]
    pub fn transliterate_char_slice(&self) -> &'static [char] {
        self.transliterate_char_slice
    }

    #[inline(always)]
    pub fn strip_char_slice(&self) -> &'static [char] {
        self.strip_char_slice
    }

    #[inline(always)]
    pub fn diacritic_slice(&self) -> Option<&'static [char]> {
        self.diacritic_slice
    }

    // ============================================================
    // CATEGORY 5: Transformations - Pattern: apply_* or get_*
    // ============================================================

    #[inline(always)]
    pub fn apply_case_fold(&self, c: char) -> Option<char> {
        if let Some(m) = self.fold_map.iter().find(|m| m.from == c) {
            let mut chars = m.to.chars();
            let first = chars.next()?;
            if chars.next().is_none() {
                Some(first)
            } else {
                None
            }
        } else if let Some(m) = self.case_map.iter().find(|m| m.from == c) {
            Some(m.to) // e.g., 'İ' → 'i' via case_map
        } else {
            c.to_lowercase().next()
        }
    }

    #[inline(always)]
    pub fn apply_lowercase(&self, c: char) -> char {
        if let Some(m) = self.case_map.iter().find(|m| m.from == c) {
            m.to
        } else {
            c.to_lowercase().next().unwrap_or(c)
        }
    }

    #[inline(always)]
    pub fn apply_strip(&self, c: char) -> Option<char> {
        self.strip_map
            .iter()
            .find(|&&StripMap { from, .. }| from == c)
            .map(|&StripMap { to, .. }| to)
    }

    /// Check if a two-character sequence needs special handling.
    /// Returns the target string if this is a context-sensitive fold.
    #[inline]
    pub fn get_peek_fold(&self, current: char, next: Option<char>) -> Option<&'static str> {
        // Early-out for languages that never need peek-ahead
        if !self.requires_peek_ahead {
            return None;
        }

        let next_char = next?;

        // Explicit peek-pairs (language-defined)
        for p in self.peek_pairs {
            if p.a == current && p.b == next_char {
                return Some(p.to);
            }
        }

        // Fallback heuristic – only for *single-char* expansions
        let cur = self.fold_map.iter().find(|m| m.from == current)?;
        let nxt = self.fold_map.iter().find(|m| m.from == next_char)?;

        if cur.to == nxt.to && cur.to.chars().count() > 1 {
            Some(cur.to)
        } else {
            None
        }
    }

    // ============================================================
    // CATEGORY 6: Capacity Hints - Pattern: hint_capacity_*
    // ============================================================

    /// Estimate output capacity for case folding with expansions.
    /// Returns (num_expansions, extra_bytes_needed).
    #[inline]
    pub fn hint_capacity_fold(&self, text: &str) -> (usize, usize) {
        if !self.has_fold_map {
            return (0, 0);
        }

        let fold_map = self.fold_map();

        // Fast path for one-to-one: count folding operations but no extra bytes
        if self.has_one_to_one_folds {
            let count = text
                .chars()
                .filter(|&c| fold_map.iter().any(|m| m.from == c))
                .count();
            return (count, 0); // ✓ Count changes, but 0 extra bytes
        }

        // Slow path: count folds AND calculate extra bytes
        let mut count = 0;
        let mut extra = 0;

        for c in text.chars() {
            if let Some(m) = fold_map.iter().find(|m| m.from == c) {
                count += 1;
                let from_len = c.len_utf8();
                let to_len = m.to.len();
                if to_len > from_len {
                    extra += to_len - from_len;
                }
            }
        }

        (count, extra)
    }

    /// Estimate output capacity for transliteration.
    /// Returns (num_transliterations, extra_bytes_needed).
    #[inline]
    pub fn hint_capacity_transliterate(&self, text: &str) -> (usize, usize) {
        if !self.has_transliterate_map {
            return (0, 0);
        }

        let map = self.transliterate_map();

        // Fast path for one-to-one: count transformations but no extra bytes needed
        if self.has_one_to_one_transliterate {
            let count = text
                .chars()
                .filter(|&c| map.iter().any(|m| m.from == c))
                .count();
            return (count, 0); // ✓ Count changes, but 0 extra bytes
        }

        // Slow path: count transformations AND calculate extra bytes
        let mut count = 0;
        let mut extra = 0;

        for c in text.chars() {
            if let Some(m) = map.iter().find(|m| m.from == c) {
                count += 1;
                let from_len = c.len_utf8();
                let to_len = m.to.len();
                if to_len > from_len {
                    extra += to_len - from_len;
                }
            }
        }

        (count, extra)
    }
}
