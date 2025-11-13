use thiserror::Error;

use crate::{
    context::Context,
    lang::Lang,
    pipeline::Pipeline,
    profile::{Profile, ProfileError},
    stage::{Stage, StageError},
};
use std::{borrow::Cow, sync::Arc};

#[derive(Debug, Error)]
pub enum NormyError {
    #[error("stage error: {0}")]
    Stage(#[from] StageError),
    #[error("profile error: {0}")]
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

    pub fn normalize<'a>(&self, text: Cow<'a, str>) -> Result<Cow<'a, str>, NormyError> {
        let results = self.pipeline.process(text, &self.ctx)?;
        Ok(results)
    }

    pub fn normalize_with_profile<'a>(
        &self,
        profile: Profile,
        text: Cow<'a, str>,
    ) -> Result<Cow<'a, str>, NormyError> {
        let results = profile.run(text, &self.ctx)?;
        Ok(results)
    }
}

pub struct NormyBuilder {
    lang: Lang,
    stages: Vec<Arc<dyn Stage + Send + Sync>>,
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

    pub fn add_stage<T: crate::stage::Stage + Send + Sync + 'static>(mut self, stage: T) -> Self {
        self.stages.push(Arc::new(stage));
        self
    }

    pub fn build(self) -> Normy {
        let ctx = Context { lang: self.lang };
        let pipeline = Pipeline::new(self.stages);
        Normy { ctx, pipeline }
    }
}
