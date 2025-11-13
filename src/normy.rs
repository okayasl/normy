use crate::{
    context::Context,
    lang::Lang,
    pipeline::Pipeline,
    profile::{Profile, ProfileError},
    stage::{Stage, StageError},
};
use std::{borrow::Cow, sync::Arc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NormyError {
    #[error(transparent)]
    Stage(#[from] StageError),
    #[error(transparent)]
    Profile(#[from] ProfileError),
}

pub struct Normy {
    ctx: Context,
    pipeline: Pipeline,
}

impl Normy {
    pub fn builder() -> NormyBuilder {
        NormyBuilder::default()
    }

    // Public API: &str → zero-copy default
    pub fn normalize<'a>(&self, text: &'a str) -> Result<Cow<'a, str>, NormyError> {
        self.pipeline
            .process(Cow::Borrowed(text), &self.ctx)
            .map_err(Into::into)
    }

    // Profile API: accepts Cow because it may be in the middle of a pipeline
    pub fn normalize_with_profile<'a>(
        &self,
        profile: &Profile,
        text: &'a str, // still &str — user-facing
    ) -> Result<Cow<'a, str>, NormyError> {
        profile
            .run(Cow::Borrowed(text), &self.ctx)
            .map_err(Into::into)
    }
}

pub struct NormyBuilder {
    lang: Lang,
    stages: Vec<Arc<dyn Stage>>, // No extra bounds needed
}

impl Default for NormyBuilder {
    fn default() -> Self {
        Self {
            lang: Lang::English,
            stages: Default::default(),
        }
    }
}

impl NormyBuilder {
    pub fn lang(mut self, lang: Lang) -> Self {
        self.lang = lang;
        self
    }

    pub fn add_stage<T: Stage + 'static>(mut self, stage: T) -> Self {
        self.stages.push(Arc::new(stage));
        self
    }

    pub fn build(self) -> Normy {
        let ctx = Context { lang: self.lang };
        let pipeline = Pipeline::new(self.stages);
        Normy { ctx, pipeline }
    }
}
