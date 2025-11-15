use crate::{
    Lang,
    context::Context,
    lang::DEFAULT_LANG,
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
    pub fn normalize<'a>(&self, text: &'a str) -> Result<Cow<'a, str>, NormyError> {
        #[cfg(debug_assertions)]
        assert_utf8(text);
        self.pipeline
            .process(Cow::Borrowed(text), &self.ctx)
            .map_err(Into::into)
    }

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

pub struct NormyBuilder<P: Process> {
    lang: Lang,
    current: P,
}

impl Default for NormyBuilder<EmptyProcess> {
    fn default() -> Self {
        Self {
            lang: DEFAULT_LANG,
            current: EmptyProcess,
        }
    }
}

impl<P: Process> NormyBuilder<P> {
    pub fn lang(mut self, lang: Lang) -> Self {
        self.lang = lang;
        self
    }

    pub fn add_stage<S: Stage + 'static>(self, stage: S) -> NormyBuilder<ChainedProcess<S, P>> {
        NormyBuilder {
            lang: self.lang,
            current: ChainedProcess {
                stage,
                previous: self.current,
            },
        }
    }

    pub fn build(self) -> Normy<P> {
        let ctx = Context { lang: self.lang };
        Normy {
            ctx,
            pipeline: self.current,
        }
    }
}

impl Normy<EmptyProcess> {
    pub fn builder() -> NormyBuilder<EmptyProcess> {
        NormyBuilder::default()
    }
}

impl Normy<DynProcess> {
    pub fn plugin_builder() -> DynNormyBuilder {
        DynNormyBuilder::new()
    }
}
pub struct DynNormyBuilder {
    lang: Lang,
    pipeline: DynProcess,
}

impl DynNormyBuilder {
    fn new() -> Self {
        Self {
            lang: DEFAULT_LANG,
            pipeline: DynProcess::new(),
        }
    }

    pub fn lang(mut self, lang: Lang) -> Self {
        self.lang = lang;
        self
    }

    pub fn add_stage<T: Stage + Send + Sync + 'static>(self, stage: T) -> Self {
        Self {
            pipeline: self.pipeline.push(stage),
            ..self
        }
    }

    pub fn build(self) -> Normy<DynProcess> {
        let ctx = Context { lang: self.lang };
        Normy {
            ctx,
            pipeline: self.pipeline,
        }
    }
}
