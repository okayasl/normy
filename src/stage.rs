pub mod lower_case;
use crate::context::Context;
use std::borrow::Cow;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StageError {
    #[error("Normalization failed at stage `{0}`: {1}")]
    Failed(&'static str, String),
}

/// Object-safe trait: used for dynamic dispatch and extensibility
pub trait Stage: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    /// Fast pre-check: can we skip this stage entirely?
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError>;

    /// Apply transformation — zero-copy when possible
    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError>;
}
