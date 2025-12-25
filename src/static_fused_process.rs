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
//
// IMPORTANT CONCEPTS:
//
// 1. CHAIN STRUCTURE (built inside-out):
//    ChainedProcess { stage: D, previous: ChainedProcess { stage: C, previous: ... } }
//    Logical order: A → B → C → D
//    Actual nesting: D wraps C wraps B wraps A
//
// 2. FUSABLE ISLANDS:
//    Given: StripHtml → NFC → Lower → Fold → StripMarkdown → UnifyWidth
//    Islands: [StripHtml] → [NFC, Lower, Fold] → [StripMarkdown] → [UnifyWidth]
//             ^barrier      ^island (size=3)      ^barrier          ^island (size=1)
//
// 3. RECURSIVE TRAVERSAL:
//    When we call methods on stage D with island_size=3:
//    - D is at position 3 (last in island)
//    - D calls previous (C) with island_size=2
//    - C calls previous (B) with island_size=1
//    - B is at position 1 (first in island), stops recursion
//
pub trait StaticFusedProcess {
    /// The type of the fused iterator chain for this island
    ///
    /// Example: For chain A → B → C where all are fusable:
    ///   EmptyProcess: IslandIter = I (identity, just returns input)
    ///   ChainedProcess(A): IslandIter = AAdapter<I>
    ///   ChainedProcess(B): IslandIter = BAdapter<AAdapter<I>>
    ///   ChainedProcess(C): IslandIter = CAdapter<BAdapter<AAdapter<I>>>
    type IslandIter<'a, I>: FusedIterator<Item = char> + 'a
    where
        I: FusedIterator<Item = char> + 'a,
        Self: 'a;

    /// Build the fused iterator chain for ONLY this island
    ///
    /// IMPORTANT: This builds the chain FORWARD (in logical order)
    /// Given stages: A → B → C
    /// Builds: input → AAdapter → BAdapter → CAdapter
    ///
    /// Recursion:
    /// - C.build_island_iter(input):
    ///   - calls B.build_island_iter(input) → gets BAdapter<AAdapter<input>>
    ///   - wraps in CAdapter → returns CAdapter<BAdapter<AAdapter<input>>>
    fn build_island_iter<'a, I>(&self, input: I, ctx: &'a Context) -> Self::IslandIter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    /// Count how many consecutive fusable stages END at this point
    ///
    /// Example chain: StripHtml → NFC → Lower → Fold
    ///                ^not fusable  ^fusable x3
    ///
    /// StripHtml.fusable_chain_len() = 0 (barrier stage)
    /// NFC.fusable_chain_len() = 1 (first fusable)
    /// Lower.fusable_chain_len() = 2 (second fusable)
    /// Fold.fusable_chain_len() = 3 (third fusable) ← We're at the END of island
    fn fusable_chain_len(&self) -> usize;

    /// Process everything BEFORE the current fusable island
    ///
    /// Given: StripHtml → [NFC → Lower → Fold] ← island size=3, we're at Fold
    ///                    ^^^^^^^^^^^^
    ///                    process this part
    ///
    /// We need to:
    /// 1. Skip backward through the island (3 stages)
    /// 2. Process everything before NFC (just StripHtml in this case)
    ///
    /// HOW IT WORKS:
    /// - Fold.process_before_island(text, 3):
    ///   - island_size=3 > 1, so recurse: Lower.process_before_island(text, 2)
    /// - Lower.process_before_island(text, 2):
    ///   - island_size=2 > 1, so recurse: NFC.process_before_island(text, 1)
    /// - NFC.process_before_island(text, 1):
    ///   - island_size=1, we're at FIRST stage of island!
    ///   - Call previous.process_static(text) → processes StripHtml
    fn process_before_island<'a>(
        &self,
        text: &'a str,
        ctx: &Context,
        island_size: usize,
    ) -> Result<Cow<'a, str>, StageError>;

    /// Check if ANY stage in the island needs to apply
    ///
    /// KEY INSIGHT: All stages check against the SAME input (island start)
    ///
    /// Given island: [NFC → Lower → Fold] with input "Café"
    ///
    /// Normal pipeline:
    ///   NFC.needs_apply("Café") → true → output "Café" (normalized)
    ///   Lower.needs_apply("Café") → true → output "café"
    ///   Fold.needs_apply("café") → false → skip (already lowercase)
    ///
    /// Static fusion:
    ///   NFC.needs_apply("Café") → true ✓
    ///   Lower.needs_apply("Café") → true ✓
    ///   Fold.needs_apply("Café") → true ✓ (checks ORIGINAL, not "café"!)
    ///   → ALL THREE get included in iterator chain!
    ///
    /// RULES:
    /// - First stage (island_size=1): Always use needs_apply
    /// - Later stages (island_size>1):
    ///   - If prev_needs=false: check directly (text unchanged)
    ///   - If prev_needs=true AND safe_skip: check anyway (approximation)
    ///   - If prev_needs=true AND !safe_skip: return true (can't check)
    fn island_needs_apply(
        &self,
        text: &str,
        ctx: &Context,
        island_size: usize,
    ) -> Result<bool, StageError>;

    /// Calculate total capacity needed for island output
    ///
    /// Chains the capacity estimates:
    /// Input: 100 bytes
    /// → NFC: 100 * 1.0 = 100
    /// → Lower: 100 * 1.1 = 110 (Turkish 'I' → 'ı' grows)
    /// → Fold: 110 * 1.2 = 132 (ß → ss expansion)
    fn island_capacity(&self, input_len: usize, island_size: usize) -> usize;

    /// Returns a bitmask where the i-th bit corresponds to the i-th stage in the island.
    /// Bit 0 is the FIRST stage, Bit (island_size-1) is the CURRENT stage.
    fn collect_island_needs(
        &self,
        text: &str,
        ctx: &Context,
        island_size: usize,
    ) -> Result<u64, StageError>;

    fn apply_island_sequentially<'a>(
        &self,
        text: Cow<'a, str>,
        ctx: &Context,
        island_size: usize,
        needs_mask: u64,
    ) -> Result<Cow<'a, str>, StageError>;

    /// Main entry point: process text through this pipeline
    ///
    /// ALGORITHM:
    /// 1. Check fusable_chain_len() - are we at the end of an island?
    /// 2. If island_len >= 2:
    ///    a. Process everything before island
    ///    b. Check if island needs work
    ///    c. If yes: build fused iterator, collect result
    ///    d. If no: return borrowed input (zero-copy!)
    /// 3. If single stage or barrier:
    ///    - Use regular apply() path
    fn process_static_fused<'a>(
        &self,
        text: &'a str,
        ctx: &Context,
    ) -> Result<Cow<'a, str>, StageError>;
}

