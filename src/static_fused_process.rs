// ============================================================================
// static_fused_process.rs - Static fusion with proper trait bounds
// ============================================================================

use crate::{
    context::Context,
    process::{ChainedProcess, EmptyProcess, Process},
    stage::{Stage, StageError, StaticFusableStage},
};
use std::borrow::Cow;
use std::iter::FusedIterator;

// ============================================================================
// StaticFusedProcess Trait
// ============================================================================

pub trait StaticFusedProcess {
    type FullIter<'a, I>: FusedIterator<Item = char> + 'a
    where
        I: FusedIterator<Item = char> + 'a,
        Self: 'a;

    fn build_fused_iter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::FullIter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    fn needs_any_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError>;

    /// Recursive capacity calculation
    fn total_expected_capacity(&self, input_len: usize) -> usize;

    fn process_static<'a>(&self, text: &'a str, ctx: &Context) -> Result<Cow<'a, str>, StageError>;

    /// Returns true if every stage from here to the root supports static fusion.
    fn can_fuse_to_root(&self) -> bool;
}

// ============================================================================
// Implementation for EmptyProcess
// ============================================================================

impl StaticFusedProcess for EmptyProcess {
    type FullIter<'a, I>
        = I
    where
        I: FusedIterator<Item = char> + 'a;

    #[inline(always)]
    fn build_fused_iter<'a, I>(&self, input: I, _ctx: &'a Context) -> Self::FullIter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        input
    }

    #[inline(always)]
    fn needs_any_apply(&self, _text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(false)
    }
    #[inline(always)]
    fn total_expected_capacity(&self, input_len: usize) -> usize {
        input_len
    }

    fn process_static<'a>(
        &self,
        text: &'a str,
        _ctx: &Context,
    ) -> Result<Cow<'a, str>, StageError> {
        Ok(Cow::Borrowed(text))
    }

    #[inline(always)]
    fn can_fuse_to_root(&self) -> bool {
        true
    }
}

// ============================================================================
// Implementation for ChainedProcess
// ============================================================================

impl<S, P> StaticFusedProcess for ChainedProcess<S, P>
where
    S: Stage + StaticFusableStage,
    P: Process + StaticFusedProcess,
{
    type FullIter<'a, I>
        = S::Adapter<'a, P::FullIter<'a, I>>
    where
        I: FusedIterator<Item = char> + 'a,
        Self: 'a,
        // Explicitly state that P::FullIter is FusedIterator
        // This is technically redundant (the trait already says this)
        // but helps the compiler understand in this context
        P::FullIter<'a, I>: FusedIterator<Item = char>;

    #[inline]
    fn build_fused_iter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::FullIter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
        S: 'a, // S must live long enough
        P: 'a, // P must live long enough
    {
        let prev_iter = self.previous.build_fused_iter(input, ctx);
        self.stage.static_fused_adapter(prev_iter, ctx)
    }

    fn needs_any_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        if self.previous.needs_any_apply(text, ctx)? {
            return Ok(true);
        }

        if self.stage.safe_skip_approximation() {
            self.stage.needs_apply(text, ctx)
        } else {
            Ok(true)
        }
    }

    #[inline(always)]
    fn total_expected_capacity(&self, input_len: usize) -> usize {
        // 1. Get capacity from previous stages
        let prev_cap = self.previous.total_expected_capacity(input_len);
        // 2. Let the current stage adjust it
        self.stage.expected_capacity(prev_cap)
    }

    #[inline(always)]
    fn can_fuse_to_root(&self) -> bool {
        self.stage.supports_static_fusion() && self.previous.can_fuse_to_root()
    }

    #[inline]
    fn process_static<'a>(&self, text: &'a str, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // --- CASE 1: THIS STAGE IS A BARRIER ---
        if !self.stage.supports_static_fusion() {
            // Force the previous pipeline to finish completely
            let prev_result = self.previous.process_static(text, ctx)?;

            // Apply this non-fusable stage to the result of the previous pipeline
            if self.stage.needs_apply(prev_result.as_ref(), ctx)? {
                return self.stage.apply(prev_result, ctx);
            }
            return Ok(prev_result);
        }

        // --- CASE 2: PREVIOUS PIPELINE HAS A BARRIER ---
        // If we can't fuse all the way to the root, we must split the execution.
        if !self.can_fuse_to_root() {
            // 1. Process the previous island
            let prev_result = self.previous.process_static(text, ctx)?;

            // 2. Check if this current stage even needs to run on the new text
            if !self.stage.needs_apply(prev_result.as_ref(), ctx)? {
                return Ok(prev_result);
            }

            // 3. Start a NEW fusion island beginning at this stage
            let cap = self.stage.expected_capacity(prev_result.len());
            let mut result = String::with_capacity(cap);

            // Build an adapter that only wraps this stage, starting from prev_result
            let iter = self.stage.static_fused_adapter(prev_result.chars(), ctx);
            result.extend(iter);

            return Ok(Cow::Owned(result));
        }

        // --- CASE 3: PURE FUSION (No barriers in the whole chain) ---
        if !self.needs_any_apply(text, ctx)? {
            return Ok(Cow::Borrowed(text));
        }

        let cap = self.total_expected_capacity(text.len());
        let mut result = String::with_capacity(cap);
        let iter = self.build_fused_iter(text.chars(), ctx);
        result.extend(iter);

        Ok(Cow::Owned(result))
    }
}
