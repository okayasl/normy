use crate::{
    context::Context,
    stage::{Stage, StageError},
};
use std::borrow::Cow;

pub struct Lowercase;

impl Stage for Lowercase {
    fn name(&self) -> &'static str {
        "lowercase"
    }

    fn apply<'a>(&self, _text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        todo!("Not implemented yet, it should be language aware and zero copy if not needed.");
        // Ok(Cow::Owned(text.to_lowercase()))
    }
}