// ============================================================================
// Implementation for EmptyProcess (Base Case)
// ============================================================================
impl StaticFusedProcess for EmptyProcess {
    type IslandIter<'a, I>
        = I
    where
        I: FusedIterator<Item = char> + 'a;

    #[inline(always)]
    fn build_island_iter<'a, I>(&self, input: I, _ctx: &'a Context) -> Self::IslandIter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        // Base case: no stages, return input as-is
        input
    }

    #[inline(always)]
    fn fusable_chain_len(&self) -> usize {
        // Base case: no stages
        0
    }

    #[inline(always)]
    fn process_before_island<'a>(
        &self,
        text: &'a str,
        _ctx: &Context,
        _island_size: usize,
    ) -> Result<Cow<'a, str>, StageError> {
        // Base case: nothing before, return input
        Ok(Cow::Borrowed(text))
    }

    #[inline(always)]
    fn island_needs_apply(
        &self,
        _text: &str,
        _ctx: &Context,
        _island_size: usize,
    ) -> Result<bool, StageError> {
        // Base case: no stages, nothing needs to apply
        Ok(false)
    }

    #[inline(always)]
    fn island_capacity(&self, input_len: usize, _island_size: usize) -> usize {
        // Base case: output size = input size
        input_len
    }

    fn collect_island_needs(
        &self,
        _text: &str,
        _ctx: &Context,
        _island_size: usize,
    ) -> Result<u64, StageError> {
        Ok(0)
    }

    #[inline(always)]
    fn apply_island_sequentially<'a>(
        &self,
        text: Cow<'a, str>,
        _ctx: &Context,
        _island_size: usize,
        _needs: u64,
    ) -> Result<Cow<'a, str>, StageError> {
        // Base case: No more stages in this island to apply
        Ok(text)
    }

    fn process_static_fused<'a>(
        &self,
        text: &'a str,
        _ctx: &Context,
    ) -> Result<Cow<'a, str>, StageError> {
        // Base case: no processing, return input
        Ok(Cow::Borrowed(text))
    }
}

