pub mod case_fold;
pub mod lower_case;
pub mod normalization;
pub mod normalize_punctuation;
pub mod normalize_whitespace;
pub mod remove_diacritics;
pub mod segment_words;
pub mod strip_control_chars;
pub mod strip_format_controls;
pub mod strip_html;
pub mod strip_markdown;
pub mod transliterate;
pub mod unify_width;

use crate::context::Context;
use std::borrow::Cow;
use std::iter::FusedIterator;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StageError {
    #[error("Normalization failed at stage `{0}`: {1}")]
    Failed(&'static str, String),

    #[error("Normalization validation failed at stage `{0}`: {1}")]
    Validation(&'static str, String),
}

/// # The Normy Stage Contract
///
/// Every stage in Normy follows this strict, performance-critical contract:
///
/// 1. `needs_apply`
///    - Must be fast, cheap, and as accurate as possible.
///    - False positives are acceptable only in astronomically rare cases.
///    - If it returns `false`, the entire stage is skipped at compile-time / machine-code level.
///    - This is the source of Normy’s extreme zero-copy performance.
///
/// 2. `apply`
///    - Is called **only** when `needs_apply` returned `true`.
///    - Is explicitly allowed to allocate and perform expensive work.
///    - Must **never** attempt to "salvage" zero-copy by comparing output with input.
///    - Must trust `needs_apply` unconditionally.
///
/// Stages that can transform text without allocation (e.g. pure character mappings)
/// should implement `as_char_mapper()` or `into_dyn_char_mapper()` instead.
pub trait Stage: Send + Sync {
    /// Human-readable name – used for profiling and error messages.
    fn name(&self) -> &'static str;

    /// Fast, cheap, usually perfect quick-check.
    /// If this returns false → the entire stage is elided at compile time / machine-code level.
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError>;

    /// Only called when needs_apply returned true.
    /// Always allocate. May mutate and may be slow.
    /// You must never try to "be clever" and return the input unchanged.
    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError>;

    /// Whether `needs_apply` on the *original* input text is a safe approximation
    /// for whether this stage will perform work, even after previous stages.
    ///
    /// If `true`, the fused pipeline may optimistically skip this stage
    /// based on the original text without breaking correctness.
    #[inline]
    fn safe_skip_approximation(&self) -> bool {
        false // default: conservative
    }

    #[inline]
    fn as_fusable(&self) -> Option<&dyn FusableStage> {
        None // Default: not fusable
    }

    /// Return a dynamic iterator
    /// Only called when needs_apply returned true.
    /// Always allocate. May mutate and may be slow.
    /// You must never try to "be clever" and return the input unchanged.
    #[inline]
    fn try_dynamic_iter<'a>(
        &self,
        _text: &'a str,
        _ctx: &'a Context,
    ) -> Option<Box<dyn FusedIterator<Item = char> + 'a>> {
        None
    }
}

/// Generic extension — Only used in static pipelines
pub trait StaticStageIter {
    /// Concrete iterator type — fully monomorphized
    type Iter<'a>: FusedIterator<Item = char> + 'a;

    /// Return a concrete iterator if this stage supports zero-dyn fusion
    /// Only called when needs_apply returned true.
    /// Always allocate. May mutate and may be slow.
    /// You must never try to "be clever" and return the input unchanged.
    #[inline]
    fn try_static_iter<'a>(&self, _text: &'a str, _ctx: &'a Context) -> Option<Self::Iter<'a>> {
        None
    }
}

// ============================================================================
// FusableStage Traits
// ============================================================================

/// A stage that can wrap an iterator input and produce an iterator output
pub trait FusableStage: Stage {
    /// Dynamic iterator adapter
    fn dyn_fused_adapter<'a>(
        &self,
        input: Box<dyn FusedIterator<Item = char> + 'a>,
        ctx: &'a Context,
    ) -> Box<dyn FusedIterator<Item = char> + 'a>;
}

/// Static (monomorphized) version for compile-time optimization
pub trait StaticFusableStage: Stage {
    type Adapter<'a, I>: FusedIterator<Item = char> + 'a
    where
        I: FusedIterator<Item = char> + 'a;

    fn static_fused_adapter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::Adapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;
}
