// src/context.rs
// This file is the single source of truth for language configuration in hot paths.
// It is deliberately tiny, Copy, and contains only 'static data.

use crate::lang::{DEFAULT_LANG, Lang, LangEntry, data::LANG_TABLE};

/// Runtime context passed to every normalization stage.
///
/// Contains:
/// - `lang`: human identifier (for logging, metrics, debugging)
/// - `lang_entry`: the actual language rules used in every hot path (zero-cost)
#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub lang: Lang,
    pub lang_entry: LangEntry,
}

impl Default for Context {
    #[inline(always)]
    fn default() -> Self {
        Self::new(DEFAULT_LANG)
    }
}

impl Context {
    /// Create a context using the canonical static data for a language.
    #[inline(always)]
    pub fn new(lang: Lang) -> Self {
        let lang_entry = LANG_TABLE
            .get(lang.code())
            .copied()
            .expect("language not present in LANG_TABLE – this is a bug");
        Self { lang, lang_entry }
    }

    /// Create a context and allow the caller to mutate any field before use.
    /// This is the zero-cost override mechanism used by `.modify_lang()`.
    #[inline(always)]
    pub fn with_modified(lang: Lang, f: impl FnOnce(&mut LangEntry)) -> Self {
        let mut lang_entry = LANG_TABLE
            .get(lang.code())
            .copied()
            .expect("language not present in LANG_TABLE – this is a bug");
        f(&mut lang_entry);
        Self { lang, lang_entry }
    }
}
