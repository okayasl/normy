// src/profile.rs
use crate::{
    context::Context,
    process::{ChainedProcess, DynProcess, EmptyProcess, Process},
    stage::Stage,
};
use std::borrow::Cow;
use thiserror::Error;

/// Errors returned from a profile.
#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("Normalization failed at profile `{0}`: {1}")]
    Failed(&'static str, String),
}

/// A pre-built pipeline – internally just a `Process`.
pub struct Profile<P: Process> {
    name: &'static str,
    pipeline: P,
}

impl<P: Process> Profile<P> {
    /// Run the profile.  The public API is identical to `Normy::normalize`.
    pub fn run<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, ProfileError> {
        self.pipeline
            .process(text, ctx)
            .map_err(|e| ProfileError::Failed(self.name, e.to_string()))
    }
}

/* ---------- 1. Static (monomorphised) builder ---------- */

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
    pub fn add_stage<S: Stage + 'static>(self, stage: S) -> ProfileBuilder<ChainedProcess<S, P>> {
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

/* ---------- 2. Dynamic (plugin) builder ---------- */
impl Profile<DynProcess> {
    /// **Exactly like `Normy::plugin_builder()`**
    pub fn plugin_builder(name: &'static str) -> DynProfileBuilder {
        DynProfileBuilder {
            name,
            pipeline: DynProcess::new(),
        }
    }
}

pub struct DynProfileBuilder {
    name: &'static str,
    pipeline: DynProcess,
}

impl DynProfileBuilder {
    pub fn add_stage<T: Stage + Send + Sync + 'static>(self, stage: T) -> Self {
        Self {
            pipeline: self.pipeline.push(stage),
            ..self
        }
    }

    pub fn build(self) -> Profile<DynProcess> {
        Profile {
            name: self.name,
            pipeline: self.pipeline,
        }
    }
}
