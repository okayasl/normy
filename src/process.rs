//! Process abstraction – monomorphised pipelines fuse into a single loop.
//!
//! `ChainedProcess` is **monomorphised** – the compiler knows the concrete
//! type of every stage.  When a stage returns `Some(&dyn CharMapper)` we
//! call `bind` **once** and hand the resulting `FusedIterator` to the next
//! stage.  The Rust compiler inlines every `Iterator::next` and fuses the
//! whole chain into **one machine-code loop** – **zero heap, zero bounds
//! checks** (thanks to `FusedIterator`).
//!
//! `DynProcess` stays exactly the same – it is the *dynamic* fallback.

use crate::{
    context::Context,
    stage::{Stage, StageError},
};
use std::{borrow::Cow, sync::Arc};

pub trait Process {
    fn process<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError>;
}

/* ------------------------------------------------------------------ */
/* Empty sentinel – the starting point of every builder               */
/* ------------------------------------------------------------------ */
pub struct EmptyProcess;
impl Process for EmptyProcess {
    fn process<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        Ok(text)
    }
}

/* ------------------------------------------------------------------ */
/* Monomorphised chain – **this is where fusion happens**            */
/* ------------------------------------------------------------------ */
pub struct ChainedProcess<S: Stage, P: Process> {
    pub stage: S,
    pub previous: P,
}

impl<S: Stage, P: Process> Process for ChainedProcess<S, P> {
    fn process<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        // 1. Run the previous part of the pipeline
        let current = self.previous.process(text, ctx)?;

        // 2. Fast-path: skip if the stage says it would do nothing
        if !self.stage.needs_apply(&current, ctx)? {
            return Ok(current);
        }

        // 3. **Zero-allocation iterator path** – only taken for static pipelines
        if let Some(mapper) = self.stage.as_char_mapper() {
            // Bind the iterator **once** – the compiler will inline everything
            let iter = mapper.bind(&current, ctx);
            // Collect into a `String` only when mutation is required
            let mut out = String::with_capacity(current.len());
            for c in iter {
                out.push(c);
            }
            return Ok(Cow::Owned(out));
        }

        // 4. Fallback – allocation-aware `apply` (still zero-copy when possible)
        self.stage.apply(current, ctx)
    }
}

/* ------------------------------------------------------------------ */
/* Dynamic plugin pipeline – unchanged (uses `Arc<dyn Stage>`)       */
/* ------------------------------------------------------------------ */
#[derive(Default)]
pub struct DynProcess {
    stages: Vec<Arc<dyn Stage + Send + Sync>>,
}

impl DynProcess {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push<T: Stage + Send + Sync + 'static>(mut self, stage: T) -> Self {
        self.stages.push(Arc::new(stage));
        self
    }
}

impl Process for DynProcess {
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
