use crate::{
    context::Context,
    lang::{DEFAULT_LANG, Lang},
    process::{ChainedProcess, DynProcess, EmptyProcess, Process},
    profile::{Profile, ProfileError},
    stage::{Stage, StageError},
};
use std::borrow::Cow;
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
    pub fn add_stage<S: Stage + 'static>(self, stage: S) -> NormyBuilder<ChainedProcess<S, P>> {
        NormyBuilder {
            ctx: self.ctx,
            current: ChainedProcess {
                stage,
                previous: self.current,
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
pub struct DynNormyBuilder {
    ctx: Context,
    pipeline: DynProcess,
}

impl Default for DynNormyBuilder {
    #[inline(always)]
    fn default() -> Self {
        Self {
            ctx: Context::new(DEFAULT_LANG),
            pipeline: DynProcess::new(),
        }
    }
}

impl DynNormyBuilder {
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
        Self {
            pipeline: self.pipeline.push(stage),
            ..self
        }
    }

    #[inline(always)]
    pub fn build(self) -> Normy<DynProcess> {
        Normy {
            ctx: self.ctx,
            pipeline: self.pipeline,
        }
    }
}

impl Normy<DynProcess> {
    #[inline(always)]
    pub fn plugin_builder() -> DynNormyBuilder {
        DynNormyBuilder::default()
    }
}
