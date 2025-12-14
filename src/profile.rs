pub mod preset;
use crate::{
    context::Context,
    process::{ChainedProcess, EmptyProcess, Process},
    stage::{Stage, StaticStageIter},
};
use std::borrow::Cow;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("Normalization failed at profile `{0}`: {1}")]
    Failed(&'static str, String),
}

pub struct Profile<P: Process> {
    name: &'static str,
    pipeline: P,
}

impl<P: Process> Profile<P> {
    pub fn run<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, ProfileError> {
        self.pipeline
            .process(text, ctx)
            .map_err(|e| ProfileError::Failed(self.name, e.to_string()))
    }
}
impl Profile<EmptyProcess> {
    pub fn builder(name: &'static str) -> ProfileBuilder<EmptyProcess> {
        ProfileBuilder::new(name)
    }
}

pub struct ProfileBuilder<P: Process> {
    name: &'static str,
    current: P,
}

impl ProfileBuilder<EmptyProcess> {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            current: EmptyProcess,
        }
    }
}

impl<P: Process> ProfileBuilder<P> {
    pub fn add_stage<S: Stage + StaticStageIter + 'static>(
        self,
        stage: S,
    ) -> ProfileBuilder<ChainedProcess<S, P>> {
        ProfileBuilder {
            name: self.name,
            current: ChainedProcess {
                stage,
                previous: self.current,
            },
        }
    }

    pub fn build(self) -> Profile<P> {
        Profile {
            name: self.name,
            pipeline: self.current,
        }
    }
}
