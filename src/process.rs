use crate::{
    context::Context,
    stage::{Stage, StageError, StaticFusableStage},
};
use smallvec::SmallVec;
use std::{borrow::Cow, iter::FusedIterator, sync::Arc};

pub trait Process {
    fn process<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError>;
}

pub trait BuildIter: Process {
    type Iter<'a, I>: FusedIterator<Item = char> + 'a
    where
        I: FusedIterator<Item = char> + 'a,
        Self: 'a;

    /// Check if any stage in this chain needs to apply
    /// All stages check against original input
    /// If one needs to work all works
    fn any_needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError>;

    fn build_iter<'a, I>(&'a self, input: I, ctx: &'a Context) -> Self::Iter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    fn process_fused<'a>(
        &'a self,
        text: Cow<'a, str>,
        ctx: &Context,
    ) -> Result<Cow<'a, str>, StageError> {
        if !self.any_needs_apply(&text, ctx)? {
            return Ok(text);
        }
        let mut result = String::with_capacity(text.len());
        result.extend(self.build_iter(text.chars(), ctx));
        Ok(Cow::Owned(result))
    }
}

pub struct EmptyProcess;

impl Process for EmptyProcess {
    #[inline(always)]
    fn process<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        Ok(text)
    }
}

impl BuildIter for EmptyProcess {
    type Iter<'a, I>
        = I
    where
        I: FusedIterator<Item = char> + 'a;

    #[inline(always)]
    fn build_iter<'a, I>(&'a self, input: I, _ctx: &'a Context) -> Self::Iter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        input
    }

    #[inline(always)]
    fn any_needs_apply(&self, _text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(false)
    }
}

pub struct ChainedProcess<S, P> {
    pub stage: S,
    pub previous: P,
}

// 1. General implementation: Always works for any Stage and Process
impl<S: Stage, P: Process> Process for ChainedProcess<S, P> {
    fn process<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let current = self.previous.process(text, ctx)?;
        if !self.stage.needs_apply(&current, ctx)? {
            return Ok(current);
        }
        self.stage.apply(current, ctx)
    }
}

// Fused implementation: ONLY exists if S is StaticFusable and P is BuildIter
impl<S, P> BuildIter for ChainedProcess<S, P>
where
    S: Stage + StaticFusableStage,
    P: BuildIter,
{
    type Iter<'a, I>
        = S::Adapter<'a, P::Iter<'a, I>>
    where
        I: FusedIterator<Item = char> + 'a,
        Self: 'a,
        S: 'a,
        P: 'a;

    #[inline]
    fn build_iter<'a, I>(&'a self, input: I, ctx: &'a Context) -> Self::Iter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
        S: 'a,
        P: 'a,
    {
        let prev_iter = self.previous.build_iter(input, ctx);
        self.stage.static_fused_adapter(prev_iter, ctx)
    }

    #[inline(always)]
    fn any_needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        // Check previous stages first
        if self.previous.any_needs_apply(text, ctx)? {
            return Ok(true);
        }

        // Check this stage (safe because safe_skip_approximation=true)
        self.stage.needs_apply(text, ctx)
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
            if stage.needs_apply(&text, ctx)? {
                text = stage.apply(text, ctx)?;
            }
        }
        Ok(text)
    }
}
