//! Core normalization stage abstraction.
//!
//! # Zero-Allocation Loop Fusion (White-Paper §3.3)
//!
//! Normy guarantees *zero-allocation* pipelines **when every stage is known at
//! compile time**.  The Rust compiler can inline `Iterator::next` of every
//! adapter and fuse the whole chain into a single machine-code loop.
//!
//! To make this possible **without heap allocation** we need two entry points:
//!
//! * `as_char_mapper(&self) -> Option<&dyn CharMapper>` – **static path**.
//!   Returns a *trait-object reference* (`&dyn`) that points to `self`.  No
//!   `Box`, no `Arc`, no heap.  Used by `ChainedProcess` (monomorphised pipelines).
//!
//! * `into_dyn_char_mapper(self: Arc<Self>) -> Option<Arc<dyn CharMapper>>` – **dynamic path**.
//!   Consumes an `Arc<Self>` and returns an `Arc<dyn CharMapper>`.  Only used
//!   inside `DynProcess` (plugin pipelines).  The `Arc` is the only allocation
//!   required for dynamic extensibility.
//!
//! Stages that cannot be expressed as a 1-to-1 character mapping (e.g. NFKC)
//! simply return `None` for both methods – they fall back to the `Cow<str>`
//! allocation path, which is the correct, safe behaviour.

pub mod fold_case;
pub mod lower_case;
pub mod normalization;
pub mod normalize_punctuation;
pub mod normalize_whitespace;
pub mod remove_control_chars;
pub mod remove_diacritics;
pub mod remove_format_controls;
pub mod replace_fullwidth;
pub mod segment_word;
pub mod strip_html;
pub mod strip_markdown;
pub mod unigram_cjk;

use crate::context::Context;
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;
use thiserror::Error;

/// Public error type for every stage.
#[derive(Debug, Error)]
pub enum StageError {
    #[error("Normalization failed at stage `{0}`: {1}")]
    Failed(&'static str, String),

    #[error("Normalization validation failed at stage `{0}`: {1}")]
    Validation(&'static str, String),
}

/// A single normalisation step.
pub trait Stage: Send + Sync {
    /// Human-readable name – used for profiling and error messages.
    fn name(&self) -> &'static str;

    /// Fast pre-check.  Returning `Ok(false)` skips the whole stage.
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError>;

    /// Allocation-aware transformation.  Must always be correct.
    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError>;

    // ──────────────────────────────────────────────────────────────
    //  Zero-allocation static path (used by `ChainedProcess`)
    // ──────────────────────────────────────────────────────────────
    /// Return a trait-object reference to `self` **iff** this stage can be
    /// expressed as a pure character iterator.  The default implementation
    /// returns `None` – stages that cannot fuse simply allocate.
    ///
    /// **Why a reference?**  
    /// `ChainedProcess` is monomorphised, so the compiler knows the concrete
    /// type of every stage.  Returning `&self` gives the compiler a *direct*
    /// pointer to the concrete iterator implementation – no heap, no vtable,
    /// full inlining + loop fusion.
    #[inline]
    fn as_char_mapper(&self, _ctx: &Context) -> Option<&dyn CharMapper> {
        None
    }

    // ──────────────────────────────────────────────────────────────
    //  Dynamic plugin path (used by `DynProcess`)
    // ──────────────────────────────────────────────────────────────
    /// Return an `Arc<dyn CharMapper>` **iff** this stage can be expressed as a
    /// pure character iterator.  The `Arc<Self>` is already owned by `DynProcess`,
    /// so the only cost is the trait-object indirection.
    ///
    /// **Why `Arc`?**  
    /// `DynProcess` stores stages in a `Vec<Arc<dyn Stage>>`.  Converting to
    /// `Arc<dyn CharMapper>` re-uses the same reference count – no extra allocation.
    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, _ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        None
    }
}

/// The heart of fused, zero-allocation pipelines.
pub trait CharMapper: Send + Sync {
    /// Map a single Unicode scalar value.
    /// Return `None` if the character should be **removed**.
    fn map(&self, c: char, ctx: &Context) -> Option<char>;

    /// Bind the mapper to a concrete `&str`.  The returned iterator must be
    /// `FusedIterator` so the compiler can eliminate bounds checks.
    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a>;
}
