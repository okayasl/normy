//! Process abstraction
//! ChainedProcess is monomorphised – the compiler knows the concrete
//! type of every stage. The Rust compiler inlines every `Iterator::next` and fuses the
//! whole chain into one machine-code loop – zero heap, zero bounds checks.
//! DynamicProcess  is the dynamic fallback.
use crate::{
    context::Context,
    stage::{Stage, StageError},
};
use smallvec::SmallVec;
use std::{borrow::Cow, sync::Arc};

pub trait Process {
    fn process<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError>;
}

pub struct EmptyProcess;
impl Process for EmptyProcess {
    #[inline(always)]
    fn process<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        Ok(text)
    }
}

// Add to stage.rs or a new island_metadata.rs
/// Pre-computed island metadata (calculated once at build time)
///
/// This eliminates runtime recursion for island analysis!
#[derive(Debug, Clone, Copy)]
pub struct IslandInfo {
    /// Length of fusable island ending at this stage
    /// - 0 = barrier stage (not fusable)
    /// - 1 = single fusable stage (use apply, not fusion)
    /// - 2+ = fusable island (use fusion for single allocation)
    pub island_len: usize,

    /// Pre-computed capacity multiplier for entire island
    /// Example: NFC (×1.0) → Lower (×1.1) → Fold (×1.2) = 1.32 total
    /// Usage: output_capacity = input_len × capacity_multiplier
    pub capacity_multiplier: f64,

    /// Whether ALL stages in this island have safe_skip_approximation=true
    /// If true: all stages can check needs_apply on original input
    /// If false: some stages need transformed input to check accurately
    pub all_safe_skip: bool,
}

impl Default for IslandInfo {
    fn default() -> Self {
        Self {
            island_len: 0,
            capacity_multiplier: 1.0,
            all_safe_skip: true,
        }
    }
}

impl IslandInfo {
    /// Create info for a new island starting with this stage
    pub fn new_island(stage_capacity_factor: f64, stage_safe_skip: bool) -> Self {
        Self {
            island_len: 1,
            capacity_multiplier: stage_capacity_factor,
            all_safe_skip: stage_safe_skip,
        }
    }

    /// Extend existing island with this stage
    pub fn extend_island(
        prev: IslandInfo,
        stage_capacity_factor: f64,
        stage_safe_skip: bool,
    ) -> Self {
        Self {
            island_len: prev.island_len + 1,
            capacity_multiplier: prev.capacity_multiplier * stage_capacity_factor,
            all_safe_skip: prev.all_safe_skip && stage_safe_skip,
        }
    }

    /// Reset island (barrier stage)
    pub fn reset() -> Self {
        Self::default()
    }
}

pub struct ChainedProcess<S: Stage, P: Process> {
    pub stage: S,
    pub previous: P,

    /// Pre-computed island metadata - calculated once at build time!
    /// This eliminates recursive calls at runtime for massive performance gain.
    pub island_info: IslandInfo,
}

impl<S: Stage, P: Process> Process for ChainedProcess<S, P> {
    #[inline(always)]
    fn process<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let current: Cow<'_, str> = self.previous.process(text, ctx)?;
        if !self.stage.needs_apply(&current, ctx)? {
            return Ok(current);
        }
        self.stage.apply(current, ctx)
    }
}

#[derive(Default)]
pub struct DynamicProcess {
    pub(crate) stages: SmallVec<[Arc<dyn Stage + Send + Sync>; 12]>,
}

impl DynamicProcess {
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

impl Process for DynamicProcess {
    #[inline(always)]
    fn process<'a>(
        &self,
        mut text: Cow<'a, str>,
        ctx: &Context,
    ) -> Result<Cow<'a, str>, StageError> {
        for stage in &self.stages {
            if !stage.needs_apply(&text, ctx)? {
                continue;
            }
            text = stage.apply(text, ctx)?;
        }
        Ok(text)
    }
}
