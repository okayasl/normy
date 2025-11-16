//! stage/trim.rs
//!
//! # Zero-Copy Trim
//! * `needs_apply` → `ctx.lang.needs_trim(text)` – O(1) early exit via language metadata.
//! * `apply` → `str::trim_start` + `trim_end` → no allocation if already trimmed.
//! * No `CharMapper` – trim is not a 1-to-1 mapping (removes chars).
//! * All branches are `#[inline(always)]` → compiler fuses with `ChainedProcess`.

use crate::{
    context::Context,
    lang::LocaleBehavior,
    stage::{Stage, StageError},
};
use std::{borrow::Cow, sync::Arc};

/// Trims **Unicode whitespace** from both ends of the input.
/// Locale-aware via `Context::lang.needs_trim`.
#[derive(Debug, Default)]
pub struct Trim;

impl Stage for Trim {
    fn name(&self) -> &'static str {
        "trim"
    }

    /// Fast early-exit using language-specific `needs_trim`.
    /// Returns `true` **iff** the text actually contains leading/trailing whitespace.
    #[inline(always)]
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        Ok(ctx.lang.needs_trim(text))
    }

    /// Perform the trim.
    /// * Uses `trim_start` + `trim_end` → no intermediate allocation.
    /// * Returns `Cow::Borrowed` if the pointer/length did **not** change.
    #[inline(always)]
    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // `trim_start` and `trim_end` are both zero-cost if no work is needed.
        let start_trimmed = text.trim_start();
        let trimmed = start_trimmed.trim_end();

        // Fast path: unchanged pointer & length → borrow.
        if trimmed.as_ptr() == text.as_ptr() && trimmed.len() == text.len() {
            return Ok(text);
        }

        // Allocation path: only when we actually removed whitespace.
        Ok(Cow::Owned(trimmed.to_owned()))
    }

    #[inline]
    fn as_char_mapper(&self, _: &Context) -> Option<&dyn crate::stage::CharMapper> {
        None
    }

    #[inline]
    fn into_dyn_char_mapper(
        self: Arc<Self>,
        _: &Context,
    ) -> Option<Arc<dyn crate::stage::CharMapper>> {
        None
    }
}