// ============================================================================
// Implementation for ChainedProcess (Recursive Case)
// ============================================================================
impl<S, P> StaticFusedProcess for ChainedProcess<S, P>
where
    S: Stage + StaticFusableStage,
    P: Process + StaticFusedProcess,
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
        // RECURSIVE STEP: Build iterator chain from previous stages
        //
        // Example: We are stage C in chain A → B → C
        // 1. Call previous (B).build_island_iter(input)
        //    → Returns BAdapter<AAdapter<input>>
        // 2. Wrap in our adapter: CAdapter<BAdapter<AAdapter<input>>>
        //
        // Result: input → A → B → C (logical forward order!)
        let prev_iter = self.previous.build_island_iter(input, ctx);
        self.stage.static_fused_adapter(prev_iter, ctx)
    }

    #[inline(always)]
    fn fusable_chain_len(&self) -> usize {
        // Count consecutive fusable stages ending here
        //
        // Example: StripHtml → NFC → Lower → Fold
        //          ^barrier      ^1     ^2      ^3
        //
        // At Fold: supports_static_fusion()=true, so 0 + 1 = 3
        // At Lower: supports_static_fusion()=true, so 0 + 1 = 2
        // At NFC: supports_static_fusion()=true, so 0 + 1 = 1
        // At StripHtml: supports_static_fusion()=false, so 0 (resets!)
        if self.stage.supports_static_fusion() {
            self.previous.fusable_chain_len() + 1
        } else {
            0 // Barrier stage resets the count
        }
    }

    fn process_before_island<'a>(
        &self,
        text: &'a str,
        ctx: &Context,
        island_size: usize,
    ) -> Result<Cow<'a, str>, StageError> {
        // GOAL: Skip backward through island, process everything before it
        //
        // Example: StripHtml → [NFC → Lower → Fold]
        //                      ^^^^^^^^^^^ skip this
        //          ^^^^^^^^^^ process this
        //
        // We are at Fold, island_size=3
        //
        // VISUALIZATION of recursion:
        // Fold.process_before_island(3):
        //   └─> Lower.process_before_island(2):
        //       └─> NFC.process_before_island(1):
        //           └─> (island_size=1!) previous.process_static()
        //               └─> StripHtml.process_static() ← THIS is what we want!

        if island_size <= 1 {
            // BASE CASE: We've walked back to the FIRST stage of island
            // Now process everything BEFORE this stage
            return self.previous.process_static_fused(text, ctx);
        }

        // RECURSIVE CASE: Not at first stage yet, keep walking back
        self.previous
            .process_before_island(text, ctx, island_size - 1)
    }

    fn island_needs_apply(
        &self,
        text: &str,
        ctx: &Context,
        island_size: usize,
    ) -> Result<bool, StageError> {
        // GOAL: Check if ANY stage in island needs work
        //
        // KEY CHALLENGE: All stages check the SAME input (island start),
        // but some stages would normally check transformed text
        //
        // Example: [NFC → Lower → Fold] on "Café"
        // - NFC checks "Café" → true (has combining char)
        // - Lower checks "Café" → true (has uppercase)
        // - Fold checks "Café" → true (but would check "café" normally!)
        //
        // RULES:
        // 1. First stage: always check (it's operating on actual input)
        // 2. Later stages:
        //    - If prev stages don't need work: check directly (input unchanged)
        //    - If prev stages need work AND safe_skip: check anyway (approximate)
        //    - If prev stages need work AND !safe_skip: assume true (can't check)

        if island_size <= 1 {
            // BASE CASE: First stage in island
            // This stage actually receives the island input, so check is accurate
            return self.stage.needs_apply(text, ctx);
        }

        // RECURSIVE CASE: Check all previous stages first
        let prev_needs = self
            .previous
            .island_needs_apply(text, ctx, island_size - 1)?;

        if prev_needs {
            // Previous stages WILL transform the text
            // This stage would receive transformed text, NOT the base text

            if self.stage.safe_skip_approximation() {
                // Stage says: "checking base text is a valid approximation"
                // Example: RemoveDiacritics can check "Café" instead of "café"
                let this_needs = self.stage.needs_apply(text, ctx)?;
                Ok(prev_needs || this_needs) // Return true if EITHER needs work
            } else {
                // Stage says: "I need actual transformed input to check accurately"
                // Example: CaseFold might give different results on "Café" vs "café"
                // We can't check without allocating intermediate → conservatively return true
                Ok(true)
            }
        } else {
            // Previous stages DON'T need to transform
            // This stage WILL receive the original base text unchanged
            // So checking it directly is accurate!
            self.stage.needs_apply(text, ctx)
        }
    }

    fn island_capacity(&self, input_len: usize, island_size: usize) -> usize {
        // GOAL: Estimate total capacity for island output
        //
        // Chain the estimates through each stage:
        // Input: 100 bytes
        // → Stage 1: 100 * 1.1 = 110
        // → Stage 2: 110 * 1.2 = 132
        // → Stage 3: 132 * 1.0 = 132

        if island_size <= 1 {
            // BASE CASE: Just this stage
            self.stage.expected_capacity(input_len)
        } else {
            // RECURSIVE CASE: Previous stages first, then this stage
            let prev_cap = self.previous.island_capacity(input_len, island_size - 1);
            self.stage.expected_capacity(prev_cap)
        }
    }

    fn collect_island_needs(
        &self,
        text: &str,
        ctx: &Context,
        island_size: usize,
    ) -> Result<u64, StageError> {
        if island_size <= 1 {
            // First stage: set bit 0 if it needs apply
            return Ok(if self.stage.needs_apply(text, ctx)? {
                1
            } else {
                0
            });
        }

        // 1. Recurse to get mask from previous stages
        let prev_mask = self
            .previous
            .collect_island_needs(text, ctx, island_size - 1)?;

        // 2. Check this stage (bit index = island_size - 1)
        let this_needs = if prev_mask > 0 {
            // Something before us changed the text
            if self.stage.safe_skip_approximation() {
                self.stage.needs_apply(text, ctx)?
            } else {
                true
            }
        } else {
            // Text is still original
            self.stage.needs_apply(text, ctx)?
        };

        // 3. Set the bit at the current position and return
        if this_needs {
            Ok(prev_mask | (1 << (island_size - 1)))
        } else {
            Ok(prev_mask)
        }
    }

    fn apply_island_sequentially<'a>(
        &self,
        mut text: Cow<'a, str>,
        ctx: &Context,
        island_size: usize,
        needs_mask: u64,
    ) -> Result<Cow<'a, str>, StageError> {
        if island_size == 0 {
            return Ok(text);
        }

        // Recurse to process earlier stages first
        text = self
            .previous
            .apply_island_sequentially(text, ctx, island_size - 1, needs_mask)?;

        // Check the bit for THIS stage
        let bit_index = island_size - 1;
        if (needs_mask & (1 << bit_index)) != 0 {
            text = self.stage.apply(text, ctx)?;
        }

        Ok(text)
    }

    #[inline]
    fn process_static_fused<'a>(
        &self,
        text: &'a str,
        ctx: &Context,
    ) -> Result<Cow<'a, str>, StageError> {
        let chain_len = self.fusable_chain_len();

        if chain_len >= 2 {
            let base_text = self.process_before_island(text, ctx, chain_len)?;
            let needs_mask = self.collect_island_needs(&base_text, ctx, chain_len)?;

            // Use CPU intrinsic to count set bits
            let active_count = needs_mask.count_ones();

            if active_count == 0 {
                return Ok(base_text);
            }

            if active_count == 1 {
                return self.apply_island_sequentially(base_text, ctx, chain_len, needs_mask);
            }

            // ... proceed to Iterator Fusion
            let capacity = self.island_capacity(base_text.len(), chain_len);
            let mut result = String::with_capacity(capacity);
            result.extend(self.build_island_iter(base_text.chars(), ctx));

            return Ok(Cow::Owned(result));
        }

        // Single stage or barrier fallback
        let prev_result = self.previous.process_static_fused(text, ctx)?;
        if !self.stage.needs_apply(&prev_result, ctx)? {
            return Ok(prev_result);
        }
        self.stage.apply(prev_result, ctx)
    }
}
