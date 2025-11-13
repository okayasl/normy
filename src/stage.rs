pub mod lower_case;

use crate::context::Context;
use std::borrow::Cow;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StageError {
    #[error("Normalization failed at stage `{0}`: {1}")]
    Failed(&'static str, String),
}

pub trait Stage {
    fn name(&self) -> &'static str;
    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError>;
}
