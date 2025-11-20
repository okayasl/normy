use crate::{
    lang::{
        data::LANG_TABLE,
        entry::{CaseMap, DiacriticSet, FoldMap, Lang, LangEntry, SegmentRule},
    },
    unicode::{CharClass, classify, is_any_whitespace, is_same_script_cluster},
};

/// ---------------------------------------------------------------------------
/// 6. LocaleBehavior – zero-runtime-cost trait with helper methods
/// ---------------------------------------------------------------------------
pub trait LocaleBehavior {
    fn entry(&self) -> &'static LangEntry;
    // Core accessors
    fn id(&self) -> Lang;
    fn case_map(&self) -> &'static [CaseMap];
    fn fold_map(&self) -> &'static [FoldMap];
    fn diacritics(&self) -> Option<DiacriticSet>;
    fn needs_segmentation(&self) -> bool;

    // -------------------------------------------------------------------------
    // Case folding helpers
    // -------------------------------------------------------------------------

    /// Does this character need case folding in this language?
    /// O(k) check on a tiny slice then fallback to Unicode lowercasing.
    #[inline(always)]
    fn needs_case_fold(&self, c: char) -> bool {
        if let Some(e) = LANG_TABLE.get(self.id().code())
            && e.fold_char_slice.contains(&c)
        {
            return true;
        }
        // Unicode-level change (this checks if lowercase differs)
        c.to_lowercase().next() != Some(c)
    }

    /// Fold a single character (1→1 only).
    /// Returns None if the mapping is multi-char.
    #[inline(always)]
    fn fold_char(&self, c: char) -> Option<char> {
        let fold_map = self.fold_map();

        if fold_map.is_empty() {
            #[cfg(feature = "ascii-fast")]
            if c.is_ascii() {
                return Some(c.to_ascii_lowercase());
            }
            return c.to_lowercase().next();
        }

        // Check if character has language-specific mapping
        match fold_map.iter().find(|m| m.from == c) {
            Some(mapping) => {
                // Found in fold_map - verify it's 1→1
                let mut chars = mapping.to.chars();
                let first = chars.next()?;

                if chars.next().is_some() {
                    // Multi-char: cannot use in CharMapper
                    None
                } else {
                    // Single char: safe to use
                    Some(first)
                }
            }
            None => {
                // Not in fold_map - use Unicode lowercase
                c.to_lowercase().next()
            }
        }
    }

    #[inline(always)]
    fn needs_lowercase(&self, c: char) -> bool {
        let case_map = self.case_map();

        // Check language-specific case_map first (O(k) where k is tiny, ~2 for Turkish)
        if case_map.iter().any(|m| m.from == c) {
            return true;
        }

        // Fallback to Unicode lowercase check
        c.to_lowercase().next() != Some(c)
    }

    // Add this after fold_char()

    /// Lowercase a single character (1→1 always, uses case_map).
    /// This is for the Lowercase stage, not FoldCase.
    #[inline(always)]
    fn lowercase_char(&self, c: char) -> char {
        let case_map = self.case_map();

        // Language-specific 1→1 (Turkish, etc.)
        if let Some(m) = case_map.iter().find(|m| m.from == c) {
            return m.to;
        }

        #[cfg(feature = "ascii-fast")]
        if c.is_ascii() {
            return c.to_ascii_lowercase();
        }

        // Unicode guarantees at least one char
        c.to_lowercase().next().unwrap_or(c)
    }

    /// Can this language use CharMapper (zero-copy path)?
    #[inline(always)]
    fn has_one_to_one_folds(&self) -> bool {
        LANG_TABLE
            .get(self.id().code())
            .map(|e| e.has_one_to_one_folds())
            .unwrap_or(true)
    }

    /// Does this language need context-sensitive folding (peek-ahead)?
    #[inline(always)]
    fn requires_peek_ahead(&self) -> bool {
        LANG_TABLE
            .get(self.id().code())
            .map(|e| e.requires_peek_ahead)
            .unwrap_or(false)
    }

    // -------------------------------------------------------------------------
    // Context-sensitive folding helpers
    // -------------------------------------------------------------------------

    /// Check if a two-character sequence needs special handling.
    /// Returns the target string if this is a context-sensitive fold.
    #[inline]
    fn peek_ahead_fold(&self, current: char, next: Option<char>) -> Option<&'static str> {
        // 1. Early-out for languages that never need peek-ahead
        if !self.requires_peek_ahead() {
            return None;
        }

        let next_char = next?;

        // --------------------------------------------------------------------
        // 2. Explicit peek-pairs (language-defined)
        // --------------------------------------------------------------------
        if let Some(entry) = LANG_TABLE.get(self.id().code()) {
            for p in entry.peek_pairs {
                // *** CASE-SENSITIVE MATCH ***
                if p.a == current && p.b == next_char {
                    return Some(p.to);
                }
            }
        }

        // --------------------------------------------------------------------
        // 3. Fallback heuristic – only for *single-char* expansions that
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

