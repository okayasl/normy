pub(crate) mod behaviour;
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
    #[inline]
    pub fn has_one_to_one_folds(&self) -> bool {
        if self.fold_map.is_empty() {
            return true;
        }
        self.fold_map.iter().all(|m| m.to.chars().count() == 1)
    }
}
