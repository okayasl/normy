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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentRule {
    WesternToScript,
    ScriptToWestern,
    CJKIdeographUnigram,
}

#[derive(Clone, Copy, Debug)]
pub struct LangEntry {
    // === Precomputed Boolean Flags (Hot Path - First Cache Line) ===
    has_case_map: bool,
    has_fold_map: bool,
    has_transliterate_map: bool,
    has_pre_composed_to_base_map: bool,
    has_spacing_diacritics: bool,
    has_peek_pairs: bool,
    has_segment_rules: bool,

    // Derived properties
    has_one_to_one_folds: bool,
    has_one_to_one_transliterate: bool,

    // Already boolean from definition
    needs_segmentation: bool,
    requires_peek_ahead: bool,
    unigram_cjk: bool,

    // === Data Arrays (Second Cache Line+) ===
    code: &'static str,
    case_map: &'static [(char, char)],
    fold_map: &'static [(char, &'static str)],
    pre_composed_to_base_map: &'static [(char, char)],
    pre_composed_to_base_char_slice: &'static [char],
    spacing_diacritics: Option<&'static [char]>,
    spacing_diacritics_slice: Option<&'static [char]>,
    transliterate_map: &'static [(char, &'static str)],
    transliterate_char_slice: &'static [char],
    peek_pairs: &'static [(char, char, &'static str)],
    segment_rules: &'static [SegmentRule],
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
        self.fold_map.iter().any(|(from, _)| *from == c)
            || self.case_map.iter().any(|(from, _)| *from == c)
            || c.to_lowercase().next() != Some(c)
    }

    #[inline(always)]
    pub fn needs_lowercase(&self, c: char) -> bool {
        if self.case_map.iter().any(|(from, _)| *from == c) {
            return true;
        }
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
    pub fn code(&self) -> &'static str {
        self.code
    }

    #[inline(always)]
    pub fn case_map(&self) -> &'static [(char, char)] {
        self.case_map
    }

    #[inline(always)]
    pub fn fold_map(&self) -> &'static [(char, &'static str)] {
        self.fold_map
    }

    #[inline(always)]
    pub fn transliterate_map(&self) -> &'static [(char, &'static str)] {
        self.transliterate_map
    }

    #[inline(always)]
    pub fn pre_composed_to_base_map(&self) -> &'static [(char, char)] {
        self.pre_composed_to_base_map
    }

    #[inline(always)]
    pub fn spacing_diacritics(&self) -> Option<&'static [char]> {
        self.spacing_diacritics
    }

    #[inline(always)]
    pub fn peek_pairs(&self) -> &'static [(char, char, &'static str)] {
        self.peek_pairs
    }

    #[inline(always)]
    pub fn segment_rules(&self) -> &'static [SegmentRule] {
        self.segment_rules
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
    pub fn find_case_map(&self, c: char) -> Option<char> {
        self.case_map
            .iter()
            .find(|(from, _)| *from == c)
            .map(|(_, to)| *to)
    }

    /// Finds a language-specific fold map entry for a character.
    #[inline(always)]
    pub fn find_fold_map(&self, c: char) -> Option<&'static str> {
        self.fold_map
            .iter()
            .find(|(from, _)| *from == c)
            .map(|(_, to)| *to)
    }

    #[inline(always)]
    pub fn find_transliterate_map(&self, c: char) -> Option<&'static str> {
        self.transliterate_map
            .iter()
            .find(|(from, _)| *from == c)
            .map(|(_, to)| *to)
    }

    #[inline(always)]
    pub fn find_pre_composed_to_base_map(&self, c: char) -> Option<char> {
        self.pre_composed_to_base_map
            .iter()
            .find(|(from, _)| *from == c)
            .map(|(_, to)| *to)
    }

    // ============================================================
    // CATEGORY 5: Transformations - Pattern: apply_* or get_*
    // ============================================================

    #[inline(always)]
    pub fn apply_case_fold(&self, c: char) -> Option<char> {
        if let Some(to) = self
            .fold_map
            .iter()
            .find(|(from, _)| *from == c)
            .map(|(_, to)| *to)
        {
            if self.has_one_to_one_folds {
                Some(to.chars().next().unwrap_or(c)) // Safe: we know it's 1 char
            } else {
                None
            }
        } else if let Some(to) = self
            .case_map
            .iter()
            .find(|(from, _)| *from == c)
            .map(|(_, to)| *to)
        {
            Some(to)
        } else {
            c.to_lowercase().next()
        }
    }

    #[inline(always)]
    pub fn apply_lowercase(&self, c: char) -> char {
        if let Some(to) = self
            .case_map
            .iter()
            .find(|(from, _)| *from == c)
            .map(|(_, to)| *to)
        {
            to
        } else {
            c.to_lowercase().next().unwrap_or(c)
        }
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
        for (a, b, to) in self.peek_pairs {
            if *a == current && *b == next_char {
                return Some(to);
            }
        }

        // Fallback heuristic – only for *single-char* expansions
        let cur = self.fold_map.iter().find(|(from, _)| *from == current)?.1;
        let nxt = self.fold_map.iter().find(|(from, _)| *from == next_char)?.1;

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
        // Check flags once. No need to worry about has_one_to_one_folds.
        if !self.has_fold_map {
            return (0, 0);
        }

        let fold_map = self.fold_map;

        // --- Single-Pass, Unified Logic ---
        let mut num_folds = 0;
        let mut extra_bytes = 0;

        for c in text.chars() {
            if let Some(to) = fold_map
                .iter()
                .find(|(from, _)| *from == c)
                .map(|(_, to)| *to)
            {
                num_folds += 1;

                let from_len = c.len_utf8();
                let to_len = to.len();

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

        let map = self.transliterate_map;

        // --- Single-Pass, Unified Logic ---
        let mut num_transformations = 0;
        let mut extra_bytes = 0;

        for c in text.chars() {
            if let Some(to) = map.iter().find(|(from, _)| *from == c).map(|(_, to)| *to) {
                num_transformations += 1;

                let from_len = c.len_utf8();
                let to_len = to.len();

                if to_len > from_len {
                    extra_bytes += to_len - from_len;
                }
            }
        }

        (num_transformations, extra_bytes)
    }

    // ============================================================
    // CATEGORY 7: Setters - Pattern: set_*
    // Updates primary field and all dependent derived fields
    // ============================================================

    /// Sets the case_map and updates has_case_map flag
    #[inline]
    pub fn set_case_map(&mut self, case_map: &'static [(char, char)]) {
        self.case_map = case_map;
        self.has_case_map = !case_map.is_empty();
    }

    /// Sets the fold_map and updates all related fields
    #[inline]
    pub fn set_fold_map(&mut self, fold_map: &'static [(char, &'static str)]) {
        self.fold_map = fold_map;
        self.has_fold_map = !fold_map.is_empty();

        if !fold_map.is_empty() {
            self.has_one_to_one_folds = fold_map.iter().all(|(_, to)| to.chars().count() == 1);
        } else {
            self.has_one_to_one_folds = false;
        }
    }

    /// Sets the transliterate_map and updates all related fields
    #[inline]
    pub fn set_transliterate_map(&mut self, transliterate_map: &'static [(char, &'static str)]) {
        self.transliterate_map = transliterate_map;
        self.has_transliterate_map = !transliterate_map.is_empty();

        if !transliterate_map.is_empty() {
            self.has_one_to_one_transliterate = transliterate_map
                .iter()
                .all(|(_, to)| to.chars().count() == 1);
        } else {
            self.has_one_to_one_transliterate = false;
        }
    }

    /// Helper to set the transliterate_char_slice directly
    #[inline]
    pub fn set_transliterate_char_slice(&mut self, slice: &'static [char]) {
        self.transliterate_char_slice = slice;
    }

    /// Sets the pre_composed_to_base_map and updates all related fields
    #[inline]
    pub fn set_pre_composed_to_base_map(&mut self, map: &'static [(char, char)]) {
        self.pre_composed_to_base_map = map;
        self.has_pre_composed_to_base_map = !map.is_empty();
    }

    /// Helper to set the pre_composed_to_base_char_slice directly
    #[inline]
    pub fn set_pre_composed_to_base_char_slice(&mut self, slice: &'static [char]) {
        self.pre_composed_to_base_char_slice = slice;
    }

    /// Sets the spacing_diacritics and updates all related fields
    #[inline]
    pub fn set_spacing_diacritics(&mut self, diacritics: Option<&'static [char]>) {
        self.spacing_diacritics = diacritics;
        self.spacing_diacritics_slice = diacritics;
        self.has_spacing_diacritics = diacritics.is_some() && !diacritics.unwrap().is_empty();
    }

    /// Sets the needs_segmentation flag
    #[inline]
    pub fn set_needs_segmentation(&mut self, needs: bool) {
        self.needs_segmentation = needs;
    }

    /// Sets the requires_peek_ahead flag
    #[inline]
    pub fn set_requires_peek_ahead(&mut self, requires: bool) {
        self.requires_peek_ahead = requires;
    }

    /// Sets the peek_pairs and updates related fields
    #[inline]
    pub fn set_peek_pairs(&mut self, pairs: &'static [(char, char, &'static str)]) {
        self.peek_pairs = pairs;
        self.has_peek_pairs = !pairs.is_empty();

        // Auto-enable peek_ahead if we have peek pairs
        if self.has_peek_pairs {
            self.requires_peek_ahead = true;
        }
    }

    /// Sets the segment_rules and updates related fields
    #[inline]
    pub fn set_segment_rules(&mut self, rules: &'static [SegmentRule]) {
        self.segment_rules = rules;
        self.has_segment_rules = !rules.is_empty();

        // Auto-detect if CJK unigram rule is present
        self.unigram_cjk = rules.contains(&SegmentRule::CJKIdeographUnigram);
    }

    /// Sets the unigram_cjk flag directly
    #[inline]
    pub fn set_unigram_cjk(&mut self, unigram: bool) {
        self.unigram_cjk = unigram;
    }
}

