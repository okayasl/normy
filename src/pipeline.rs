// src/pipeline.rs
use crate::{
    context::Context,
    stage::{Stage, StageError},
};
use std::borrow::Cow;
use std::sync::Arc;

pub struct Pipeline {
    stages: Vec<Arc<dyn Stage>>,
}

impl Pipeline {
    pub fn new(stages: Vec<Arc<dyn Stage>>) -> Self {
        Self { stages }
    }

    pub fn process<'a>(
        &self,
        text: Cow<'a, str>,
        ctx: &Context,
    ) -> Result<Cow<'a, str>, StageError> {
        let mut current = text;

        for stage in &self.stages {
            // Fast path: skip if no mutation needed
            if !stage.needs_apply(&current, ctx)? {
                continue;
            }

            // Mutation required — apply stage
            current = stage.apply(current, ctx)?;
        }

        Ok(current)
    }
}
