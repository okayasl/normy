pub mod lower_case;
pub mod trim_whitespace;
pub mod validation;

use crate::context::Context;
use std::borrow::Cow;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StageError {
    #[error("Normalization failed at stage `{0}`: {1}")]
    Failed(&'static str, String),
    #[error("Normalization validation failed at stage `{0}`: {1}")]
    Validation(&'static str, String),
}

pub trait Stage: Send + Sync {
    fn name(&self) -> &'static str;
    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError>;
    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError>;
    fn char_mapper(&self) -> Option<Box<dyn CharMapper + 'static>>;
}

/// **The heart of fused, zero-allocation pipelines**
pub trait CharMapper: Send + Sync {
    fn map(&self, c: char, ctx: &Context) -> char;

    // Default: always apply (conservative)
    fn needs_apply(&self, _text: &str, _ctx: &Context) -> bool {
        true
    }

    fn bind<'a>(
        &self,
        text: &'a str,
        ctx: &Context,
    ) -> Box<dyn std::iter::Iterator<Item = char> + 'a>;
}