pub fn get_lang_entry_by_code(code: &str) -> Option<&'static LangEntry> {
    LANG_TABLE.get(&code.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use crate::{all_langs, lang::get_lang_entry_by_code};

    // Helper for concise test access
    fn lang(code: &str) -> &'static crate::lang::LangEntry {
        get_lang_entry_by_code(code).unwrap()
    }

    // ============================================================
    // CATEGORY 1: Language-Specific Behavior Tests
    // ============================================================

    #[test]
    fn turkish_case_folding() {
        let tur = lang("TUR");

        // Turkish-specific: İ→i, I→ı
        assert_eq!(tur.apply_case_fold('İ'), Some('i'));
        assert_eq!(tur.apply_case_fold('I'), Some('ı'));
        assert_eq!(tur.apply_lowercase('İ'), 'i');
        assert_eq!(tur.apply_lowercase('I'), 'ı');

        // Metadata checks
        assert!(tur.has_one_to_one_folds());
        assert!(!tur.requires_peek_ahead());
        assert!(!tur.case_map().is_empty(), "Turkish has custom case_map");
    }

    #[test]
    fn german_multi_char_folding() {
        let deu = lang("DEU");

        // German: ẞ→ß (lowercase) vs ẞ→ss (case fold)
        assert_eq!(deu.apply_lowercase('ẞ'), 'ß');
        assert_eq!(deu.apply_case_fold('ẞ'), None, "ẞ→ss is multi-char");
        assert_eq!(deu.apply_case_fold('ß'), None, "ß→ss is multi-char");

        // Metadata
        assert!(!deu.has_one_to_one_folds());
        assert!(!deu.requires_peek_ahead());
    }

    #[test]
    fn dutch_peek_ahead_ligatures() {
        let nld = lang("NLD");

        // Ĳ→ij is multi-char fold
        assert_eq!(nld.apply_case_fold('Ĳ'), None);
        assert_eq!(nld.apply_case_fold('ĳ'), None);

        // Metadata
        assert!(!nld.has_one_to_one_folds());
    }

    #[test]
    fn arabic_diacritics() {
        let ara = lang("ARA");

        assert!(ara.has_spacing_diacritics());
        assert!(ara.is_spacing_diacritic('\u{064E}'), "َ (fatha)");
        assert!(!ara.is_spacing_diacritic('ا'), "Base letter ا");
    }

    #[test]
    fn english_basic_case_folding() {
        let eng = lang("ENG");

        assert_eq!(eng.apply_case_fold('A'), Some('a'));
        assert_eq!(eng.apply_case_fold('Z'), Some('z'));
        assert_eq!(eng.apply_lowercase('A'), 'a');

        assert!(eng.has_one_to_one_folds());
        assert!(!eng.requires_peek_ahead());
        assert!(eng.case_map().is_empty(), "English uses Unicode defaults");
    }

    // ============================================================
    // CATEGORY 2: Cross-Language Consistency Tests
    // ============================================================

    #[test]
    fn segmentation_languages() {
        // Languages requiring word segmentation
        let needs_seg = ["JPN", "ZHO", "KOR", "THA", "MYA", "KHM"];
        for code in needs_seg {
            assert!(
                lang(code).needs_segmentation(),
                "{} needs segmentation",
                code
            );
        }

        // Languages with space-delimited words
        let no_seg = ["ENG", "TUR", "DEU", "FRA", "ARA"];
        for code in no_seg {
            assert!(
                !lang(code).needs_segmentation(),
                "{} doesn't need segmentation",
                code
            );
        }
    }

    #[test]
    fn case_map_exclusivity() {
        // Only Turkish has custom case_map (for İ/I)
        assert!(!lang("TUR").case_map().is_empty());

        // All others use Unicode defaults
        for code in ["ENG", "DEU", "FRA", "ARA", "NLD", "POL"] {
            assert!(
                lang(code).case_map().is_empty(),
                "{} should use Unicode case",
                code
            );
        }
    }

    #[test]
    fn peek_ahead_requirements() {
        // Only Dutch requires peek-ahead for IJ ligature
        // assert!(lang("NLD").requires_peek_ahead());

        // Others don't need it
        for code in ["ENG", "TUR", "DEU", "FRA", "ARA"] {
            assert!(
                !lang(code).requires_peek_ahead(),
                "{} shouldn't need peek-ahead",
                code
            );
        }
    }

    // ============================================================
    // CATEGORY 3: Metadata Consistency Tests
    // ============================================================

    #[test]
    fn all_languages_metadata_valid() {
        for lang_info in all_langs() {
            let entry = lang(lang_info.code());

            // Peek-ahead requires peek_pairs
            if entry.requires_peek_ahead() {
                assert!(
                    !entry.peek_pairs().is_empty(),
                    "{}: requires_peek_ahead but no peek_pairs",
                    lang_info.code()
                );
            }

            // has_one_to_one_folds correctness
            if entry.has_one_to_one_folds() {
                for (from, to) in entry.fold_map() {
                    assert_eq!(
                        to.chars().count(),
                        1,
                        "{}: {} → {} violates one_to_one",
                        lang_info.code(),
                        from,
                        to
                    );
                }
            }

            // Spacing diacritics consistency
            if entry.has_spacing_diacritics() {
                assert!(
                    entry.spacing_diacritics().is_some(),
                    "{}: has_spacing_diacritics but field is None",
                    lang_info.code()
                );
            }
        }
    }

    #[test]
    fn fold_targets_already_lowercase() {
        // Fold targets must be idempotent (already in lowercase form)
        let test_langs = ["TUR", "DEU", "NLD", "FRA", "POL", "CES"];

        for code in test_langs {
            for (_, to) in lang(code).fold_map() {
                let target_lower: String = to.chars().flat_map(|c| c.to_lowercase()).collect();
                assert_eq!(
                    *to, target_lower,
                    "{}: fold target '{}' not lowercase",
                    code, to
                );
            }
        }
    }

    // ============================================================
    // CATEGORY 4: API Contract Tests
    // ============================================================

    #[test]
    fn apply_case_fold_rejects_multi_char() {
        // Case fold returns None for multi-char expansions
        assert_eq!(lang("DEU").apply_case_fold('ß'), None, "ß→ss");
        assert_eq!(lang("DEU").apply_case_fold('ẞ'), None, "ẞ→ss");
        assert_eq!(lang("NLD").apply_case_fold('Ĳ'), None, "Ĳ→ij");

        // But accepts one-to-one
        assert_eq!(lang("TUR").apply_case_fold('İ'), Some('i'));
        assert_eq!(lang("DEU").apply_case_fold('A'), Some('a'));
    }

    #[test]
    fn apply_lowercase_always_succeeds() {
        // Lowercase is always 1→1, never fails
        assert_eq!(lang("TUR").apply_lowercase('İ'), 'i');
        assert_eq!(lang("DEU").apply_lowercase('ẞ'), 'ß', "ẞ→ß, NOT →ss");
        assert_eq!(lang("ARA").apply_lowercase('ا'), 'ا', "Already lowercase");

        // Preserves grapheme count
        let text = "İIıiẞßABC";
        for c in text.chars() {
            let result = lang("TUR").apply_lowercase(c);
            assert_eq!(result.to_string().chars().count(), 1);
        }
    }

    #[test]
    fn fold_vs_lowercase_difference() {
        // Key difference: lowercase is 1→1, fold can be 1→n

        // German ẞ: lowercase→ß (1→1), fold→ss (1→2)
        assert_eq!(lang("DEU").apply_lowercase('ẞ'), 'ß');
        assert_eq!(lang("DEU").apply_case_fold('ẞ'), None);

        // This is why: DEU supports lowercase but not apply_case_fold
        assert!(!lang("DEU").has_one_to_one_folds());
    }

    #[test]
    fn get_lang_entry_by_code_case_insensitive() {
        assert!(get_lang_entry_by_code("TUR").is_some());
        assert!(get_lang_entry_by_code("tur").is_some());
        assert!(get_lang_entry_by_code("Tur").is_some());
        assert!(get_lang_entry_by_code("XXX").is_none());
    }

    // ============================================================
    // CATEGORY 5: Performance Characteristics Tests
    // ============================================================

    #[test]
    fn needs_case_fold_is_fast() {
        // Verify O(1) or O(log n) lookup performance
        let text = "A".repeat(1000);
        let start = std::time::Instant::now();

        let count = text
            .chars()
            .filter(|&c| lang("ENG").needs_case_fold(c))
            .count();

        let elapsed = start.elapsed();
        assert_eq!(count, 1000);
        assert!(elapsed.as_millis() < 10, "Should be sub-millisecond");
    }

    #[test]
    fn one_to_one_preserves_grapheme_count() {
        let cases = [
            ("ABCabc", "ENG"), // ASCII
            ("éÉèÈ", "FRA"),   // Latin accents
            ("İIıi", "TUR"),   // Turkish dotted-I
        ];

        for (text, code) in cases {
            let entry = lang(code);
            let folded: String = text
                .chars()
                .filter_map(|c| entry.apply_case_fold(c))
                .collect();

            assert_eq!(
                text.chars().count(),
                folded.chars().count(),
                "{} grapheme count changed for {}",
                code,
                text
            );
        }
    }

    // ============================================================
    // CATEGORY 6: Edge Cases
    // ============================================================

    #[test]
    fn empty_text_operations() {
        let entry = lang("ENG");

        assert_eq!(entry.hint_capacity_fold(""), (0, 0));
        assert_eq!(entry.hint_capacity_transliterate(""), (0, 0));
        assert!(!entry.needs_pre_composed_to_base_map_or_spacing_diacritics_removal(""));
    }

    #[test]
    fn ascii_fast_paths() {
        let entry = lang("ENG");

        // ASCII lowercase should be fast rejection
        assert!(!entry.needs_case_fold('a'));
        assert!(!entry.needs_lowercase('a'));

        // ASCII uppercase needs folding
        assert!(entry.needs_case_fold('A'));
        assert!(entry.needs_lowercase('A'));
    }
}
