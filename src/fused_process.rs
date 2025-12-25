use std::{borrow::Cow, iter::FusedIterator};

use crate::{
    context::Context,
    process::{ChainedProcess, EmptyProcess, IslandInfo, Process},
    stage::{Stage, StageError, StaticFusableStage},
};

pub trait ProcessIslandInfo {
    fn get_island_info(&self) -> IslandInfo;
}

impl ProcessIslandInfo for EmptyProcess {
    fn get_island_info(&self) -> IslandInfo {
        IslandInfo::default()
    }
}

impl<S: Stage, P: Process> ProcessIslandInfo for ChainedProcess<S, P> {
    fn get_island_info(&self) -> IslandInfo {
        self.island_info
    }
}

// pub struct FusionChainedProcess<S: Stage, P: Process> {
//     pub stage: S,
//     pub previous: P,

//     /// Pre-computed island metadata - calculated once at build time!
//     /// This eliminates recursive calls at runtime for massive performance gain.
//     pub island_info: IslandInfo,
// }

pub trait FusedProcess {
    /// Iterator type for the fusable island starting from this stage
    type IslandIter<'a, I>: FusedIterator<Item = char> + 'a
    where
        I: FusedIterator<Item = char> + 'a,
        Self: 'a;

    /// Builds an iterator for ONLY the fusable stages starting from this point
    fn build_island_iter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::IslandIter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    /// Returns the number of consecutive fusable stages ending at this point
    fn fusable_chain_len(&self) -> usize;

    /// Process everything BEFORE the current fusable island
    fn process_before_island<'a>(
        &self,
        text: &'a str,
        ctx: &Context,
        island_size: usize,
    ) -> Result<Cow<'a, str>, StageError>;

    /// Check if the island needs to apply
    fn island_needs_apply(
        &self,
        text: &str,
        ctx: &Context,
        island_size: usize,
    ) -> Result<bool, StageError>;

    /// Calculate capacity for the island
    fn island_capacity(&self, input_len: usize, island_size: usize) -> usize;

    // ========================================================================
    // 🆕 NEW METHODS (added for single-stage optimization)
    // ========================================================================

    /// Collect needs_apply results for all stages in the island
    ///
    /// Returns Vec where index 0 = first stage, last index = last stage
    /// Example: [NFC → Lower → Fold] might return [true, true, false]
    ///
    /// Used to determine if only 1 stage needs work (optimization)
    fn collect_island_needs(
        &self,
        text: &str,
        ctx: &Context,
        island_size: usize,
    ) -> Result<Vec<bool>, StageError>;

    /// Apply only the stages that need work sequentially (no fusion)
    ///
    /// Used when only 1 stage in island needs work - no benefit to fusion!
    /// This calls apply() directly on that one stage, avoiding iterator overhead.
    ///
    /// # Arguments
    /// - `needs`: Vec from collect_island_needs
    /// - `current_index`: Position in needs vec (for recursion tracking)
    fn apply_island_sequentially<'a>(
        &self,
        text: Cow<'a, str>,
        ctx: &Context,
        island_size: usize,
        needs: &[bool],
        current_index: usize,
    ) -> Result<Cow<'a, str>, StageError>;

    /// Main entry point: process text through this pipeline
    fn process_fused<'a>(&self, text: &'a str, ctx: &Context) -> Result<Cow<'a, str>, StageError>;
}

impl FusedProcess for EmptyProcess {
    type IslandIter<'a, I>
        = I
    where
        I: FusedIterator<Item = char> + 'a;

    #[inline(always)]
    fn build_island_iter<'a, I>(&self, input: I, _ctx: &'a Context) -> Self::IslandIter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        input
    }

    #[inline(always)]
    fn fusable_chain_len(&self) -> usize {
        0
    }

    #[inline(always)]
    fn process_before_island<'a>(
        &self,
        text: &'a str,
        _ctx: &Context,
        _island_size: usize,
    ) -> Result<Cow<'a, str>, StageError> {
        Ok(Cow::Borrowed(text))
    }

    #[inline(always)]
    fn island_needs_apply(
        &self,
        _text: &str,
        _ctx: &Context,
        _island_size: usize,
    ) -> Result<bool, StageError> {
        Ok(false)
    }

    #[inline(always)]
    fn island_capacity(&self, input_len: usize, _island_size: usize) -> usize {
        input_len
    }

    fn collect_island_needs(
        &self,
        _text: &str,
        _ctx: &Context,
        _island_size: usize,
    ) -> Result<Vec<bool>, StageError> {
        Ok(Vec::new())
    }

    fn apply_island_sequentially<'a>(
        &self,
        text: Cow<'a, str>,
        _ctx: &Context,
        _island_size: usize,
        _needs: &[bool],
        _current_index: usize,
    ) -> Result<Cow<'a, str>, StageError> {
        Ok(text)
    }

    fn process_fused<'a>(&self, text: &'a str, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        Ok(Cow::Borrowed(text))
    }
}

