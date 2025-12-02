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
    // pub diacritic_slice: Option<&'static [char]>,
    pub fold_char_slice: &'static [char],
    pub transliterate_char_slice: &'static [char],
    pub peek_pairs: &'static [PeekPair],
    pub segment_rules: &'static [SegmentRule],

    // === Zero-Cost Function Pointers (Compile-Time Specialized) ===
    pub find_case_map: fn(char) -> Option<char>,
    pub find_fold_map: fn(char) -> Option<&'static str>,
    pub find_transliterate_map: fn(char) -> Option<&'static str>,
    pub find_strip_map: fn(char) -> Option<char>,
    pub contains_fold_char: fn(char) -> bool,
    pub contains_transliterate_char: fn(char) -> bool,
    pub contains_strip_char: fn(char) -> bool,
    pub contains_diacritic: fn(char) -> bool,
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
    // CATEGORY 2: Character Checks - Pattern: is_* (now zero-cost!)
    // ============================================================

    #[inline(always)]
    pub fn is_diacritic(&self, c: char) -> bool {
        (self.contains_diacritic)(c)
    }

    #[inline(always)]
    pub fn is_foldable(&self, c: char) -> bool {
        (self.contains_fold_char)(c)
    }

    #[inline(always)]
    pub fn is_transliterable(&self, c: char) -> bool {
        (self.contains_transliterate_char)(c)
    }

    #[inline(always)]
    pub fn is_strippable(&self, c: char) -> bool {
        (self.contains_strip_char)(c)
    }

    // ============================================================
    // CATEGORY 3: Text Analysis - Pattern: needs_*
    // ============================================================

    #[inline(always)]
    pub fn needs_case_fold(&self, c: char) -> bool {
        (self.contains_fold_char)(c)
            || (self.find_case_map)(c).is_some()
            || c.to_lowercase().next() != Some(c)
    }

    #[inline(always)]
    pub fn needs_lowercase(&self, c: char) -> bool {
        (self.find_case_map)(c).is_some() || c.to_lowercase().next() != Some(c)
    }

    #[inline(always)]
    pub fn needs_diacritic_removal(&self, text: &str) -> bool {
        if self.has_strip_map {
            text.chars().any(|c| (self.contains_strip_char)(c))
        } else {
            text.chars().any(|c| (self.contains_diacritic)(c))
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

    // #[inline(always)]
    // pub fn diacritic_slice(&self) -> Option<&'static [char]> {
    //     self.diacritic_slice
    // }

    // ============================================================
    // CATEGORY 5: Transformations - Pattern: apply_* or get_*
    // ============================================================

    #[inline(always)]
    pub fn apply_case_fold(&self, c: char) -> Option<char> {
        // 1. Try fold_map (handles multi-char expansions, returns None if >1 char)
        if let Some(folded) = (self.find_fold_map)(c) {
            let mut chars = folded.chars();
            let first = chars.next()?;
            if chars.next().is_none() {
                return Some(first);
            } else {
                return None;
            }
        }
        // 2. Try case_map
        if let Some(mapped) = (self.find_case_map)(c) {
            return Some(mapped);
        }
        // 3. Unicode fallback
        c.to_lowercase().next()
    }

    #[inline(always)]
    pub fn apply_lowercase(&self, c: char) -> char {
        (self.find_case_map)(c).unwrap_or_else(|| c.to_lowercase().next().unwrap_or(c))
    }

    #[inline(always)]
    pub fn apply_strip(&self, c: char) -> Option<char> {
        (self.find_strip_map)(c)
    }

    #[inline]
    pub fn get_peek_fold(&self, current: char, next: Option<char>) -> Option<&'static str> {
        if !self.requires_peek_ahead {
            return None;
        }
        let next_char = next?;

        // Check explicit peek pairs first
        for p in self.peek_pairs {
            if p.a == current && p.b == next_char {
                return Some(p.to);
            }
        }

        // Check if both chars fold to the same multi-char sequence
        let cur = (self.find_fold_map)(current)?;
        let nxt = (self.find_fold_map)(next_char)?;
        if cur == nxt && cur.chars().count() > 1 {
            Some(cur)
        } else {
            None
        }
    }

    // ============================================================
    // CATEGORY 6: Capacity Hints - Pattern: hint_capacity_*
    // ============================================================

    #[inline]
    pub fn hint_capacity_fold(&self, text: &str) -> (usize, usize) {
        if !self.has_fold_map {
            return (0, 0);
        }

        if self.has_one_to_one_folds {
            let count = text
                .chars()
                .filter(|&c| (self.contains_fold_char)(c))
                .count();
            return (count, 0);
        }

        let mut count = 0;
        let mut extra = 0;
        for c in text.chars() {
            if let Some(to) = (self.find_fold_map)(c) {
                count += 1;
                let from_len = c.len_utf8();
                let to_len = to.len();
                if to_len > from_len {
                    extra += to_len - from_len;
                }
            }
        }
        (count, extra)
    }

    #[inline]
    pub fn hint_capacity_transliterate(&self, text: &str) -> (usize, usize) {
        if !self.has_transliterate_map {
            return (0, 0);
        }

        if self.has_one_to_one_transliterate {
            let count = text
                .chars()
                .filter(|&c| (self.contains_transliterate_char)(c))
                .count();
            return (count, 0);
        }

        let mut count = 0;
        let mut extra = 0;
        for c in text.chars() {
            if let Some(to) = (self.find_transliterate_map)(c) {
                count += 1;
                let from_len = c.len_utf8();
                let to_len = to.len();
                if to_len > from_len {
                    extra += to_len - from_len;
                }
            }
        }
        (count, extra)
    }

    // ============================================================
    // OLD: Slice-based versions (for comparison) - suffixed with _old
    // ============================================================
    #[inline(always)]
    pub fn is_diacritic_old(&self, c: char) -> bool {
        self.diacritics
            .map(|slice| slice.contains(&c))
            .unwrap_or(false)
    }

    #[inline(always)]
    pub fn is_foldable_old(&self, c: char) -> bool {
        self.fold_char_slice.contains(&c)
    }

    #[inline(always)]
    pub fn is_transliterable_old(&self, c: char) -> bool {
        self.transliterate_char_slice.contains(&c)
    }

    #[inline(always)]
    pub fn is_strippable_old(&self, c: char) -> bool {
        self.strip_char_slice.contains(&c)
    }

    #[inline(always)]
    pub fn needs_case_fold_old(&self, c: char) -> bool {
        self.fold_char_slice.contains(&c)
            || self.case_map.iter().any(|m| m.from == c)
            || c.to_lowercase().next() != Some(c)
    }

    #[inline(always)]
    pub fn needs_lowercase_old(&self, c: char) -> bool {
        if self.case_map.iter().any(|m| m.from == c) {
            return true;
        }
        c.to_lowercase().next() != Some(c)
    }

    #[inline(always)]
    pub fn needs_diacritic_removal_old(&self, text: &str) -> bool {
        if self.has_strip_map {
            text.chars().any(|c| self.strip_char_slice.contains(&c))
        } else {
            self.diacritics
                .map(|diacs| text.chars().any(|c| diacs.contains(&c)))
                .unwrap_or(false)
        }
    }

    #[inline(always)]
    pub fn apply_case_fold_old(&self, c: char) -> Option<char> {
        if let Some(m) = self.fold_map.iter().find(|m| m.from == c) {
            let mut chars = m.to.chars();
            let first = chars.next()?;
            if chars.next().is_none() {
                Some(first)
            } else {
                None
            }
        } else if let Some(m) = self.case_map.iter().find(|m| m.from == c) {
            Some(m.to)
        } else {
            c.to_lowercase().next()
        }
    }

    #[inline(always)]
    pub fn apply_lowercase_old(&self, c: char) -> char {
        if let Some(m) = self.case_map.iter().find(|m| m.from == c) {
            m.to
        } else {
            c.to_lowercase().next().unwrap_or(c)
        }
    }

    #[inline(always)]
    pub fn apply_strip_old(&self, c: char) -> Option<char> {
        self.strip_map
            .iter()
            .find(|&&StripMap { from, .. }| from == c)
            .map(|&StripMap { to, .. }| to)
    }

    #[inline]
    pub fn get_peek_fold_old(&self, current: char, next: Option<char>) -> Option<&'static str> {
        if !self.requires_peek_ahead {
            return None;
        }
        let next_char = next?;

        for p in self.peek_pairs {
            if p.a == current && p.b == next_char {
                return Some(p.to);
            }
        }

        let cur = self.fold_map.iter().find(|m| m.from == current)?;
        let nxt = self.fold_map.iter().find(|m| m.from == next_char)?;
        if cur.to == nxt.to && cur.to.chars().count() > 1 {
            Some(cur.to)
        } else {
            None
        }
    }

    #[inline]
    pub fn hint_capacity_fold_old(&self, text: &str) -> (usize, usize) {
        if !self.has_fold_map {
            return (0, 0);
        }

        let fold_map = self.fold_map();

        if self.has_one_to_one_folds {
            let count = text
                .chars()
                .filter(|&c| fold_map.iter().any(|m| m.from == c))
                .count();
            return (count, 0);
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
    pub fn hint_capacity_transliterate_old(&self, text: &str) -> (usize, usize) {
        if !self.has_transliterate_map {
            return (0, 0);
        }

        let map = self.transliterate_map();

        if self.has_one_to_one_transliterate {
            let count = text
                .chars()
                .filter(|&c| map.iter().any(|m| m.from == c))
                .count();
            return (count, 0);
        }

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

// #[inline(always)]
// const fn slice_len<T>(slice: &[T]) -> usize {
//     slice.len()
// }

// /// Hybrid lookup: chooses the fastest strategy based on map size at compile time.
// ///
// /// - ≤4 entries  → `match` (branch predictor heaven)
// /// - ≤15 entries → linear scan (cache-hot, no binary search overhead)
// /// - ≥16 entries → binary search (scales perfectly)
// #[inline(always)]
// fn hybrid_find<T, F>(map: &'static [T], c: char, key: F) -> Option<&'static T>
// where
//     F: Fn(&T) -> char,
// {
//     match slice_len(map) {
//         0 => None,
//         1 => (key(&map[0]) == c).then_some(&map[0]),
//         2 => match c {
//             k if k == key(&map[0]) => Some(&map[0]),
//             k if k == key(&map[1]) => Some(&map[1]),
//             _ => None,
//         },
//         3 => match c {
//             k if k == key(&map[0]) => Some(&map[0]),
//             k if k == key(&map[1]) => Some(&map[1]),
//             k if k == key(&map[2]) => Some(&map[2]),
//             _ => None,
//         },
//         4 => match c {
//             k if k == key(&map[0]) => Some(&map[0]),
//             k if k == key(&map[1]) => Some(&map[1]),
//             k if k == key(&map[2]) => Some(&map[2]),
//             k if k == key(&map[3]) => Some(&map[3]),
//             _ => None,
//         },
//         5..=15 => map.iter().find(|entry| key(entry) == c),
//         _ => match map.binary_search_by(|entry| key(entry).cmp(&c)) {
//             Ok(i) => Some(&map[i]),
//             Err(_) => None,
//         },
//     }
//}

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
        assert!(entry.has_diacritics());
        assert!(entry.is_diacritic('َ'));
        assert!(!entry.is_diacritic('ا'));
        // assert!(entry.contains_diacritics("مَرْحَبًا"));
        // assert!(!entry.contains_diacritics("مرحبا"));
    }

    #[test]
    fn test_from_code() {
        assert_eq!(from_code("TUR").unwrap().code, "TUR");
        assert_eq!(from_code("tur").unwrap().code, "TUR");
        assert_eq!(from_code("ENG").unwrap().code, "ENG");
        assert!(from_code("XXX").is_none());
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

            if entry.diacritics.is_some() {
                assert!(entry.has_diacritics());
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

    #[test]
    fn test_spacing_diacritics() {
        let entry = get_from_table("VIE");
        assert!(entry.diacritics.is_some());
    }

    #[test]
    fn test_apply_strip_vietnamese_stacked_diacritic() {
        let entry = get_from_table("VIE");
        assert_eq!(entry.apply_strip('\u{1EA1}'), Some('a')); // ạ → a
        assert_eq!(entry.apply_strip('A'), None); // No mapping
    }

    // #[test]
    // fn test_hybrid_find_turkish_case_map() {
    //     let turkish = get_from_table("TUR");
    //     assert_eq!(turkish.case_map.len(), 2); // Triggers n=2 path

    //     assert_eq!(
    //         hybrid_find(turkish.case_map, 'İ', |m| m.from).unwrap().to,
    //         'i'
    //     );
    //     assert_eq!(
    //         hybrid_find(turkish.case_map, 'I', |m| m.from).unwrap().to,
    //         'ı'
    //     );
    //     assert!(hybrid_find(turkish.case_map, 'A', |m| m.from).is_none());
    // }
}
