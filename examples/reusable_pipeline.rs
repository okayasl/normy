use std::{borrow::Cow, iter::FusedIterator};

use normy::{
    CaseFold, ENG, JPN, LowerCase, NFKC, NORMALIZE_WHITESPACE_FULL, NormalizePunctuation, Normy,
    NormyBuilder, RemoveDiacritics, SegmentWords, StripControlChars, StripFormatControls,
    StripHtml, StripMarkdown, TUR, UnifyWidth, ZHO,
    context::Context,
    process::FusablePipeline,
    stage::{Stage, StageError, StaticFusableStage, StaticIdentityAdapter},
};

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
// 2. REUSABLE PIPELINE FUNCTIONS (inlined, zero-cost)
// ————————————————————————————————
fn ascii_fast() -> NormyBuilder<impl FusablePipeline> {
    Normy::builder().add_stage(NORMALIZE_WHITESPACE_FULL)
}

fn machine_translation() -> NormyBuilder<impl FusablePipeline> {
    Normy::builder()
        .add_stage(NFKC)
        .add_stage(StripControlChars)
        .add_stage(StripFormatControls)
        .add_stage(NormalizePunctuation)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
}

fn markdown_processing() -> NormyBuilder<impl FusablePipeline> {
    Normy::builder()
        .add_stage(NFKC)
        .add_stage(StripMarkdown)
        .add_stage(StripControlChars)
        .add_stage(StripFormatControls)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
}

fn web_scraping() -> NormyBuilder<impl FusablePipeline> {
    Normy::builder()
        .add_stage(NFKC)
        .add_stage(StripHtml)
        .add_stage(StripControlChars)
        .add_stage(StripFormatControls)
        .add_stage(UnifyWidth)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
}

fn search() -> NormyBuilder<impl FusablePipeline> {
    Normy::builder()
        .add_stage(NFKC)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(StripFormatControls)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .add_stage(SegmentWords)
}

fn cjk_search() -> NormyBuilder<impl FusablePipeline> {
    Normy::builder()
        .add_stage(NFKC)
        .add_stage(UnifyWidth)
        .add_stage(StripFormatControls)
        .add_stage(StripControlChars)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .add_stage(SegmentWords)
}

fn minimum() -> NormyBuilder<impl FusablePipeline> {
    Normy::builder()
        .add_stage(NFKC)
        .add_stage(StripControlChars)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
}

fn maximum() -> NormyBuilder<impl FusablePipeline> {
    Normy::builder()
        .add_stage(NFKC)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(StripFormatControls)
        .add_stage(StripControlChars)
        .add_stage(UnifyWidth)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
        .add_stage(SegmentWords)
}

fn social_media() -> NormyBuilder<impl FusablePipeline> {
    Normy::builder()
        .add_stage(NFKC)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .add_stage(UnifyWidth)
        .add_stage(StripControlChars)
        .add_stage(StripFormatControls)
        .add_stage(NormalizePunctuation)
        .add_stage(NORMALIZE_WHITESPACE_FULL)
}

fn custom_social_media() -> NormyBuilder<impl FusablePipeline> {
    social_media().add_stage(StripEmoji)
}

