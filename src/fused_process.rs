use std::{borrow::Cow, iter::FusedIterator};

use crate::{
    context::Context,
    process::{ChainedProcess, EmptyProcess, Process},
    stage::{Stage, StageError, StaticFusableStage},
};

/// Simple optimistic fusion - if it supports fusion, fuse it!
pub trait FusedProcess {
    /// Iterator type for the fused chain starting from this stage
    type Iter<'a, I>: FusedIterator<Item = char> + 'a
    where
        I: FusedIterator<Item = char> + 'a,
        Self: 'a;

    /// Build fused iterator chain
    fn build_iter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::Iter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    /// Process text with fusion
    fn process_fused<'a>(&self, text: &'a str, ctx: &Context) -> Result<Cow<'a, str>, StageError>;
}

// ============================================================================
// EmptyProcess
// ============================================================================

impl FusedProcess for EmptyProcess {
    type Iter<'a, I>
        = I
    where
        I: FusedIterator<Item = char> + 'a;

    #[inline(always)]
    fn build_iter<'a, I>(&self, input: I, _ctx: &'a Context) -> Self::Iter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        input
    }

    #[inline(always)]
    fn process_fused<'a>(&self, text: &'a str, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        Ok(Cow::Borrowed(text))
    }
}

impl<S, P> FusedProcess for ChainedProcess<S, P>
where
    S: Stage + StaticFusableStage,
    P: Process + FusedProcess,
{
    type Iter<'a, I>
        = S::Adapter<'a, P::Iter<'a, I>>
    where
        I: FusedIterator<Item = char> + 'a,
        Self: 'a,
        S: 'a,
        P: 'a;

    #[inline]
    fn build_iter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::Iter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
        S: 'a,
        P: 'a,
    {
        let prev_iter = self.previous.build_iter(input, ctx);
        self.stage.static_fused_adapter(prev_iter, ctx)
    }

    fn process_fused<'a>(&self, text: &'a str, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let prev_result = self.previous.process_fused(text, ctx)?;
        if !self.stage.supports_static_fusion() {
            if !self.stage.needs_apply(&prev_result, ctx)? {
                return Ok(prev_result);
            }
            return self.stage.apply(prev_result, ctx);
        }

        // Then apply only THIS stage via iterator

        let mut result = String::with_capacity(self.stage.expected_capacity(prev_result.len()));
        let iter = self.stage.static_fused_adapter(prev_result.chars(), ctx);
        result.extend(iter);

        if result == prev_result.as_ref() {
            return Ok(prev_result);
        }
        Ok(Cow::Owned(result))
    }

    // fn process_fused<'a>(&self, text: &'a str, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
    //     if !self.stage.supports_static_fusion() {
    //         let prev_result = self.previous.process_fused(text, ctx)?;
    //         if !self.stage.needs_apply(&prev_result, ctx)? {
    //             return Ok(prev_result);
    //         }
    //         return self.stage.apply(prev_result, ctx);
    //     }

    //     // FUSABLE: Process previous stages FIRST (handles barriers correctly)
    //     let prev_result = self.previous.process_fused(text, ctx)?;

    //     // Then apply only THIS stage via iterator
    //     let capacity = prev_result.len();
    //     let mut result = String::with_capacity(capacity);
    //     let iter = self.stage.static_fused_adapter(prev_result.chars(), ctx);
    //     result.extend(iter);

    //     if result == prev_result.as_ref() {
    //         return Ok(prev_result);
    //     }
    //     Ok(Cow::Owned(result))
    // }
}
