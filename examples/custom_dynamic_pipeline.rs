use std::borrow::Cow;

use normy::context::Context;
use normy::stage::{Stage, StageError};
use normy::{CaseFold, NORMALIZE_WHITESPACE_FULL, Normy, StripControlChars, TUR};

fn is_emoji(c: char) -> bool {
    matches!(
        c,
        '\u{1F300}'..='\u{1F5FF}'
            | '\u{1F600}'..='\u{1F64F}'
            | '\u{1F680}'..='\u{1F6FF}'
            | '\u{1F900}'..='\u{1F9FF}'
            | '\u{2600}'..='\u{26FF}'
            | '\u{2700}'..='\u{27BF}'
            | '\u{FE0E}' | '\u{FE0F}'
    )
}

#[derive(Default)]
pub struct StripEmoji;

impl Stage for StripEmoji {
    fn name(&self) -> &'static str {
        "strip_emoji"
    }

    fn needs_apply(&self, text: &str, _: &Context) -> Result<bool, StageError> {
        Ok(text.chars().any(is_emoji))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _: &Context) -> Result<Cow<'a, str>, StageError> {
        Ok(Cow::Owned(text.chars().filter(|&c| !is_emoji(c)).collect()))
    }
}

fn stage_from_name(name: &str) -> Option<Box<dyn Stage + Send + Sync>> {
    match name {
        "remove_control" => Some(Box::new(StripControlChars)),
        "fold_case" => Some(Box::new(CaseFold)),
        "strip_emoji" => Some(Box::new(StripEmoji)),
        "whitespace" => Some(Box::new(NORMALIZE_WHITESPACE_FULL)),
        _ => None,
    }
}

fn main() {
    let config = vec!["remove_control", "fold_case", "strip_emoji", "whitespace"];

    let mut builder = Normy::dynamic_builder() // ← renamed!
        .lang(TUR);

    for name in config {
        if let Some(stage) = stage_from_name(name) {
            builder = builder.add_boxed_stage(stage);
        } else {
            eprintln!("Unknown stage: {name}");
        }
    }

    let normalizer = builder.build();

    let input = "İSTANBUL ❤️ 2024 🚀 Büyük Şehir!";
    let output = normalizer.normalize(input).unwrap();
    assert_eq!(output.as_ref(), "istanbul 2024 büyük şehir!");
    println!("Dynamic pipeline result: {output}");
}