fn main() {
    println!("=== NORMY PIPELINE EXAMPLES ===\n");

    // Build language-specific normalizers upfront
    let tr = search().lang(TUR).build();
    let en = search().lang(ENG).build();
    let zh = search().lang(ZHO).build();
    let ja = cjk_search().lang(JPN).build();

    // ========================================
    // 1. ASCII_FAST - Ultra-fast for clean text
    // ========================================
    println!("1. ASCII_FAST Pipeline");
    println!("   Use case: Pre-validated input, performance-critical paths\n");

    let ascii_fast_en = ascii_fast().lang(ENG).build();
    let text = "Hello    World\t\n  Fast   Processing";
    println!("   Input:  '{}'", text);
    println!("   Output: '{}'\n", ascii_fast_en.normalize(text).unwrap());
    // → "Hello World Fast Processing"

    // ========================================
    // 2. MACHINE_TRANSLATION - Preserve semantics
    // ========================================
    println!("2. MACHINE_TRANSLATION Pipeline");
    println!("   Use case: MT preprocessing, preserving linguistic features\n");

    let machine_translation_en = machine_translation().lang(ENG).build();
    let text = "Café naïve — \"smart quotes\" & dashes…";
    println!("   Input:  {}", text);
    println!(
        "   Output: {}\n",
        machine_translation_en.normalize(text).unwrap()
    );
    // → "Café naïve - \"smart quotes\" & dashes..."
    // Note: Preserves diacritics, normalizes punctuation

    // ========================================
    // 3. MARKDOWN_PROCESSING - Clean docs
    // ========================================
    println!("3. MARKDOWN_PROCESSING Pipeline");
    println!("   Use case: SSGs, documentation, README parsing\n");

    let markdown_processing_en = markdown_processing().lang(ENG).build();
    let text = r#"
# Heading
**Bold** and *italic* text
- List item
[Link](https://example.com)
`code` and ```blocks```
"#;
    println!("   Input:\n{}", text);
    println!(
        "   Output: {}\n",
        markdown_processing_en.normalize(text).unwrap()
    );
    // → "Heading Bold and italic text List item Link code and blocks"

    // ========================================
    // 4. WEB_SCRAPING - Extract clean text
    // ========================================
    println!("4. WEB_SCRAPING Pipeline");
    println!("   Use case: Content extraction, web crawlers\n");

    let web_scraping_en = web_scraping().lang(ENG).build();
    let text = r#"
<div class="article">
    <h1>Title</h1>
    <p>Café　in　Tokyo</p>  <!-- fullwidth spaces -->
    <script>alert('ignore me')</script>
</div>
"#;
    println!("   Input: {}", text.trim());
    println!("   Output: {}\n", web_scraping_en.normalize(text).unwrap());
    // → "Title Café in Tokyo"
    // Note: Strips HTML, normalizes fullwidth spaces

    // ========================================
    // 5. SEARCH - The gold standard
    // ========================================
    println!("5. SEARCH Pipeline");
    println!("   Use case: Search engines, autocomplete, fuzzy matching\n");

    let text = "café naïve résumé İstanbul 東京";
    println!("   Input: {}\n", text);

    println!("   → Turkish user: {}", tr.normalize(text).unwrap());
    // → "cafe naive resume istanbul 東京"

    println!("   → English user: {}", en.normalize(text).unwrap());
    // → "cafe naive resume istanbul 東京"

    println!("   → Chinese user: {}", zh.normalize(text).unwrap());
    // → "cafe naive resume istanbul 東 京" (CJK unigram)
    println!();

    // ========================================
    // 6. CJK_SEARCH - Optimized for Asian text
    // ========================================
    println!("6. CJK_SEARCH Pipeline");
    println!("   Use case: Chinese/Japanese/Korean search systems\n");

    let text = "東京タワー　ＴＯＫＹＯ　123"; // Mixed fullwidth
    println!("   Input:  {}", text);
    println!("   Output: {}\n", ja.normalize(text).unwrap());
    // → "東 京 タ ワ ー TOKYO 123"
    // Note: Fullwidth → ASCII, CJK unigram segmentation

    // ========================================
    // 7. MINIMUM - Preserve everything
    // ========================================
    println!("7. MINIMUM Pipeline");
    println!("   Use case: NER, POS tagging, grammar checking\n");

    let minimum_en = minimum().lang(ENG).build();
    let text = "Dr. Smith's COVID-19 test was NEGATIVE!!!";
    println!("   Input:  {}", text);
    println!("   Output: {}\n", minimum_en.normalize(text).unwrap());
    // → "Dr. Smith's COVID-19 test was NEGATIVE!!!"
    // Note: Only whitespace normalized, everything else preserved

    // ========================================
    // 8. MAXIMUM - Nuclear cleaning
    // ========================================
    println!("8. MAXIMUM Pipeline");
    println!("   Use case: Deduplication, fuzzy matching, log cleaning\n");

    let maximum_en = maximum().lang(ENG).build();
    let text = r#"
<HTML>Café  "Naïve"   İstanbul　東京タワー
**Bold** [Link](url) 
UPPERCASE lowercase MixedCase
"#;
    println!("   Input: {}", text.trim());
    println!("   Output: {}\n", maximum_en.normalize(text).unwrap());
    // → "cafe naive istanbul 東 京 タ ワ ー bold link uppercase lowercase mixedcase"
    // Note: Everything normalized aggressively

    // ========================================
    // 9. SOCIAL_MEDIA - Handle noisy UGC
    // ========================================
    println!("9. SOCIAL_MEDIA Pipeline");
    println!("   Use case: Twitter, Reddit, forum posts\n");

    let social_media_en = social_media().lang(ENG).build();
    let custom_social_media_en = custom_social_media().lang(ENG).build();

    let text = r#"
OMG!!! Check this café  ☕️
<script>alert()</script>
**SO** excited about #AI2024
Visit: https://example.com
"#;
    println!("   Input: {}", text.trim());
    println!(
        "Pipeline (without StripEmoji stage)   Output: {}\n",
        social_media_en.normalize(text).unwrap()
    );
    // → "omg!!! check this cafe ☕️ so excited about #ai2024 visit: https://example.com"
    // Note: Aggressive cleaning while preserving emoji

    println!(
        "Custom (with custom StripEmoji stage)   Output: {}\n",
        custom_social_media_en.normalize(text).unwrap()
    );
    // → "omg!!! check this cafe so excited about #ai2024 visit: https://example.com"
    // Note: Aggressive cleaning and striping emoji

    // ========================================
    // COMPARISON: Same text, different pipelines
    // ========================================
    println!("\n=== PIPELINE COMPARISON ===");
    let sample = "Café \"Naïve\" İstanbul ❤️ 東京 <b>Bold</b>";
    println!("Input: {}\n", sample);

    println!(
        "ascii_fast:          {}",
        ascii_fast_en.normalize(sample).unwrap()
    );

    println!(
        "minimum:             {}",
        minimum_en.normalize(sample).unwrap()
    );

    println!(
        "machine_translation: {}",
        machine_translation_en.normalize(sample).unwrap()
    );

    println!(
        "web_scraping:        {}",
        web_scraping_en.normalize(sample).unwrap()
    );

    println!("search:              {}", en.normalize(sample).unwrap());

    println!(
        "social_media:        {}",
        social_media_en.normalize(sample).unwrap()
    );

    println!(
        "custom_social_media: {}",
        custom_social_media_en.normalize(sample).unwrap()
    );

    println!(
        "maximum:             {}",
        maximum_en.normalize(sample).unwrap()
    );

    // ========================================
    // LANGUAGE-AWARE EXAMPLES
    // ========================================
    println!("\n=== LANGUAGE-AWARE NORMALIZATION ===");

    let turkish_text = "İSTANBUL istanbul İzmir";
    println!("\nTurkish dotted-i handling:");
    println!("Input: {}", turkish_text);
    println!("  Turkish locale: {}", tr.normalize(turkish_text).unwrap());
    // → "istanbul istanbul izmir" (correct Turkish lowercasing)
    println!("  English locale: {}", en.normalize(turkish_text).unwrap());
    // → "i̇stanbul istanbul i̇zmir" (incorrect, shows why language matters)

    let french_text = "Où est l'hôtel à Zürich?";
    println!("\nFrench diacritics:");
    println!("Input: {}", french_text);
    let search_en = search().lang(ENG).build();
    println!(
        "  With RemoveDiacritics: {}",
        search_en.normalize(french_text).unwrap()
    );
    // → "ou est l'hotel a zurich?"
    let minimum_en_fr = minimum().lang(ENG).build();
    println!(
        "  Without (minimum):     {}",
        minimum_en_fr.normalize(french_text).unwrap()
    );
    // → "Où est l'hôtel à Zürich?"

    let cjk_text = "東京都渋谷区";
    println!("\nCJK segmentation:");
    println!("Input: {}", cjk_text);
    let cjk_search_zh = cjk_search()
        .lang(ZHO)
        .modify_lang(|le| le.set_unigram_cjk(true))
        .build();
    println!(
        "  With UnigramCJK: {}",
        cjk_search_zh.normalize(cjk_text).unwrap()
    );
    // → "東 京 都 渋 谷 区" (character-level tokens)
    let minimum_zh = minimum().lang(ZHO).build();
    println!(
        "  Without:         {}",
        minimum_zh.normalize(cjk_text).unwrap()
    );
    // → "東京都渋谷区" (no segmentation)

    println!("\n=== FINAL COMPARISON TABLE ===");
    let input = "Café naïve İstanbul ❤️ 東京 <b>Bold</b> １２３";

    let ascii_fast_final = ascii_fast().lang(ENG).build();
    let minimum_final = minimum().lang(ENG).build();
    let machine_translation_final = machine_translation().lang(ENG).build();
    let web_scraping_final = web_scraping().lang(ENG).build();
    let search_final = search().lang(ENG).build();
    let social_media_final = social_media().lang(ENG).build();
    let custom_social_media_final = custom_social_media().lang(ENG).build();
    let maximum_final = maximum().lang(ENG).build();

    println!(
        "{:20} → {}",
        "ascii_fast",
        ascii_fast_final.normalize(input).unwrap()
    );
    println!(
        "{:20} → {}",
        "minimum",
        minimum_final.normalize(input).unwrap()
    );
    println!(
        "{:20} → {}",
        "machine_translation",
        machine_translation_final.normalize(input).unwrap()
    );
    println!(
        "{:20} → {}",
        "web_scraping",
        web_scraping_final.normalize(input).unwrap()
    );
    println!(
        "{:20} → {}",
        "search",
        search_final.normalize(input).unwrap()
    );
    println!(
        "{:20} → {}",
        "social_media",
        social_media_final.normalize(input).unwrap()
    );
    println!(
        "{:20} → {}",
        "custom_social_media",
        custom_social_media_final.normalize(input).unwrap()
    );
    println!(
        "{:20} → {}",
        "maximum",
        maximum_final.normalize(input).unwrap()
    );
}