impl<S, P> FusedProcess for ChainedProcess<S, P>
where
    S: Stage + StaticFusableStage,
    P: Process + FusedProcess,
{
    type IslandIter<'a, I>
        = S::Adapter<'a, P::IslandIter<'a, I>>
    where
        I: FusedIterator<Item = char> + 'a,
        Self: 'a,
        P::IslandIter<'a, I>: FusedIterator<Item = char>;

    #[inline]
    fn build_island_iter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::IslandIter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
        S: 'a,
        P: 'a,
    {
        let prev_iter = self.previous.build_island_iter(input, ctx);
        self.stage.static_fused_adapter(prev_iter, ctx)
    }

    #[inline(always)]
    fn fusable_chain_len(&self) -> usize {
        // 🚀 O(1) field access instead of O(n) recursion!
        self.island_info.island_len
    }

    fn process_before_island<'a>(
        &self,
        text: &'a str,
        ctx: &Context,
        island_size: usize,
    ) -> Result<Cow<'a, str>, StageError> {
        // Debug assertion: caller should pass correct island_size
        debug_assert_eq!(island_size, self.island_info.island_len);

        if island_size <= 1 {
            // BASE CASE: We're at first stage of island
            return self.previous.process_fused(text, ctx);
        }

        // RECURSIVE CASE: Keep walking back through island
        self.previous
            .process_before_island(text, ctx, island_size - 1)
    }

    fn island_needs_apply(
        &self,
        text: &str,
        ctx: &Context,
        island_size: usize,
    ) -> Result<bool, StageError> {
        debug_assert_eq!(island_size, self.island_info.island_len);

        // 🚀 OPTIMIZATION: If all stages have safe_skip, can use faster check!
        if self.island_info.all_safe_skip {
            // All stages can check on base text - just check this one
            // If ANY previous returned true, we already know island needs work
            if island_size <= 1 {
                return self.stage.needs_apply(text, ctx);
            }

            let prev_needs = self
                .previous
                .island_needs_apply(text, ctx, island_size - 1)?;
            let this_needs = self.stage.needs_apply(text, ctx)?;
            return Ok(prev_needs || this_needs);
        }

        // Standard logic for mixed safe_skip values
        if island_size <= 1 {
            return self.stage.needs_apply(text, ctx);
        }

        let prev_needs = self
            .previous
            .island_needs_apply(text, ctx, island_size - 1)?;

        if prev_needs {
            if self.stage.safe_skip_approximation() {
                let this_needs = self.stage.needs_apply(text, ctx)?;
                Ok(prev_needs || this_needs)
            } else {
                Ok(true)
            }
        } else {
            self.stage.needs_apply(text, ctx)
        }
    }

    fn island_capacity(&self, input_len: usize, island_size: usize) -> usize {
        debug_assert_eq!(island_size, self.island_info.island_len);

        // 🚀 O(1) multiplication instead of O(n) recursion!
        (input_len as f64 * self.island_info.capacity_multiplier).ceil() as usize
    }

    fn collect_island_needs(
        &self,
        text: &str,
        ctx: &Context,
        island_size: usize,
    ) -> Result<Vec<bool>, StageError> {
        debug_assert_eq!(island_size, self.island_info.island_len);

        if island_size <= 1 {
            let needs = self.stage.needs_apply(text, ctx)?;
            return Ok(vec![needs]);
        }

        let mut needs = self
            .previous
            .collect_island_needs(text, ctx, island_size - 1)?;

        let this_needs = if needs.iter().any(|&x| x) {
            if self.stage.safe_skip_approximation() {
                self.stage.needs_apply(text, ctx)?
            } else {
                true
            }
        } else {
            self.stage.needs_apply(text, ctx)?
        };

        needs.push(this_needs);
        Ok(needs)
    }

    fn apply_island_sequentially<'a>(
        &self,
        mut text: Cow<'a, str>,
        ctx: &Context,
        island_size: usize,
        needs: &[bool],
        current_index: usize,
    ) -> Result<Cow<'a, str>, StageError> {
        if island_size <= 1 {
            if needs[current_index] {
                return self.stage.apply(text, ctx);
            } else {
                return Ok(text);
            }
        }

        text = self.previous.apply_island_sequentially(
            text,
            ctx,
            island_size - 1,
            needs,
            current_index,
        )?;

        if needs[current_index + island_size - 1] {
            text = self.stage.apply(text, ctx)?;
        }

        Ok(text)
    }

    #[inline]
    fn process_fused<'a>(&self, text: &'a str, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // 🚀 Direct field access - no function call overhead!
        let chain_len = self.island_info.island_len;

        // CASE 1: Island with 2+ stages
        if chain_len >= 2 {
            // STEP 1: Process everything before this island
            let base_text = self.process_before_island(text, ctx, chain_len)?;

            // STEP 2: Analyze which stages need work
            let needs = self.collect_island_needs(&base_text, ctx, chain_len)?;
            let active_count = needs.iter().filter(|&&x| x).count();

            // OPTIMIZATION 1: No work needed → zero-copy! 🚀
            if active_count == 0 {
                return Ok(base_text);
            }

            // OPTIMIZATION 2: Only 1 stage needs work → use apply()! 🎯
            // No benefit to fusion with single stage - just call apply directly
            if active_count == 1 {
                return self.apply_island_sequentially(base_text, ctx, chain_len, &needs, 0);
            }

            // OPTIMIZATION 3: 2+ stages need work → fusion for single allocation! ⚡
            // 🚀 Use pre-computed capacity!
            let capacity = self.island_capacity(base_text.len(), chain_len);
            let mut result = String::with_capacity(capacity);

            let iter = self.build_island_iter(base_text.chars(), ctx);
            result.extend(iter);

            // SAFETY NET: Handle false positives
            if result == base_text.as_ref() {
                return Ok(base_text);
            }

            return Ok(Cow::Owned(result));
        }

        // CASE 2: Single stage or barrier - use apply() path
        let prev_result = self.previous.process_fused(text, ctx)?;

        if !self.stage.needs_apply(&prev_result, ctx)? {
            return Ok(prev_result);
        }

        self.stage.apply(prev_result, ctx)
    }
}
