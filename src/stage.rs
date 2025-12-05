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

    /// You are only called when needs_apply returned true.
    /// You may allocate. You may mutate. You may be slow.
    /// You must never try to "be clever" and return the input unchanged.
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
    /// Note: `CharMapper::map` returning `Some(c)` (identity) is perfectly valid
    /// even when the stage inserts characters (e.g. spaces, ZWSPs). The insertion
    /// logic lives in `bind()`, not `map()`. `map()` is only a **hint** for simple
    /// 1:1 stages — it is **not** required to describe all transformations.
    fn map(&self, c: char, ctx: &Context) -> Option<char>;

    /// Bind the mapper to a concrete `&str`.  The returned iterator must be
    /// `FusedIterator` so the compiler can eliminate bounds checks.
    fn bind<'a>(&self, text: &'a str, ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a>;
}
