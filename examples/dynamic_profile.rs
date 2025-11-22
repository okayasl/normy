// examples/language_agnostic_profiles.rs
//! Language-Agnostic Profiles + Runtime Context Override
//!
//! This is the **final form** of Normy.
//! This is what the whitepaper meant when it said:
//! "Profiles are pure data. Context is the soul."
//!
//! • Profiles contain NO language
//! • Context is injected at runtime
//! • Same profile → different behavior per user
//! • Zero-cost, zero-duplication, perfect fusion
//! • This is how you scale to 200+ languages with 5 profiles

use normy::{
    ENG, FoldCase, NormalizeWhitespace, Normy, RemoveControlChars, RemoveDiacritics,
    ReplaceFullwidth, StripHtml, StripMarkdown, TUR, UnigramCJK, ZHO,
    context::Context,
    profile::Profile,
    stage::{CharMapper, Stage, StageError},
};
use std::sync::Arc;
use std::{borrow::Cow, iter::FusedIterator};

// ————————————————————————————————
// 1. CUSTOM STAGE: StripEmoji (perfect, clippy-clean, fused)
// ————————————————————————————————
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
    fn as_char_mapper(&self, _: &Context) -> Option<&dyn CharMapper> {
        Some(self)
    }
    fn into_dyn_char_mapper(self: Arc<Self>, _: &Context) -> Option<Arc<dyn CharMapper>> {
        Some(self)
    }
}

impl CharMapper for StripEmoji {
    fn map(&self, c: char, _: &Context) -> Option<char> {
        (!is_emoji(c)).then_some(c)
    }
    fn bind<'a>(&self, text: &'a str, _: &Context) -> Box<dyn FusedIterator<Item = char> + 'a> {
        Box::new(text.chars().filter(|&c| !is_emoji(c)))
    }
}

// ————————————————————————————————
// 2. LANGUAGE-AGNOSTIC PROFILES — Using Your Exact API
// ————————————————————————————————
fn search_profile() -> Profile<normy::process::DynProcess> {
    Profile::plugin_builder("search")
        .add_stage(RemoveControlChars)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(FoldCase)
        .add_stage(RemoveDiacritics)
        .add_stage(ReplaceFullwidth)
        .add_stage(StripEmoji)
        .add_stage(NormalizeWhitespace::default())
        .build()
}

fn social_media_profile() -> Profile<normy::process::DynProcess> {
    Profile::plugin_builder("social_media")
        .add_stage(RemoveControlChars)
        .add_stage(StripHtml)
        .add_stage(FoldCase)
        .add_stage(NormalizeWhitespace::default())
        .build()
}

fn cjk_search_profile() -> Profile<normy::process::DynProcess> {
    Profile::plugin_builder("cjk_search")
        .add_stage(RemoveControlChars)
        .add_stage(ReplaceFullwidth)
        .add_stage(UnigramCJK)
        .add_stage(NormalizeWhitespace::default())
        .build()
}

fn maximum_profile() -> Profile<normy::process::DynProcess> {
    Profile::plugin_builder("maximum")
        .add_stage(RemoveControlChars)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(FoldCase)
        .add_stage(RemoveDiacritics)
        .add_stage(ReplaceFullwidth)
        .add_stage(StripEmoji)
        .add_stage(NormalizeWhitespace::default())
        .add_stage(StripEmoji) // Nuclear double-pass
        .build()
}

fn main() {
    let input = "İSTANBUL ❤️ café naïve Zürich 東京タワー <b>BOLD</b> https://x.com";

    // One Normy per user/session — holds language context
    let turkish_user = Normy::plugin_builder().lang(TUR).build();
    let english_user = Normy::plugin_builder().lang(ENG).build();
    let chinese_user = Normy::plugin_builder().lang(ZHO).build();

    println!("Input: {input}\n");

    // Same profile → different behavior based on runtime context
    let search = search_profile();
    println!("PROFILE: search (language-agnostic)");
    println!(
        "  Turkish user → {}",
        turkish_user.normalize_with_profile(&search, input).unwrap()
    );
    println!(
        "  English user → {}",
        english_user.normalize_with_profile(&search, input).unwrap()
    );
    println!(
        "  Chinese user → {}",
        chinese_user.normalize_with_profile(&search, input).unwrap()
    );

    println!("\nPROFILE: cjk_search");
    println!(
        "  Chinese user → {}",
        chinese_user
            .normalize_with_profile(&cjk_search_profile(), input)
            .unwrap()
    );

    println!("\nPROFILE: social_media (preserves emoji)");
    println!(
        "  Any user     → {}",
        english_user
            .normalize_with_profile(&social_media_profile(), input)
            .unwrap()
    );

    println!("\nPROFILE: maximum (nuclear)");
    println!(
        "  English user → {}",
        english_user
            .normalize_with_profile(&maximum_profile(), input)
            .unwrap()
    );

    // Real-world: Accept-Language routing
    println!("\n=== REAL-WORLD USAGE ===");
    let accept_language = "tr-TR,tr;q=0.9,en;q=0.7";
    let normalizer = if accept_language.starts_with("tr") {
        Normy::plugin_builder().lang(TUR).build()
    } else if accept_language.contains("zh") {
        Normy::plugin_builder().lang(ZHO).build()
    } else {
        Normy::plugin_builder().lang(ENG).build()
    };

    println!("Accept-Language: {accept_language}");
    println!("Using profile: search");
    println!(
        "Output: {}",
        normalizer.normalize_with_profile(&search, input).unwrap()
    );
}
