// ============================================================================
// process_fused.rs - Iterator fusion with zero-copy optimization
// ============================================================================

use crate::{
    context::Context,
    stage::{Stage, StageError},
};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

// ============================================================================
// ProcessFused Trait - Returns Cow for zero-copy when possible
// ============================================================================

pub trait ProcessFused {
    /// Process text using iterator fusion where possible
    /// Returns Cow::Borrowed when no changes needed (zero allocation!)
    fn process_fused<'a>(&self, text: &'a str, ctx: &Context) -> Result<Cow<'a, str>, StageError>;

    /// Internal: Collect all stages in this chain (used for fusion analysis)
    fn collect_stages<'a>(&'a self, out: &mut Vec<&'a dyn Stage>);
}

// ============================================================================
// EmptyProcessFused - Base case
// ============================================================================

pub struct EmptyProcessFused;

impl ProcessFused for EmptyProcessFused {
    #[inline(always)]
    fn process_fused<'a>(&self, text: &'a str, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        Ok(Cow::Borrowed(text)) // Zero allocation!
    }

    fn collect_stages<'a>(&'a self, _out: &mut Vec<&'a dyn Stage>) {
        // Base case: nothing to add
    }
}

// ============================================================================
// ChainedProcessFused - Recursive chain
// ============================================================================

pub struct ChainedProcessFused<S, P> {
    pub stage: S,
    pub previous: P,
}

impl<S, P> ProcessFused for ChainedProcessFused<S, P>
where
    S: Stage,
    P: ProcessFused,
{
    fn process_fused<'a>(&self, text: &'a str, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // Collect all stages in order
        let mut stages = Vec::new();
        self.collect_stages(&mut stages);

        // Process with fusion
        fuse_and_process(text, &stages, ctx)
    }

    fn collect_stages<'a>(&'a self, out: &mut Vec<&'a dyn Stage>) {
        // Recursively collect previous stages first
        self.previous.collect_stages(out);
        // Then add this stage
        out.push(&self.stage);
    }
}

// ============================================================================
// Core Fusion Algorithm - Zero-copy optimized
// ============================================================================

fn fuse_and_process<'a>(
    text: &'a str,
    stages: &[&dyn Stage],
    ctx: &Context,
) -> Result<Cow<'a, str>, StageError> {
    // Fast path: no stages
    if stages.is_empty() {
        return Ok(Cow::Borrowed(text));
    }

    let mut current = Cow::Borrowed(text);
    let mut i = 0;

    while i < stages.len() {
        // Find next fusable segment
        let segment_end = find_fusable_end(stages, i);

        if segment_end > i {
            // We have fusable stages - process as segment
            current = process_fusable_segment(current, &stages[i..segment_end], ctx)?;
            i = segment_end;
        } else {
            // Single non-fusable stage
            let stage = stages[i];
            if stage.needs_apply(current.as_ref(), ctx)? {
                current = stage.apply(current, ctx)?;
            }
            // If doesn't need apply, current stays the same (might still be Borrowed!)
            i += 1;
        }
    }

    Ok(current)
}

/// Find where the fusable segment ends
fn find_fusable_end(stages: &[&dyn Stage], start: usize) -> usize {
    let mut end = start;
    while end < stages.len() && stages[end].safe_skip_approximation() {
        end += 1;
    }
    end
}

/// Process a fusable segment with single allocation (or zero if no work needed!)
fn process_fusable_segment<'a>(
    input: Cow<'a, str>,
    segment: &[&dyn Stage],
    ctx: &Context,
) -> Result<Cow<'a, str>, StageError> {
    if segment.is_empty() {
        return Ok(input);
    }

    let text = input.as_ref();

    // Check which stages need to apply (all on same input text!)
    let mut active: Vec<&dyn Stage> = Vec::new();
    for stage in segment {
        if stage.needs_apply(text, ctx)? {
            active.push(*stage);
        }
    }

    // ZERO-COPY FAST PATH: No stages need to apply!
    if active.is_empty() {
        return Ok(input); // Return input as-is (might be Borrowed!)
    }

    // Build fused iterator chain
    let mut iter: Box<dyn FusedIterator<Item = char>> = Box::new(text.chars());

    for stage in active {
        if let Some(fusable) = stage.as_fusable() {
            iter = fusable.dyn_fused_adapter(iter, ctx);
        } else {
            // Stage claims safe_skip_approximation but isn't fusable?
            // Fall back to apply
            let s: String = iter.collect();
            return stage.apply(Cow::Owned(s), ctx);
        }
    }

    // SINGLE ALLOCATION - collect entire fused chain
    let result: String = iter.collect();

    // Optional optimization: Check if result is same as input
    // This can save memory when transformation is a no-op despite needs_apply returning true
    if result == text {
        Ok(input) // Return original (possibly Borrowed)
    } else {
        Ok(Cow::Owned(result))
    }
}

