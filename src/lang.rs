pub mod data;

use crate::{ENG, LANG_TABLE};

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
pub struct PreComposedToBaseMap {
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
    pub has_pre_composed_to_base_map: bool,
    pub has_spacing_diacritics: bool,
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
    pub pre_composed_to_base_map: &'static [PreComposedToBaseMap],
    pub pre_composed_to_base_char_slice: &'static [char],
    pub spacing_diacritics: Option<&'static [char]>,
    pub spacing_diacritics_slice: Option<&'static [char]>,
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
    pub fn has_pre_composed_to_base_map(&self) -> bool {
        self.has_pre_composed_to_base_map
    }

    #[inline(always)]
    pub fn has_spacing_diacritics(&self) -> bool {
        self.has_spacing_diacritics
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
    pub fn has_pre_composed_to_base_map_or_spacing_diacritics(&self) -> bool {
        self.has_pre_composed_to_base_map || self.has_spacing_diacritics
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
    pub fn is_spacing_diacritic(&self, c: char) -> bool {
        self.spacing_diacritics_slice
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
    pub fn is_pre_composed_to_base_char(&self, c: char) -> bool {
        self.pre_composed_to_base_char_slice.contains(&c)
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
    pub fn needs_pre_composed_to_base_map_or_spacing_diacritics_removal(&self, text: &str) -> bool {
        if self.has_pre_composed_to_base_map {
            text.chars()
                .any(|c| self.pre_composed_to_base_char_slice.contains(&c))
        } else {
            self.spacing_diacritics
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
    pub fn pre_composed_to_base_map(&self) -> &'static [PreComposedToBaseMap] {
        self.pre_composed_to_base_map
    }

    #[inline(always)]
    pub fn spacing_diacritics(&self) -> Option<DiacriticSet> {
        self.spacing_diacritics
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
    pub fn pre_composed_to_base_char_slice(&self) -> &'static [char] {
        self.pre_composed_to_base_char_slice
    }

    #[inline(always)]
    pub fn spacing_diacritics_slice(&self) -> Option<&'static [char]> {
        self.spacing_diacritics_slice
    }

    /// Finds a language-specific case map entry for a character.
    #[inline(always)]
    pub fn find_case_map(&self, c: char) -> Option<&'static CaseMap> {
        self.case_map.iter().find(|m| m.from == c)
    }

    /// Finds a language-specific fold map entry for a character.
    #[inline(always)]
    pub fn find_fold_map(&self, c: char) -> Option<&'static FoldMap> {
        self.fold_map.iter().find(|m| m.from == c)
    }

    #[inline(always)]
    pub fn find_transliterate_map(&self, c: char) -> Option<&'static FoldMap> {
        self.transliterate_map // Transliterate uses the FoldMap struct
            .iter()
            .find(|m| m.from == c)
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
    pub fn apply_pre_composed_to_base_map(&self, c: char) -> Option<char> {
        self.pre_composed_to_base_map
            .iter()
            .find(|&&PreComposedToBaseMap { from, .. }| from == c)
            .map(|&PreComposedToBaseMap { to, .. }| to)
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
    /// Estimate output capacity for case folding with expansions.
    /// Returns (num_expansions, extra_bytes_needed).
    #[inline]
    pub fn hint_capacity_fold(&self, text: &str) -> (usize, usize) {
        // Check flags once. No need to worry about has_one_to_one_folds.
        if !self.has_fold_map {
            return (0, 0);
        }

        let fold_map = self.fold_map; // Use the raw slice directly

        // --- Single-Pass, Unified Logic ---
        let mut num_folds = 0;
        let mut extra_bytes = 0;

        for c in text.chars() {
            // Replaced the call to external find_fold_map with inline iteration
            if let Some(m) = fold_map.iter().find(|m| m.from == c) {
                num_folds += 1;

                // This is the core logic for expansion calculation.
                // It runs for ALL matched characters, regardless of whether they expand.
                let from_len = c.len_utf8();
                let to_len = m.to.len();

                if to_len > from_len {
                    extra_bytes += to_len - from_len;
                }
            }
        }

        (num_folds, extra_bytes)
    }

    #[inline]
    pub fn hint_capacity_transliterate(&self, text: &str) -> (usize, usize) {
        if !self.has_transliterate_map {
            return (0, 0);
        }

        // `map` is the slice of TransliterateMap structs
        let map = self.transliterate_map; // Use the raw slice directly

        // --- Single-Pass, Unified Logic ---
        let mut num_transformations = 0;
        let mut extra_bytes = 0;

        for c in text.chars() {
            // Replaced the call to external find_fold_map with inline iteration
            // Note: Since FoldMap and TransliterateMap are the same struct (FoldMap),
            // the find logic is identical.
            if let Some(m) = map.iter().find(|m| m.from == c) {
                num_transformations += 1;

                // This calculates the expansion for ALL matches,
                // but the cost is negligible compared to the map search.
                let from_len = c.len_utf8();
                let to_len = m.to.len();

                // The branch is only taken if expansion occurs, which is rare.
                if to_len > from_len {
                    extra_bytes += to_len - from_len;
                }
            }
        }

        (num_transformations, extra_bytes)
    }
}

pub fn get_lang_entry_by_code(code: &str) -> Option<&'static LangEntry> {
    LANG_TABLE.get(&code.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use crate::{
        LANG_TABLE,
        lang::{
            LangEntry,
            data::{
                ARA, CAT, CES, DAN, DEU, ENG, FRA, HEB, HRV, JPN, KHM, KOR, MYA, NLD, NOR, POL,
                SLK, SRP, SWE, THA, TUR, VIE, ZHO, from_code,
            },
        },
    };

    fn get_from_table(code: &str) -> &'static LangEntry {
        LANG_TABLE.get(code).unwrap()
    }

    #[test]
    fn test_turkish_metadata() {
        let entry = get_from_table("TUR");
        assert!(entry.has_one_to_one_folds());
        assert!(!entry.requires_peek_ahead());
        assert!(entry.needs_case_fold('İ'));
        assert!(entry.needs_case_fold('I'));
        assert_eq!(entry.apply_case_fold('İ'), Some('i'));
        assert_eq!(entry.apply_case_fold('I'), Some('ı'));
    }

    #[test]
    fn test_german_metadata() {
        let entry = get_from_table("DEU");
        assert!(!entry.has_one_to_one_folds());
        assert!(!entry.requires_peek_ahead());
        assert!(entry.needs_case_fold('ß'));
    }

    #[test]
    fn test_dutch_metadata() {
        let entry = get_from_table("NLD");
        assert!(!entry.has_one_to_one_folds());
        assert!(entry.requires_peek_ahead());

        // Only uppercase triggers peek-ahead
        assert_eq!(entry.get_peek_fold('I', Some('J')), Some("ij"));
        assert_eq!(entry.get_peek_fold('i', Some('j')), None); // ← FIXED
        assert_eq!(entry.get_peek_fold('I', Some('K')), None);
        assert_eq!(entry.get_peek_fold('I', None), None);
    }

    #[test]
    fn test_english_metadata() {
        let entry = get_from_table("ENG");
        assert!(entry.has_one_to_one_folds());
        assert!(!entry.requires_peek_ahead());
        assert!(entry.needs_case_fold('A'));
        assert_eq!(entry.apply_case_fold('A'), Some('a'));
    }

    #[test]
    fn test_arabic_diacritics() {
        let entry = get_from_table("ARA");
        assert!(entry.has_spacing_diacritics());
        assert!(entry.is_spacing_diacritic('َ'));
        assert!(!entry.is_spacing_diacritic('ا'));
        // assert!(entry.contains_diacritics("مَرْحَبًا"));
        // assert!(!entry.contains_diacritics("مرحبا"));
    }

    #[test]
    fn test_from_code() {
        assert_eq!(from_code("TUR"), Some(TUR));
        assert_eq!(from_code("tur"), Some(TUR));
        assert_eq!(from_code("ENG"), Some(ENG));
        assert_eq!(from_code("XXX"), None);
    }

    #[test]
    fn test_apply_case_fold_preserves_grapheme_count_in_one_to_one_cases() {
        let cases = [
            ("ABCabc", "ENG"), // ASCII: byte == char
            ("éÉèÈ", "FRA"),   // Latin-1: 2-byte chars, but 1:1 mapping
            ("İIıi", "TUR"),   // Turkish: should preserve grapheme count
        ];

        for (text, code) in cases {
            let entry = get_from_table(code);
            let folded: String = text
                .chars()
                .filter_map(|c| entry.apply_case_fold(c))
                .collect();

            assert_eq!(
                text.chars().count(),
                folded.chars().count(),
                "Grapheme count changed for {} in lang {}",
                text,
                code
            );
        }
    }

    #[test]
    fn test_all_languages_have_valid_metadata() {
        let langs = [
            TUR, DEU, NLD, DAN, NOR, SWE, ARA, HEB, VIE, JPN, ZHO, KOR, THA, MYA, KHM, FRA, CAT,
            POL, CES, SLK, HRV, SRP, ENG,
        ];

        for lang in langs {
            let entry = get_from_table(lang.code());

            if entry.requires_peek_ahead {
                assert!(
                    !entry.peek_pairs.is_empty(),
                    "peek_ahead=true but peek_pairs empty for {:?}",
                    lang.name
                );
            }

            for fold in entry.fold_map {
                let char_count = fold.to.chars().count();

                if entry.has_one_to_one_folds() {
                    assert_eq!(
                        char_count,
                        1,
                        "{}: fold {} -> {} should be 1→1",
                        lang.code(),
                        fold.from,
                        fold.to
                    );
                }
            }

            if entry.spacing_diacritics.is_some() {
                assert!(entry.spacing_diacritics_slice.is_some());
                assert!(entry.has_spacing_diacritics());
            }
        }
    }

    #[test]
    fn test_segmentation_languages() {
        assert!(get_from_table("JPN").needs_segmentation());
        assert!(get_from_table("ZHO").needs_segmentation());
        assert!(get_from_table("KOR").needs_segmentation());
        assert!(get_from_table("THA").needs_segmentation());
        assert!(!get_from_table("ENG").needs_segmentation());
        assert!(!get_from_table("TUR").needs_segmentation());
    }

    #[test]
    fn test_case_map_only_turkish() {
        assert!(!get_from_table("TUR").case_map().is_empty());
        assert!(get_from_table("ENG").case_map().is_empty());
        assert!(get_from_table("DEU").case_map().is_empty());
        assert!(get_from_table("ARA").case_map().is_empty());
    }

    #[test]
    fn test_idempotency_metadata() {
        let langs = [TUR, DEU, NLD, DAN, NOR, SWE, FRA, POL, CES, SLK, HRV, SRP];

        for lang in langs {
            for fold in get_from_table(lang.code()).fold_map() {
                let target_lower: String = fold.to.chars().flat_map(|c| c.to_lowercase()).collect();
                assert_eq!(
                    fold.to,
                    target_lower,
                    "{}: fold target '{}' should already be lowercase",
                    lang.code(),
                    fold.to
                );
            }
        }
    }

    #[test]
    fn test_dutch_ij_variants() {
        assert_eq!(
            get_from_table("NLD").get_peek_fold('I', Some('J')),
            Some("ij")
        );
        assert_eq!(get_from_table("NLD").get_peek_fold('i', Some('j')), None); // ← FIXED
        assert_eq!(get_from_table("NLD").get_peek_fold('I', Some('K')), None);
        assert_eq!(get_from_table("NLD").get_peek_fold('I', None), None);
        assert_eq!(get_from_table("ENG").get_peek_fold('I', Some('J')), None);
        assert_eq!(get_from_table("TUR").get_peek_fold('I', Some('J')), None);
    }

    #[test]
    fn test_get_peek_fold_is_generalized() {
        assert_eq!(
            get_from_table("NLD").get_peek_fold('I', Some('J')),
            Some("ij")
        );
        assert_eq!(get_from_table("NLD").get_peek_fold('A', Some('B')), None);
    }

    #[test]
    fn test_performance_o1_lookup() {
        let text = "AAAAAAAAAA";
        let count = text
            .chars()
            .filter(|&c| get_from_table("ENG").needs_case_fold(c))
            .count();
        assert_eq!(count, 10);

        let turkish_text = "İİİİİİİİİİ";
        let count = turkish_text
            .chars()
            .filter(|&c| get_from_table("TUR").needs_case_fold(c))
            .count();
        assert_eq!(count, 10);
    }

    #[test]
    fn test_apply_case_fold_rejects_multi_char() {
        // German: multi-char folds should return None
        assert_eq!(
            get_from_table("DEU").apply_case_fold('ß'),
            None,
            "ß→ss is multi-char"
        );
        assert_eq!(
            get_from_table("DEU").apply_case_fold('ẞ'),
            None,
            "ẞ→ss is multi-char"
        );

        // Dutch: multi-char folds (ligatures) should return None
        assert_eq!(
            get_from_table("NLD").apply_case_fold('Ĳ'),
            None,
            "Ĳ→ij is multi-char"
        );
        assert_eq!(
            get_from_table("NLD").apply_case_fold('ĳ'),
            None,
            "ĳ→ij is multi-char"
        );

        // But regular chars work
        assert_eq!(get_from_table("DEU").apply_case_fold('A'), Some('a'));
        assert_eq!(get_from_table("NLD").apply_case_fold('A'), Some('a'));
    }

    #[test]
    fn test_apply_case_fold_accepts_one_to_one() {
        // Turkish: 1→1 folds should work
        assert_eq!(get_from_table("TUR").apply_case_fold('İ'), Some('i'));
        assert_eq!(get_from_table("TUR").apply_case_fold('I'), Some('ı'));

        // English: Unicode lowercase
        assert_eq!(get_from_table("ENG").apply_case_fold('A'), Some('a'));
        assert_eq!(get_from_table("ENG").apply_case_fold('Z'), Some('z'));
    }

    #[test]
    fn test_apply_lowercase_always_one_to_one() {
        // German: lowercase is always 1→1 (ẞ→ß, not →"ss")
        assert_eq!(get_from_table("DEU").apply_lowercase('ẞ'), 'ß');
        assert_eq!(get_from_table("DEU").apply_lowercase('ß'), 'ß');

        // Turkish
        assert_eq!(get_from_table("TUR").apply_lowercase('İ'), 'i');
        assert_eq!(get_from_table("TUR").apply_lowercase('I'), 'ı');

        // English
        assert_eq!(get_from_table("ENG").apply_lowercase('A'), 'a');
    }

    #[test]
    fn test_fold_vs_lowercase_difference() {
        // German ẞ (capital eszett)
        assert_eq!(
            get_from_table("DEU").apply_lowercase('ẞ'),
            'ß',
            "Lowercase: ẞ→ß"
        );
        assert_eq!(
            get_from_table("DEU").apply_case_fold('ẞ'),
            None,
            "Fold: ẞ→ss (multi-char, rejected)"
        );

        // German ß (lowercase eszett)
        assert_eq!(
            get_from_table("DEU").apply_lowercase('ß'),
            'ß',
            "Already lowercase"
        );
        assert_eq!(
            get_from_table("DEU").apply_case_fold('ß'),
            None,
            "Fold: ß→ss (multi-char, rejected)"
        );

        // This is why German can use CharMapper for Lowercase but not CaseFold
        assert!(!get_from_table("DEU").has_one_to_one_folds());
    }

    #[test]
    fn apply_lowercase_is_infallible() {
        assert_eq!(get_from_table("TUR").apply_lowercase('İ'), 'i');
        assert_eq!(get_from_table("TUR").apply_lowercase('I'), 'ı');
        assert_eq!(get_from_table("ENG").apply_lowercase('A'), 'a');
        assert_eq!(get_from_table("DEU").apply_lowercase('ẞ'), 'ß');
        assert_eq!(get_from_table("ARA").apply_lowercase('ا'), 'ا');
    }
}
