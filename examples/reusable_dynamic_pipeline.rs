use normy::context::Context;
use normy::process::FusablePipeline;
use normy::stage::{Stage, StageError, StaticFusableStage, StaticIdentityAdapter};
use normy::{
    CaseFold, ENG, NFKC, NORMALIZE_WHITESPACE_FULL, Normy, NormyBuilder, RemoveDiacritics,
    SegmentWords, StripControlChars, StripFormatControls, StripHtml, StripMarkdown, TUR,
};
use std::borrow::Cow;
use std::iter::FusedIterator;

// ————————————————————————————————
// CUSTOM STAGE: StripEmoji
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
}

impl StaticFusableStage for StripEmoji {
    type Adapter<'a, I>
        = StaticIdentityAdapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a;

    // Trigger the fallback to the optimized apply() method
    #[inline(always)]
    fn supports_static_fusion(&self) -> bool {
        false
    }

    #[inline(always)]
    fn static_fused_adapter<'a, I>(&self, input: I, _ctx: &'a Context) -> Self::Adapter<'a, I>
    where
        I: FusedIterator<Item = char> + 'a,
    {
        StaticIdentityAdapter::new(input)
    }
}

// ————————————————————————————————
// DYNAMIC STAGE REGISTRY
// ————————————————————————————————
fn stage_from_name(name: &str) -> Option<Box<dyn Stage + Send + Sync>> {
    match name {
        "nfkc" => Some(Box::new(NFKC)),
        "lowercase" => Some(Box::new(CaseFold)),
        "remove_diacritics" => Some(Box::new(RemoveDiacritics)),
        "strip_html" => Some(Box::new(StripHtml)),
        "strip_markdown" => Some(Box::new(StripMarkdown)),
        "strip_control" => Some(Box::new(StripControlChars)),
        "strip_format" => Some(Box::new(StripFormatControls)),
        "strip_emoji" => Some(Box::new(StripEmoji)),
        "normalize_whitespace" => Some(Box::new(NORMALIZE_WHITESPACE_FULL)),
        "segment_words" => Some(Box::new(SegmentWords)),
        _ => None,
    }
}

// ————————————————————————————————
// STATIC PIPELINE FUNCTIONS (compile-time optimization)
// ————————————————————————————————
fn social_media_pipeline() -> NormyBuilder<impl FusablePipeline> {
    Normy::builder()
        .add_stage(NFKC)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .add_stage(StripControlChars)
        .add_stage(StripFormatControls)
        .add_stage(StripEmoji)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
}

fn search_pipeline() -> NormyBuilder<impl FusablePipeline> {
    Normy::builder()
        .add_stage(NFKC)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(StripFormatControls)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .add_stage(SegmentWords)
}