// ============================================================================
// DynamicProcessFused - For dynamic/runtime stage composition
// ============================================================================

#[derive(Default)]
pub struct DynamicProcessFused {
    pub(crate) stages: SmallVec<[Arc<dyn Stage + Send + Sync>; 12]>,
}

impl DynamicProcessFused {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub fn push<T: Stage + Send + Sync + 'static>(mut self, stage: T) -> Self {
        self.stages.push(Arc::new(stage));
        self
    }
}

impl ProcessFused for DynamicProcessFused {
    fn process_fused<'a>(&self, text: &'a str, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // Convert Arc stages to references with explicit cast
        let stage_refs: Vec<&dyn Stage> = self
            .stages
            .iter()
            .map(|s| s.as_ref() as &dyn Stage)
            .collect();

        // Use the same fusion algorithm
        fuse_and_process(text, &stage_refs, ctx)
    }

    fn collect_stages<'a>(&'a self, out: &mut Vec<&'a dyn Stage>) {
        for stage in &self.stages {
            out.push(stage.as_ref() as &dyn Stage);
        }
    }
}

// ============================================================================
// Performance characteristics with zero-copy optimization
// ============================================================================

/*
EXAMPLE 1: No changes needed
-----------------------------
Pipeline: [RemoveDiacritics, CaseFold]
Input: "already clean" (no diacritics, all lowercase)

Execution:
1. Check RemoveDiacritics.needs_apply("already clean") → false
2. Check CaseFold.needs_apply("already clean") → false
3. active = []
4. Return Cow::Borrowed("already clean")

Allocations: 0 ✅ ZERO ALLOCATION!


EXAMPLE 2: All fusable with changes
------------------------------------
Pipeline: [TRIM, RemoveDiacritics, CaseFold]
Input: "  Café  "

Execution:
1. Segment [TRIM, RemoveDiacritics, CaseFold] - all fusable
2. Check all needs_apply on "  Café  " → all true
3. Build iterator chain, collect → "cafe"
4. Compare "cafe" != "  Café  " → return Cow::Owned("cafe")

Allocations: 1 ✅ SINGLE ALLOCATION!


EXAMPLE 3: Mixed pipeline
--------------------------
Pipeline: [TRIM, StripHtml, RemoveDiacritics, CaseFold]
Input: "  already clean  " (no HTML, no diacritics, lowercase)

Execution:
1. Segment [TRIM]:
   - TRIM.needs_apply("  already clean  ") → true
   - Build iterator, collect → "already clean"
   - current = Cow::Owned("already clean")

2. Stage [StripHtml]:
   - StripHtml.needs_apply("already clean") → false
   - current stays Cow::Owned("already clean") (no allocation!)

3. Segment [RemoveDiacritics, CaseFold]:
   - RemoveDiacritics.needs_apply("already clean") → false
   - CaseFold.needs_apply("already clean") → false
   - active = []
   - Return current (Cow::Owned("already clean"))

Allocations: 1 (only for TRIM) ✅


EXAMPLE 4: Completely unchanged
--------------------------------
Pipeline: [RemoveDiacritics, CaseFold, StripControlChars]
Input: "hello" (perfect input)

Execution:
1. Segment [RemoveDiacritics, CaseFold, StripControlChars]:
   - All needs_apply("hello") → false
   - active = []
   - Return Cow::Borrowed("hello")

Allocations: 0 ✅ ZERO ALLOCATION - ZERO COPY!


COMPARISON TABLE:
================

| Scenario | Regular normalize() | Fused normalize_fused() | Improvement |
|----------|-------------------|------------------------|-------------|
| No changes | 0 (Borrowed) | 0 (Borrowed) | Same ✓ |
| All need work (3 stages) | 3 allocations | 1 allocation | 67% ↓ |
| Mixed (some need work) | 2-3 allocations | 1-2 allocations | 33-50% ↓ |
| Partial changes | Variable | Minimal | Better ✓ |

KEY INSIGHT:
The fused path now has the SAME zero-copy behavior as the regular path
when no changes are needed, PLUS the fusion benefits when changes are needed!
*/
