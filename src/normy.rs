use crate::{
    context::Context,
    lang::{DEFAULT_LANG, Lang, LangEntry},
    process::{ChainedProcess, DynamicProcess, EmptyProcess, FusablePipeline, Process},
    stage::{Stage, StageError, StaticFusableStage},
};
use smallvec::SmallVec;
use std::{borrow::Cow, sync::Arc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NormyError {
    #[error(transparent)]
    Stage(#[from] StageError),
}

/// # Safety: `text` **must** be valid UTF-8.
#[cfg(debug_assertions)]
#[inline(always)]
fn assert_utf8(text: &str) {
    debug_assert!(std::str::from_utf8(text.as_bytes()).is_ok());
}

// ============================================================================
// Normy - Smart routing based on runtime flag
// ============================================================================
pub struct Normy<P: Process> {
    ctx: Context,
    pipeline: P,
    all_fusable: bool,
    stage_count: usize,
}

impl<P: Process> Normy<P> {
    #[inline(always)]
    pub fn uses_fusion(&self) -> bool {
        self.all_fusable && self.stage_count > 1
    }
}

impl<P: FusablePipeline> Normy<P> {
    /// Normalize text using the fastest available execution strategy.
    /// Automatically uses fusion when supported.
    #[inline(always)]
    pub fn normalize<'a>(&'a self, text: &'a str) -> Result<Cow<'a, str>, NormyError> {
        #[cfg(debug_assertions)]
        assert_utf8(text);
        if self.uses_fusion() {
            // Use fusion path
            self.pipeline
                .process_fused(Cow::Borrowed(text), &self.ctx)
                .map_err(Into::into)
        } else {
            // Use apply path (faster for single/zero stages)
            self.pipeline
                .process(Cow::Borrowed(text), &self.ctx)
                .map_err(Into::into)
        }
    }

    /// Normalize text **without fusion**.
    ///
    /// This forces full materialization at each stage and disables
    /// iterator fusion, even when the pipeline supports it.
    ///
    /// Useful for:
    /// - Benchmark comparisons
    /// - Debugging stage behavior
    /// - Semantic validation
    #[inline(always)]
    pub fn normalize_no_fusion<'a>(&'a self, text: &'a str) -> Result<Cow<'a, str>, NormyError> {
        #[cfg(debug_assertions)]
        assert_utf8(text);
        self.pipeline
            .process(Cow::Borrowed(text), &self.ctx)
            .map_err(Into::into)
    }
}

// For non-fusable pipelines (DynamicProcess, etc.)
impl Normy<DynamicProcess> {
    /// Normalize text - always uses apply path
    #[inline(always)]
    pub fn normalize<'a>(&self, text: &'a str) -> Result<Cow<'a, str>, NormyError> {
        #[cfg(debug_assertions)]
        assert_utf8(text);
        self.pipeline
            .process(Cow::Borrowed(text), &self.ctx)
            .map_err(Into::into)
    }
}

// ============================================================================
// Builder – tracks fusability while building
// ============================================================================
pub struct NormyBuilder<P: Process> {
    ctx: Context,
    current: P,
    all_fusable: bool,
    stage_count: usize,
}

impl Default for NormyBuilder<EmptyProcess> {
    #[inline(always)]
    fn default() -> Self {
        Self {
            ctx: Context::new(DEFAULT_LANG),
            current: EmptyProcess,
            all_fusable: true,
            stage_count: 0,
        }
    }
}

impl<P: Process> NormyBuilder<P> {
    #[inline(always)]
    pub fn lang(mut self, lang: Lang) -> Self {
        self.ctx = Context::new(lang);
        self
    }
    #[inline(always)]
    pub fn modify_lang(mut self, f: impl FnOnce(&mut crate::lang::LangEntry)) -> Self {
        self.ctx = Context::with_modified(self.ctx.lang, f);
        self
    }
    #[inline(always)]
    pub fn add_stage<S: Stage + StaticFusableStage + 'static>(
        mut self,
        stage: S,
    ) -> NormyBuilder<ChainedProcess<S, P>> {
        if !stage.supports_static_fusion() {
            self.all_fusable = false;
        }
        self.stage_count += 1;
        NormyBuilder {
            ctx: self.ctx,
            current: ChainedProcess {
                stage,
                previous: self.current,
            },
            all_fusable: self.all_fusable,
            stage_count: self.stage_count,
        }
    }
    #[inline(always)]
    pub fn build(self) -> Normy<P> {
        Normy {
            ctx: self.ctx,
            pipeline: self.current,
            all_fusable: self.all_fusable,
            stage_count: self.stage_count,
        }
    }
}

impl Normy<EmptyProcess> {
    #[inline(always)]
    pub fn builder() -> NormyBuilder<EmptyProcess> {
        NormyBuilder::default()
    }
}

// ============================================================================
// Dynamic builder path
// ============================================================================
pub struct DynamicNormyBuilder {
    ctx: Context,
    stages: SmallVec<[Arc<dyn Stage + Send + Sync>; 12]>,
    all_fusable: bool,
}

impl Default for DynamicNormyBuilder {
    #[inline(always)]
    fn default() -> Self {
        Self {
            ctx: Context::new(DEFAULT_LANG),
            stages: SmallVec::new(),
            all_fusable: true,
        }
    }
}

impl DynamicNormyBuilder {
    #[inline(always)]
    pub fn lang(mut self, lang: Lang) -> Self {
        self.ctx = Context::new(lang);
        self
    }
    #[inline(always)]
    pub fn modify_lang(mut self, f: impl FnOnce(&mut LangEntry)) -> Self {
        self.ctx = Context::with_modified(self.ctx.lang, f);
        self
    }
    #[inline(always)]
    pub fn add_stage<T: Stage + Send + Sync + 'static>(self, stage: T) -> Self {
        self.add_arc_stage(Arc::new(stage))
    }
    #[inline(always)]
    pub fn add_arc_stage(mut self, stage: Arc<dyn Stage + Send + Sync>) -> Self {
        self.stages.push(stage);
        self
    }
    #[inline(always)]
    pub fn add_boxed_stage(mut self, stage: Box<dyn Stage + Send + Sync>) -> Self {
        self.stages.push(stage.into());
        self
    }
    #[inline(always)]
    pub fn build(self) -> Normy<DynamicProcess> {
        let stage_len = self.stages.len();
        Normy {
            ctx: self.ctx,
            pipeline: DynamicProcess {
                stages: self.stages,
            },
            all_fusable: self.all_fusable,
            stage_count: stage_len,
        }
    }
}

impl Normy<DynamicProcess> {
    #[inline(always)]
    pub fn dynamic_builder() -> DynamicNormyBuilder {
        DynamicNormyBuilder::default()
    }
}
