use crate::stage::StageError;
use crate::{context::Context, stage::Stage};
use std::borrow::Cow;
use std::sync::Arc;

pub struct Pipeline {
    stages: Vec<Arc<dyn Stage + Send + Sync>>,
}

impl Pipeline {
    pub fn new(stages: Vec<Arc<dyn Stage + Send + Sync>>) -> Self {
        Self { stages }
    }

    pub fn process<'a>(
        &self,
        mut text: Cow<'a, str>,
        ctx: &Context,
    ) -> Result<Cow<'a, str>, StageError> {
        for stage in &self.stages {
            text = stage.apply(text, ctx)?
        }
        Ok(text)
    }
}
