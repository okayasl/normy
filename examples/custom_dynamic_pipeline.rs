// examples/custom_dynamic_pipeline.rs
//! Example: Building a fully dynamic normalization pipeline at runtime
//! using Normy's plugin system (`Normy::plugin_builder()` + custom Stage)
//!
//! This demonstrates:
//! • Zero-cost dynamic extensibility
//! • Full locale awareness
//! • Seamless integration with built-in stages
//! • Real-world use case: "aggressive search normalization" for a search engine

use std::borrow::Cow;
use std::iter::FusedIterator;
use std::sync::Arc;

use normy::context::Context;
use normy::stage::{CharMapper, Stage, StageError};
use normy::{FoldCase, NormalizeWhitespace, Normy, RemoveControlChars, TUR};

/// Custom stage: Strip common emoji and pictographs (e.g. ❤️, 🚀, 🏆)
/// This is useful for search engines that treat emoji as noise.
fn is_emoji(c: char) -> bool {
    matches!(
        c,
        '\u{1F300}'..='\u{1F5FF}'   // Symbols & Pictographs (INCLUDES Skin Tones)
            | '\u{1F600}'..='\u{1F64F}' // Emoticons
            | '\u{1F680}'..='\u{1F6FF}' // Transport & Map Symbols
            | '\u{1F900}'..='\u{1F9FF}' // Supplemental Symbols and Pictographs
            | '\u{2600}'..='\u{26FF}'   // Misc Symbols
            | '\u{2700}'..='\u{27BF}'   // Dingbats
            | '\u{FE0E}' | '\u{FE0F}'   // Variation selectors
    )
}

pub struct StripEmoji;

impl Stage for StripEmoji {
    fn name(&self) -> &'static str {
        "strip_emoji"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(text.chars().any(is_emoji))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        Ok(Cow::Owned(text.chars().filter(|&c| !is_emoji(c)).collect()))
    }

    #[inline]
    fn as_char_mapper(&self, _ctx: &Context) -> Option<&dyn CharMapper> {
        Some(self)
    }

    #[inline]
    fn into_dyn_char_mapper(self: Arc<Self>, _ctx: &Context) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }
}

impl CharMapper for StripEmoji {
    #[inline(always)]
    fn map(&self, c: char, _ctx: &Context) -> Option<char> {
        if is_emoji(c) { None } else { Some(c) }
    }

    fn bind<'a>(&self, text: &'a str, _ctx: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(text.chars().filter(|&c| !is_emoji(c)))
    }
}

fn main() {
    let normalizer = Normy::plugin_builder()
        .lang(TUR)
        .add_stage(RemoveControlChars)
        .add_stage(FoldCase)
        .add_stage(StripEmoji)
        .add_stage(NormalizeWhitespace::default())
        .build();

    let output = normalizer
        .normalize("İSTANBUL ❤️ 2024 🚀 Büyük Şehir!")
        .unwrap();
    assert_eq!(output.as_ref(), "istanbul 2024 büyük şehir!");
    println!("{output}");
}
