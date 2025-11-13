use std::borrow::Cow;

use crate::{
    context::Context,
    stage::{Stage, StageError},
};

pub struct TrimWhitespace;

impl Stage for TrimWhitespace {
    fn name(&self) -> &'static str {
        "trim_ws"
    }

    fn needs_apply(&self, text: &str, _: &Context) -> Result<bool, StageError> {
        Ok(text.trim() != text)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _: &Context) -> Result<Cow<'a, str>, StageError> {
        Ok(Cow::Owned(text.trim().to_owned()))
    }
}
