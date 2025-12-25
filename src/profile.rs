pub mod preset;
use crate::{
    context::Context,
    fused_process::ProcessIslandInfo,
    process::{ChainedProcess, EmptyProcess, IslandInfo, Process},
    stage::{Stage, StageMetadata},
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

impl<P: Process + ProcessIslandInfo> ProfileBuilder<P> {
    pub fn add_stage<S: Stage + 'static>(self, stage: S) -> ProfileBuilder<ChainedProcess<S, P>> {
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

        ProfileBuilder {
            name: self.name,
            current: ChainedProcess {
                stage,
                previous: self.current,
                island_info,
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
