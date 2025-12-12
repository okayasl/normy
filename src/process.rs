//! Process abstraction
//! ChainedProcess is monomorphised – the compiler knows the concrete
//! type of every stage.  When a stage returns `Some(&dyn CharMapper)` we
//! call bind once and hand the resulting `FusedIterator` to the next
//! stage.  The Rust compiler inlines every `Iterator::next` and fuses the
//! whole chain into one machine-code loop – zero heap, zero bounds checks.
//! DynamicProcess  is the dynamic fallback.
use crate::{
    context::Context,
    stage::{Stage, StageError, StageIter},
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
pub struct ChainedProcess<S: Stage, P: Process> {
    pub stage: S,
    pub previous: P,
}

impl<S: Stage + StageIter, P: Process> Process for ChainedProcess<S, P> {
    #[inline(always)]
    fn process<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let current: Cow<'_, str> = self.previous.process(text, ctx)?;
        if !self.stage.needs_apply(&current, ctx)? {
            return Ok(current);
        }

        // BEST PATH: Fully static, zero dyn dispatch
        if let Some(iter) = self.stage.try_iter(&current, ctx) {
            let mut out = String::with_capacity(current.len());
            out.extend(iter);
            return Ok(Cow::Owned(out));
        }

        if let Some(mapper) = self.stage.as_char_mapper(ctx) {
            let iter = mapper.bind(&current, ctx);
            let mut out = String::with_capacity(current.len());
            out.extend(iter);
            return Ok(Cow::Owned(out));
        }

        // FALLBACK 2: apply()
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
            let needs_apply = stage.needs_apply(&text, ctx)?;
            if needs_apply {
                text = match stage.clone().into_dyn_char_mapper(ctx) {
                    Some(mapper) => {
                        let mut out = String::with_capacity(text.len());
                        out.extend(mapper.bind(&text, ctx));
                        Cow::Owned(out)
                    }
                    None => stage.apply(text, ctx)?,
                };
            }
        }
        Ok(text)
    }
}
