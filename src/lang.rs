pub mod data;

use crate::{
    ENG,
    unicode::{CharClass, classify, is_any_whitespace, is_same_script_cluster},
};

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
    pub case_map: &'static [CaseMap],
    pub fold_map: &'static [FoldMap],
    pub diacritics: Option<DiacriticSet>,
    pub needs_segmentation: bool,
    pub requires_peek_ahead: bool,
    pub fold_char_slice: &'static [char],
    pub diacritic_slice: Option<&'static [char]>,
    pub peek_pairs: &'static [PeekPair],
    pub segment_rules: &'static [SegmentRule],
    pub unigram_cjk: bool,
}

impl LangEntry {
    #[inline(always)]
    pub fn has_one_to_one_folds(&self) -> bool {
        if self.fold_map.is_empty() {
            return true;
        }
        self.fold_map.iter().all(|m| m.to.chars().count() == 1)
    }

    // === Core accessors (all inlined to static field loads) ===
    #[inline(always)]
    pub fn case_map(&self) -> &'static [CaseMap] {
        self.case_map
    }

    #[inline(always)]
    pub fn fold_map(&self) -> &'static [FoldMap] {
        self.fold_map
    }

    #[inline(always)]
    pub fn diacritics(&self) -> Option<DiacriticSet> {
        self.diacritics
    }

    #[inline(always)]
    pub fn needs_segmentation(&self) -> bool {
        self.needs_segmentation
    }

    #[inline(always)]
    pub fn requires_peek_ahead(&self) -> bool {
        self.requires_peek_ahead
    }

    /// Check if a two-character sequence needs special handling.
    /// Returns the target string if this is a context-sensitive fold.
    #[inline]
    pub fn peek_ahead_fold(&self, current: char, next: Option<char>) -> Option<&'static str> {
        // Early-out for languages that never need peek-ahead
        if !self.requires_peek_ahead() {
            return None;
        }

        let next_char = next?;

        // --------------------------------------------------------------------
        // Explicit peek-pairs (language-defined)
        // --------------------------------------------------------------------
        for p in self.peek_pairs {
            if p.a == current && p.b == next_char {
                return Some(p.to);
            }
        }

        // --------------------------------------------------------------------
        // Fallback heuristic – only for *single-char* expansions that
        //     happen to be identical for the two adjacent chars.
        // --------------------------------------------------------------------
        // This branch is **never taken for Dutch** because Dutch has no
        // single-char entries that expand to "ij".  It stays here for
        // future-proofness (e.g. a hypothetical language where both 'X' and
        // 'Y' map to "xy").
        let fold_map = self.fold_map();
        let cur = fold_map.iter().find(|m| m.from == current)?;
        let nxt = fold_map.iter().find(|m| m.from == next_char)?;

        // The heuristic must be **case-sensitive** as well – we only
        // consider the *exact* mapping, not a lower-cased version.
        if cur.to == nxt.to && cur.to.chars().count() > 1 {
            Some(cur.to)
        } else {
            None
        }
    }

    // === Case folding helpers ===
    #[inline(always)]
    pub fn needs_case_fold(&self, c: char) -> bool {
        let entry = self;
        entry.fold_char_slice.contains(&c)
            || entry.case_map.iter().any(|m| m.from == c)
            || c.to_lowercase().next() != Some(c)
    }

    #[inline(always)]
    pub fn needs_lowercase(&self, c: char) -> bool {
        let case_map = self.case_map();

        // Check language-specific case_map first (O(k) where k is tiny, ~2 for Turkish)
        if case_map.iter().any(|m| m.from == c) {
            return true;
        }

        // Fallback to Unicode lowercase check
        c.to_lowercase().next() != Some(c)
    }

    #[inline(always)]
    pub fn fold_char(&self, c: char) -> Option<char> {
        let entry = self;
        if let Some(m) = entry.fold_map.iter().find(|m| m.from == c) {
            let mut chars = m.to.chars();
            let first = chars.next()?;
            if chars.next().is_none() {
                Some(first)
            } else {
                None
            }
        } else {
            c.to_lowercase().next()
        }
    }

    #[inline(always)]
    pub fn lowercase_char(&self, c: char) -> char {
        if let Some(m) = self.case_map().iter().find(|m| m.from == c) {
            m.to
        } else {
            #[cfg(feature = "ascii-fast")]
            if c.is_ascii() {
                return c.to_ascii_lowercase();
            }
            c.to_lowercase().next().unwrap_or(c)
        }
    }

    #[inline(always)]
    pub fn is_diacritic(&self, c: char) -> bool {
        self.diacritic_slice
            .map(|slice| slice.contains(&c))
            .unwrap_or(false)
    }

    // #[inline(always)]
    // fn is_diacritic(&self, c: char) -> bool {
    //     self
    //         .diacritic_slice
    //         .and_then(|s| s.binary_search(&c).ok())
    //         .is_some()
    // }

    #[inline(always)]
    pub fn has_diacritics(&self) -> bool {
        self.diacritics().is_some()
    }

    /// Count foldable characters and *exact* extra bytes needed.
    /// Returns `(count, extra_bytes)`
    #[inline]
    pub fn count_foldable_bytes(&self, text: &str) -> (usize, usize) {
        let fold_map = self.fold_map();
        if fold_map.is_empty() {
            return (0, 0);
        }

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

    #[inline(always)]
    pub fn needs_unigram_cjk(&self) -> bool {
        self.unigram_cjk
    }

    #[inline(always)]
    pub fn segment_rules(&self) -> &'static [SegmentRule] {
        self.segment_rules
    }

    // -------------------------------------------------------------------------
    // General helpers
    // -------------------------------------------------------------------------
    // #[inline]
    // fn needs_trim(&self, text: &str) -> bool {
    //     text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace)
    // }

    // #[inline]
    // fn count_foldable_chars(&self, text: &str) -> usize {
    //     text.chars().filter(|&c| self.needs_case_fold(c)).count()
    // }

    // /// Convenience: does this language need word segmentation at all?
    // #[inline(always)]
    // fn needs_word_segmentation(&self) -> bool {
    //     self.needs_segmentation() && !self.segment_rules().is_empty()
    // }

    // #[inline]
    // fn count_diacritics(&self, text: &str) -> usize {
    //     if !self.has_diacritics() {
    //         return 0;
    //     }
    //     text.chars().filter(|&c| self.is_diacritic(c)).count()
    // }

    // #[inline]
    // fn contains_diacritics(&self, text: &str) -> bool {
    //     if !self.has_diacritics() {
    //         return false;
    //     }
    //     text.chars().any(|c| self.is_diacritic(c))
    // }

    /// Determine whether a boundary (space) should be inserted between two characters.
    /// Returns `true` if a segmentation boundary is required, `false` otherwise.
    ///
    /// Rules:
    /// 1. Whitespace never produces boundaries.
    /// 2. Characters in the same script cluster (Western, CJK, Hangul, SE-Asian) do not produce boundaries.
    /// 3. Consecutive CJK ideographs do not produce boundaries.
    /// 4. Western → Script or Script → Western boundaries follow the language's segment rules.
    /// 5. Cross-script transitions produce a boundary.
    #[inline(always)]
    pub fn needs_boundary_between(&self, prev: char, curr: char) -> bool {
        // --- 1. Whitespace never produces boundaries ---
        if is_any_whitespace(prev) || is_any_whitespace(curr) {
            return false;
        }

        // --- 2. Same script cluster: no boundary ---
        if is_same_script_cluster(prev, curr) {
            return false;
        }

        // --- 3. Cross-cluster transitions ---
        let prev_class = classify(prev);
        let curr_class = classify(curr);

        match (prev_class, curr_class) {
            // Western → Script (CJK/Hangul/SEAsian/NonCJKScript)
            (
                CharClass::Western,
                CharClass::Cjk | CharClass::Hangul | CharClass::SEAsian | CharClass::NonCJKScript,
            ) => self.segment_rules().contains(&SegmentRule::WesternToScript),

            // Script → Western
            (
                CharClass::Cjk | CharClass::Hangul | CharClass::SEAsian | CharClass::NonCJKScript,
                CharClass::Western,
            ) => self.segment_rules().contains(&SegmentRule::ScriptToWestern),

            // Cross-script (CJK → Hangul/SEAsian/NonCJKScript etc.)
            (pc, cc) if pc != cc => true,

            // Everything else: no boundary
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lang::{
        Lang,
        data::{JPN, KOR, LANG_TABLE, THA, ZHO},
    };

    // Small helper for iterating character pairs
    fn assert_boundaries(lang: &Lang, pairs: &[(&str, &str)], expected: bool) {
        for &(a, b) in pairs {
            let chars: Vec<char> = a.chars().collect();
            let chars2: Vec<char> = b.chars().collect();
            let lang_entry = LANG_TABLE
                .get(lang.code())
                .copied()
                .expect("language not present in LANG_TABLE – this is a bug");
            assert_eq!(
                lang_entry.needs_boundary_between(chars[0], chars2[0]),
                expected,
                "Failed: {} -> {} for {}",
                a,
                b,
                std::any::type_name::<Lang>()
            );
        }
    }

    #[test]
    fn test_whitespace_no_boundary() {
        let whitespace_pairs = &[(" ", "あ"), ("あ", " "), ("\n", "A"), ("A", "\t")];
        assert_boundaries(&JPN, whitespace_pairs, false);
    }

    #[test]
    fn test_western_script_breaks() {
        let pairs = &[
            ("A", "あ"),
            ("あ", "A"),
            ("A", "中"),
            ("文", "A"),
            ("A", "\u{AC00}"), // Hangul
            ("\u{AC00}", "A"),
        ];
        assert_boundaries(&JPN, &pairs[0..2], true);
        assert_boundaries(&ZHO, &pairs[2..4], true);
        assert_boundaries(&KOR, &pairs[4..6], true);
    }

    #[test]
    fn test_same_cluster_no_break() {
        let japanese = &[("あ", "ア")];
        let hangul = &[("\u{AC00}", "\u{AC01}")];
        let thai = &[("\u{0E01}", "\u{0E02}")];

        assert_boundaries(&JPN, japanese, false);
        assert_boundaries(&KOR, hangul, false);
        assert_boundaries(&THA, thai, false);
    }

    #[test]
    fn test_punctuation_and_symbols() {
        let script_to_punct = &[
            ("日", ")"),
            ("文", "."),
            ("\u{0E01}", ","),
            ("\u{AC00}", "-"),
        ];
        let script_to_emoji = &[("あ", "😀"), ("😀", "あ"), ("A", "😃"), ("가", "🎉")];

        assert_boundaries(&JPN, &script_to_punct[0..2], true);
        assert_boundaries(&THA, &script_to_punct[2..3], true);
        assert_boundaries(&KOR, &script_to_punct[3..4], true);

        assert_boundaries(&JPN, &script_to_emoji[0..2], true);
        assert_boundaries(&ZHO, &script_to_emoji[2..3], true);
        assert_boundaries(&KOR, &script_to_emoji[3..4], true);
    }

    #[test]
    fn test_digits_break() {
        let pairs = &[("1", "あ"), ("あ", "1"), ("9", "中"), ("0", "\u{AC00}")];
        assert_boundaries(&JPN, &pairs[0..2], true);
        assert_boundaries(&ZHO, &pairs[2..3], true);
        assert_boundaries(&KOR, &pairs[3..4], true);
    }

    #[test]
    fn test_cross_script_clusters() {
        let pairs = &[
            ("A", "Я"),
            ("Z", "Ж"),
            ("あ", "\u{0E01}"),
            ("文", "\u{AC00}"),
        ];
        assert_boundaries(&JPN, &pairs[0..3], true);
        assert_boundaries(&KOR, &pairs[1..4], true);
    }

    #[test]
    fn test_edge_cjk_blocks() {
        // No break inside CJK blocks
        let no_break = &[("\u{2F00}", "\u{2F01}"), ("\u{2F00}", "\u{2F00}")];
        assert_boundaries(&JPN, no_break, false);

        // Break with CJK punctuation
        let break_pairs = &[("、", "あ"), ("日", "。")];
        assert_boundaries(&JPN, break_pairs, true);
    }

    #[test]
    fn test_western_and_digits() {
        let pairs = &[
            ("A", "B"), // Western → Western
            ("1", "2"), // Digit → Digit
            ("A", "1"), // Letter → Digit
            ("1", "A"), // Digit → Letter
        ];
        assert_boundaries(&JPN, &pairs[0..2], false); // Western→Western and digits: no break
        assert_boundaries(&JPN, &pairs[2..4], false); // Cross Western class: no break
    }

    #[test]
    fn test_ascii_to_cjk_and_back() {
        let pairs = &[
            ("H", "世"), // Western → CJK
            ("o", "世"), // Western → CJK
            ("世", "H"), // CJK → Western
            ("文", "A"), // CJK → Western
        ];
        // Western -> CJK: MUST insert space (true)
        assert_boundaries(&JPN, &pairs[0..2], true);

        // CJK -> Western: MUST insert space (true)
        assert_boundaries(&JPN, &pairs[2..4], true); // <-- FIX: Change false to true
    }
}