fn main() {
    println!("=== NORMY PIPELINE CONFIGURATION STRATEGIES ===\n");

    // ========================================
    // 1. STATIC PIPELINES - Best Performance
    // ========================================
    println!("1. STATIC PIPELINES (Compile-time Optimization)");
    println!("   Use case: Known pipeline at compile time, maximum performance\n");

    let static_normalizer = social_media_pipeline().lang(TUR).build();
    let input = "İSTANBUL ❤️ 2024 🚀 Büyük Şehir!";
    let output = static_normalizer.normalize(input).unwrap();

    println!("   Input:  {}", input);
    println!("   Output: {}", output);
    assert_eq!(output.as_ref(), "istanbul 2024 büyük şehir!");
    println!("   ✓ Zero-cost abstraction, fully inlined\n");

    // ========================================
    // 1b. ANOTHER STATIC PIPELINE - Search
    // ========================================
    println!("1b. ANOTHER STATIC PIPELINE (Search Optimization)");
    println!("   Use case: Search engines, autocomplete, indexing\n");

    let static_search = search_pipeline().lang(TUR).build();
    let search_input = "Café <b>İstanbul</b> résumé";
    let search_output = static_search.normalize(search_input).unwrap();

    println!("   Input:  {}", search_input);
    println!("   Output: {}", search_output);
    println!("   ✓ Optimized for search with word segmentation\n");

    // ========================================
    // 2. DYNAMIC PIPELINES - Runtime Configuration
    // ========================================
    println!("2. DYNAMIC PIPELINES (Runtime Configuration)");
    println!("   Use case: Pipeline determined at runtime (config files, user preferences)\n");

    // Simulating configuration from external source (file, database, API, etc.)
    let config = vec![
        "nfkc",
        "lowercase",
        "strip_emoji",
        "strip_control",
        "normalize_whitespace",
    ];

    println!("   Configuration: {:?}", config);

    let mut builder = Normy::dynamic_builder().lang(TUR);

    for name in config {
        if let Some(stage) = stage_from_name(name) {
            builder = builder.add_boxed_stage(stage);
        } else {
            eprintln!("   ⚠️  Unknown stage: {}", name);
        }
    }

    let dynamic_normalizer = builder.build();
    let output = dynamic_normalizer.normalize(input).unwrap();

    println!("   Input:  {}", input);
    println!("   Output: {}", output);
    assert_eq!(output.as_ref(), "istanbul 2024 büyük şehir!");
    println!("   ✓ Flexible runtime configuration\n");

    // ========================================
    // 3. COMPARISON: Multiple Configurations
    // ========================================
    println!("3. DYNAMIC CONFIGURATION SCENARIOS\n");

    let scenarios = vec![
        (
            "Minimal Cleaning",
            vec!["nfkc", "normalize_whitespace"],
            "İSTANBUL ❤️ 2024",
            "İSTANBUL ❤️ 2024",
            ENG,
        ),
        (
            "Remove Emoji Only",
            vec!["strip_emoji", "normalize_whitespace"],
            "Hello ❤️ World 🚀",
            "Hello World",
            ENG,
        ),
        (
            "Search Optimization (English)",
            vec![
                "nfkc",
                "lowercase",
                "remove_diacritics",
                "strip_html",
                "normalize_whitespace",
            ],
            "<b>Café</b> Naïve",
            "café naïve", // English preserves foreign diacritics
            ENG,
        ),
        (
            "Search Optimization (Turkish)",
            vec![
                "nfkc",
                "lowercase",
                "remove_diacritics",
                "strip_html",
                "normalize_whitespace",
            ],
            "<b>Café</b> Naïve",
            "café naïve", // Turkish preserves foreign diacritics
            TUR,
        ),
        (
            "Full Social Media",
            vec![
                "nfkc",
                "strip_html",
                "strip_markdown",
                "lowercase",
                "strip_emoji",
                "normalize_whitespace",
            ],
            "**Check** this! ❤️ <script>alert()</script>",
            "check this!",
            ENG,
        ),
    ];

    for (name, config, input, expected, lang) in scenarios {
        println!("   Scenario: {}", name);
        println!("   Pipeline: {:?}", config);

        let mut builder = Normy::dynamic_builder().lang(lang);
        for stage_name in config {
            if let Some(stage) = stage_from_name(stage_name) {
                builder = builder.add_boxed_stage(stage);
            }
        }

        let normalizer = builder.build();
        let output = normalizer.normalize(input).unwrap();

        println!("   Input:  {}", input);
        println!("   Output: {}", output);
        assert_eq!(output.as_ref(), expected);
        println!();
    }

    // ========================================
    // 4. PERFORMANCE COMPARISON
    // ========================================
    println!("4. PERFORMANCE CHARACTERISTICS\n");

    let test_input = "İSTANBUL ❤️ Café <b>Bold</b> 2024";

    // Static pipeline
    let static_social = social_media_pipeline().lang(TUR).build();
    let static_result = static_social.normalize(test_input).unwrap();

    // Dynamic pipeline with same stages
    let dynamic_config = vec![
        "nfkc",
        "strip_html",
        "strip_markdown",
        "lowercase",
        "remove_diacritics",
        "strip_control",
        "strip_format",
        "strip_emoji",
        "normalize_whitespace",
    ];

    let mut dynamic_builder = Normy::dynamic_builder().lang(TUR);
    for stage_name in &dynamic_config {
        if let Some(stage) = stage_from_name(stage_name) {
            dynamic_builder = dynamic_builder.add_boxed_stage(stage);
        }
    }
    let dynamic_social = dynamic_builder.build();
    let dynamic_result = dynamic_social.normalize(test_input).unwrap();

    println!("   Input: {}", test_input);
    println!("   Static Output:  {}", static_result);
    println!("   Dynamic Output: {}", dynamic_result);
    assert_eq!(static_result, dynamic_result);

    println!("\n   Performance Notes:");
    println!("   • Static: Fully inlined, zero-cost abstractions, fastest");
    println!("   • Dynamic: Virtual dispatch overhead, but flexible");
    println!("   • Both: Same correctness guarantees, zero-copy when possible");

    // ========================================
    // 5. WHEN TO USE EACH APPROACH
    // ========================================
    println!("\n=== DECISION GUIDE ===\n");

    println!("Use STATIC pipelines when:");
    println!("  ✓ Pipeline is known at compile time");
    println!("  ✓ Maximum performance is critical");
    println!("  ✓ You want compile-time type safety");
    println!("  ✓ Example: Library with fixed normalization strategy\n");

    println!("Use DYNAMIC pipelines when:");
    println!("  ✓ Pipeline comes from config files");
    println!("  ✓ Users can customize normalization steps");
    println!("  ✓ You need plugin-like extensibility");
    println!("  ✓ Example: User-configurable search engine, CMS, chat app\n");

    println!("HYBRID approach:");
    println!("  ✓ Provide static pipeline functions as presets");
    println!("  ✓ Allow dynamic overrides for power users");
    println!("  ✓ Example: search_pipeline() + custom stages from config");
}
