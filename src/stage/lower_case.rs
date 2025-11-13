// src/stage/lower_case.rs
use crate::context::Context;
use crate::lang::Lang;
use crate::stage::{Stage, StageError};
use std::borrow::Cow;

pub struct Lowercase;

impl Stage for Lowercase {
    fn name(&self) -> &'static str {
        "lowercase"
    }

    fn needs_apply(&self, text: &str, ctx: &Context) -> Result<bool, StageError> {
        Ok(match ctx.lang {
            Lang::Turkish => text.contains(['I', 'İ']),
            _ => text.chars().any(char::is_uppercase),
        })
    }

    fn apply<'a>(&self, text: Cow<'a, str>, ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if !self.needs_apply(&text, ctx)? {
            return Ok(text);
        }

        let result: String = if ctx.lang == Lang::Turkish {
            let mut out = String::with_capacity(text.len());
            for c in text.chars() {
                match c {
                    'I' => out.push('ı'),
                    'İ' => out.push('i'),
                    _ => out.extend(c.to_lowercase()),
                }
            }
            out
        } else {
            text.to_lowercase()
        };

        Ok(Cow::Owned(result))
    }
}
