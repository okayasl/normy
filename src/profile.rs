use thiserror::Error;

use crate::{context::Context, stage::Stage};
use std::{borrow::Cow, sync::Arc};

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("Normalization failed at profile `{0}`: {1}")]
    Failed(&'static str, String),
}

pub struct Profile {
    pub name: &'static str,
    pub stages: Vec<Arc<dyn Stage + Send + Sync>>,
}

impl Profile {
    pub fn new(name: &'static str, stages: Vec<Arc<dyn Stage + Send + Sync>>) -> Self {
        Self { name, stages }
    }

    pub fn run<'a>(
        &self,
        mut text: Cow<'a, str>,
        ctx: &Context,
    ) -> Result<Cow<'a, str>, ProfileError> {
        for stage in &self.stages {
            text = stage
                .apply(text, ctx)
                .map_err(|e| ProfileError::Failed(self.name, e.to_string()))?;
        }
        Ok(text)
    }
}
