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
pub struct ChainedProcess<S: Stage, P: Process> {
    pub stage: S,
    pub previous: P,
}

impl<S: Stage, P: Process> Process for ChainedProcess<S, P> {
    #[inline(always)]
    fn process<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let current: Cow<'_, str> = self.previous.process(text, ctx)?;
        if !self.stage.needs_apply(&current, ctx)? {
            return Ok(current);
        }
        // if let Some(iter) = self.stage.try_static_iter(&current, ctx) {
        //     let mut out = String::with_capacity(current.len());
        //     for c in iter {
        //         out.push(c);
        //     }
        //     return Ok(Cow::Owned(out));
        // }
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
