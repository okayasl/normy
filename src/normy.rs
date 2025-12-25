use crate::{
    context::Context,
    fused_process::{FusedProcess, ProcessIslandInfo},
    lang::{DEFAULT_LANG, Lang},
    process::{ChainedProcess, DynamicProcess, EmptyProcess, IslandInfo, Process},
    profile::{Profile, ProfileError},
    stage::{Stage, StageError, StageMetadata},
    static_fused_process::StaticFusedProcess,
};
use smallvec::SmallVec;
use std::{borrow::Cow, sync::Arc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NormyError {
    #[error(transparent)]
    Stage(#[from] StageError),
    #[error(transparent)]
    Profile(#[from] ProfileError),
}

/// # Safety: `text` **must** be valid UTF-8.
#[cfg(debug_assertions)]
#[inline(always)]
fn assert_utf8(text: &str) {
    debug_assert!(std::str::from_utf8(text.as_bytes()).is_ok());
}

pub struct Normy<P: Process> {
    ctx: Context,
    pipeline: P,
}

impl<P: Process> Normy<P> {
    #[inline(always)]
    pub fn normalize<'a>(&self, text: &'a str) -> Result<Cow<'a, str>, NormyError> {
        #[cfg(debug_assertions)]
        assert_utf8(text);
        self.pipeline
            .process(Cow::Borrowed(text), &self.ctx)
            .map_err(Into::into)
    }

    #[inline(always)]
    pub fn normalize_fused<'a>(&self, text: &'a str) -> Result<Cow<'a, str>, NormyError>
    where
        P: FusedProcess,
    {
        #[cfg(debug_assertions)]
        assert_utf8(text);
        self.pipeline
            .process_fused(text, &self.ctx)
            .map_err(Into::into)
    }

    #[inline(always)]
    pub fn normalize_static_fused<'a>(&self, text: &'a str) -> Result<Cow<'a, str>, NormyError>
    where
        P: StaticFusedProcess,
    {
        #[cfg(debug_assertions)]
        assert_utf8(text);
        self.pipeline
            .process_static_fused(text, &self.ctx)
            .map_err(Into::into)
    }

    #[inline(always)]
    pub fn normalize_with_profile<'a, Q: Process>(
        &self,
        profile: &Profile<Q>,
        text: &'a str,
    ) -> Result<Cow<'a, str>, NormyError> {
        profile
            .run(Cow::Borrowed(text), &self.ctx)
            .map_err(Into::into)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Builder – monomorphised path
// ─────────────────────────────────────────────────────────────────────────────
pub struct NormyBuilder<P: Process> {
    ctx: Context,
    current: P,
}

impl Default for NormyBuilder<EmptyProcess> {
    #[inline(always)]
    fn default() -> Self {
        Self {
            ctx: Context::new(DEFAULT_LANG),
            current: EmptyProcess,
        }
    }
}

impl<P: Process + ProcessIslandInfo> NormyBuilder<P> {
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
    pub fn add_stage<S: Stage + 'static>(self, stage: S) -> NormyBuilder<ChainedProcess<S, P>> {
        // Extract metadata from this stage
        let stage_meta = StageMetadata::from_stage(&stage);

        // Get previous island info
        let prev_island = self.current.get_island_info();

        // Compute THIS stage's island info based on previous + this stage
        let island_info = if stage_meta.supports_fusion {
            // This stage is fusable - extend or start island
            if prev_island.island_len == 0 {
                // Starting a new island
                IslandInfo::new_island(stage_meta.capacity_factor, stage_meta.safe_skip)
            } else {
                // Extending existing island
                IslandInfo::extend_island(
                    prev_island,
                    stage_meta.capacity_factor,
                    stage_meta.safe_skip,
                )
            }
        } else {
            // Barrier stage - reset island
            IslandInfo::reset()
        };

        NormyBuilder {
            ctx: self.ctx,
            current: ChainedProcess {
                stage,
                previous: self.current,
                island_info, // ✨ Computed once at build time!
            },
        }
    }
    #[inline(always)]
    pub fn build(self) -> Normy<P> {
        Normy {
            ctx: self.ctx,
            pipeline: self.current,
        }
    }
}

impl Normy<EmptyProcess> {
    #[inline(always)]
    pub fn builder() -> NormyBuilder<EmptyProcess> {
        NormyBuilder::default()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Dynamic plugin path – unchanged except ctx construction
// ─────────────────────────────────────────────────────────────────────────────
pub struct DynamicNormyBuilder {
    ctx: Context,
    stages: SmallVec<[Arc<dyn Stage + Send + Sync>; 12]>,
}

impl Default for DynamicNormyBuilder {
    #[inline(always)]
    fn default() -> Self {
        Self {
            ctx: Context::new(DEFAULT_LANG),
            stages: SmallVec::new(),
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
    pub fn modify_lang(mut self, f: impl FnOnce(&mut crate::lang::LangEntry)) -> Self {
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
        self.stages.push(stage.into()); // ← Box → Arc conversion
        self
    }

    #[inline(always)]
    pub fn build(self) -> Normy<DynamicProcess> {
        Normy {
            ctx: self.ctx,
            pipeline: DynamicProcess {
                stages: self.stages,
            },
        }
    }
}

impl Normy<DynamicProcess> {
    #[inline(always)]
    pub fn dynamic_builder() -> DynamicNormyBuilder {
        DynamicNormyBuilder::default()
    }
}