    // -------------------------------------------------------------------------
    // Diacritic helpers
    // -------------------------------------------------------------------------
    #[inline(always)]
    fn has_diacritics(&self) -> bool {
        self.diacritics().is_some()
    }

    #[inline(always)]
    fn is_diacritic(&self, c: char) -> bool {
        LANG_TABLE
            .get(self.id().code())
            .and_then(|e| e.diacritic_slice)
            .map(|slice| slice.contains(&c))
            .unwrap_or(false)
    }

    #[inline]
    fn contains_diacritics(&self, text: &str) -> bool {
        if !self.has_diacritics() {
            return false;
        }
        text.chars().any(|c| self.is_diacritic(c))
    }

    // -------------------------------------------------------------------------
    // General helpers
    // -------------------------------------------------------------------------
    #[inline]
    fn needs_trim(&self, text: &str) -> bool {
        text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace)
    }

    #[inline]
    fn count_foldable_chars(&self, text: &str) -> usize {
        text.chars().filter(|&c| self.needs_case_fold(c)).count()
    }

    /// Count foldable characters and *exact* extra bytes needed.
    /// Returns `(count, extra_bytes)`
    #[inline]
    fn count_foldable_bytes(&self, text: &str) -> (usize, usize) {
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

    #[inline]
    fn count_diacritics(&self, text: &str) -> usize {
        if !self.has_diacritics() {
            return 0;
        }
        text.chars().filter(|&c| self.is_diacritic(c)).count()
    }

    #[inline(always)]
    fn needs_unigram_cjk(&self) -> bool {
        LANG_TABLE
            .get(self.id().code)
            .map(|e| e.unigram_cjk)
            .unwrap_or(false)
    }

    /// Returns the compile-time segmentation rules for this language.
    /// Empty slice = no special rules (fast path).
    #[inline(always)]
    fn segment_rules(&self) -> &'static [SegmentRule] {
        LANG_TABLE
            .get(self.id().code())
            .map(|e| e.segment_rules)
            .unwrap_or(&[])
    }

    /// Convenience: does this language need word segmentation at all?
    #[inline(always)]
    fn needs_word_segmentation(&self) -> bool {
        self.needs_segmentation() && !self.segment_rules().is_empty()
    }

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
    fn needs_boundary_between(&self, prev: char, curr: char) -> bool {
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
                CharClass::CJK | CharClass::Hangul | CharClass::SEAsian | CharClass::NonCJKScript,
            ) => self.segment_rules().contains(&SegmentRule::WesternToScript),

            // Script → Western
            (
                CharClass::CJK | CharClass::Hangul | CharClass::SEAsian | CharClass::NonCJKScript,
                CharClass::Western,
            ) => self.segment_rules().contains(&SegmentRule::ScriptToWestern),

            // Cross-script (CJK → Hangul/SEAsian/NonCJKScript etc.)
            (pc, cc) if pc != cc => true,

            // Everything else: no boundary
            _ => false,
        }
    }
}


impl LocaleBehavior for Lang {
    #[inline(always)]
    fn entry(&self) -> &'static LangEntry {
        // SAFETY: LANG_TABLE is perfect hash + 'static lifetime
        // This is the only safe place we ever do the lookup
        LANG_TABLE.get(self.code).expect("invalid language code")
    }
}

// impl LocaleBehavior for Lang {
//     #[inline(always)]
//     fn id(&self) -> Lang {
//         *self
//     }

//     #[inline(always)]
//     fn case_map(&self) -> &'static [CaseMap] {
//         LANG_TABLE.get(self.code).map(|e| e.case_map).unwrap_or(&[])
//     }

//     #[inline(always)]
//     fn fold_map(&self) -> &'static [FoldMap] {
//         LANG_TABLE.get(self.code).map(|e| e.fold_map).unwrap_or(&[])
//     }

//     #[inline(always)]
//     fn diacritics(&self) -> Option<DiacriticSet> {
//         LANG_TABLE.get(self.code).and_then(|e| e.diacritics)
//     }

//     #[inline(always)]
//     fn needs_segmentation(&self) -> bool {
//         LANG_TABLE
//             .get(self.code)
//             .map(|e| e.needs_segmentation)
//             .unwrap_or(false)
//     }

//     #[inline(always)]
//     fn segment_rules(&self) -> &'static [SegmentRule] {
//         LANG_TABLE
//             .get(self.code)
//             .map(|e| e.segment_rules)
//             .unwrap_or(&[])
//     }

//     #[inline(always)]
//     fn needs_word_segmentation(&self) -> bool {
//         self.needs_segmentation() && !self.segment_rules().is_empty()
//     }

//     #[inline(always)]
//     fn needs_unigram_cjk(&self) -> bool {
//         LANG_TABLE
//             .get(self.code)
//             .map(|e| e.unigram_cjk)
//             .unwrap_or(false)
//     }
// }
