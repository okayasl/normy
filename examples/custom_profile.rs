use std::{borrow::Cow, iter::FusedIterator, sync::Arc};

use normy::{
    CaseFold, ENG, JPN, LowerCase, NFKC, NormalizePunctuation, NormalizeWhitespace, NormyBuilder,
    RemoveDiacritics, StripControlChars, StripFormatControls, StripHtml, StripMarkdown, TUR,
    UnifyWidth, ZHO,
    context::Context,
    process::Process,
    profile::{
        Profile,
        preset::{
            ascii_fast, cjk_search, machine_translation, markdown_processing, maximum, minimum,
            search, social_media, web_scraping,
        },
    },
    stage::{CharMapper, Stage, StageError},
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

pub fn custom_social_media() -> Profile<impl Process> {
    Profile::builder("social_media")
        .add_stage(NFKC)
        .add_stage(StripHtml)
        .add_stage(StripMarkdown)
        .add_stage(LowerCase)
        .add_stage(CaseFold)
        .add_stage(RemoveDiacritics)
        .add_stage(UnifyWidth)
        .add_stage(StripControlChars)
        .add_stage(StripFormatControls)
        .add_stage(StripEmoji)
        .add_stage(NormalizePunctuation)
        .add_stage(NormalizeWhitespace::default())
        .build()
}

fn main() {
    println!("=== NORMY PROFILE EXAMPLES ===\n");

    // Setup normalizers for different languages
    let tr = NormyBuilder::default().lang(TUR).build();
    let en = NormyBuilder::default().lang(ENG).build();
    let zh = NormyBuilder::default().lang(ZHO).build();
    let ja = NormyBuilder::default().lang(JPN).build();

    // ========================================
    // 1. ASCII_FAST - Ultra-fast for clean text
    // ========================================
    println!("1. ASCII_FAST Profile");
    println!("   Use case: Pre-validated input, performance-critical paths\n");

    let text = "Hello    World\t\n  Fast   Processing";
    println!("   Input:  '{}'", text);
    println!(
        "   Output: '{}'\n",
        en.normalize_with_profile(&ascii_fast(), text).unwrap()
    );
    // → "Hello World Fast Processing"

    // ========================================
    // 2. MACHINE_TRANSLATION - Preserve semantics
    // ========================================
    println!("2. MACHINE_TRANSLATION Profile");
    println!("   Use case: MT preprocessing, preserving linguistic features\n");

    let text = "Café naïve — \"smart quotes\" & dashes…";
    println!("   Input:  {}", text);
    println!(
        "   Output: {}\n",
        en.normalize_with_profile(&machine_translation(), text)
            .unwrap()
    );
    // → "Café naïve - \"smart quotes\" & dashes..."
    // Note: Preserves diacritics, normalizes punctuation

    // ========================================
    // 3. MARKDOWN_PROCESSING - Clean docs
    // ========================================
    println!("3. MARKDOWN_PROCESSING Profile");
    println!("   Use case: SSGs, documentation, README parsing\n");

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
        en.normalize_with_profile(&markdown_processing(), text)
            .unwrap()
    );
    // → "Heading Bold and italic text List item Link code and blocks"

    // ========================================
    // 4. WEB_SCRAPING - Extract clean text
    // ========================================
    println!("4. WEB_SCRAPING Profile");
    println!("   Use case: Content extraction, web crawlers\n");

    let text = r#"
<div class="article">
    <h1>Title</h1>
    <p>Café　in　Tokyo</p>  <!-- fullwidth spaces -->
    <script>alert('ignore me')</script>
</div>
"#;
    println!("   Input: {}", text.trim());
    println!(
        "   Output: {}\n",
        en.normalize_with_profile(&web_scraping(), text).unwrap()
    );
    // → "Title Café in Tokyo"
    // Note: Strips HTML, normalizes fullwidth spaces

    // ========================================
    // 5. SEARCH - The gold standard
    // ========================================
    println!("5. SEARCH Profile");
    println!("   Use case: Search engines, autocomplete, fuzzy matching\n");

    let text = "café naïve résumé İstanbul 東京";
    println!("   Input: {}\n", text);

    println!(
        "   → Turkish user: {}",
        tr.normalize_with_profile(&search(), text).unwrap()
    );
    // → "cafe naive resume istanbul 東京"

    println!(
        "   → English user: {}",
        en.normalize_with_profile(&search(), text).unwrap()
    );
    // → "cafe naive resume istanbul 東京"

    println!(
        "   → Chinese user: {}",
        zh.normalize_with_profile(&search(), text).unwrap()
    );
    // → "cafe naive resume istanbul 東 京" (CJK unigram)
    println!();

    // ========================================
    // 6. CJK_SEARCH - Optimized for Asian text
    // ========================================
    println!("6. CJK_SEARCH Profile");
    println!("   Use case: Chinese/Japanese/Korean search systems\n");

    let text = "東京タワー　ＴＯＫＹＯ　123"; // Mixed fullwidth
    println!("   Input:  {}", text);
    println!(
        "   Output: {}\n",
        ja.normalize_with_profile(&cjk_search(), text).unwrap()
    );
    // → "東 京 タ ワ ー TOKYO 123"
    // Note: Fullwidth → ASCII, CJK unigram segmentation

    // ========================================
    // 7. MINIMUM - Preserve everything
    // ========================================
    println!("7. MINIMUM Profile");
    println!("   Use case: NER, POS tagging, grammar checking\n");

    let text = "Dr. Smith's COVID-19 test was NEGATIVE!!!";
    println!("   Input:  {}", text);
    println!(
        "   Output: {}\n",
        en.normalize_with_profile(&minimum(), text).unwrap()
    );
    // → "Dr. Smith's COVID-19 test was NEGATIVE!!!"
    // Note: Only whitespace normalized, everything else preserved

    // ========================================
    // 8. MAXIMUM - Nuclear cleaning
    // ========================================
    println!("8. MAXIMUM Profile");
    println!("   Use case: Deduplication, fuzzy matching, log cleaning\n");

    let text = r#"
<HTML>Café  "Naïve"   İstanbul　東京タワー
**Bold** [Link](url) 
UPPERCASE lowercase MixedCase
"#;
    println!("   Input: {}", text.trim());
    println!(
        "   Output: {}\n",
        en.normalize_with_profile(&maximum(), text).unwrap()
    );
    // → "cafe naive istanbul 東 京 タ ワ ー bold link uppercase lowercase mixedcase"
    // Note: Everything normalized aggressively

    // ========================================
    // 9. SOCIAL_MEDIA - Handle noisy UGC
    // ========================================
    println!("9. SOCIAL_MEDIA Profile");
    println!("   Use case: Twitter, Reddit, forum posts\n");

    let text = r#"
OMG!!! Check this café  ☕️
<script>alert()</script>
**SO** excited about #AI2024
Visit: https://example.com
"#;
    println!("   Input: {}", text.trim());
    println!(
        "Preset(without StripEmoji stage)   Output: {}\n",
        en.normalize_with_profile(&social_media(), text).unwrap()
    );
    // → "omg!!! check this cafe ☕️ so excited about #ai2024 visit: https://example.com"
    // Note: Aggressive cleaning while preserving emoji
    println!(
        "Custom (with custom StripEmoji stage)   Output: {}\n",
        en.normalize_with_profile(&custom_social_media(), text)
            .unwrap()
    );
    // → "omg!!! check this cafe so excited about #ai2024 visit: https://example.com"
    // Note: Aggressive cleaning and striping emoji

    // ========================================
    // COMPARISON: Same text, different profiles
    // ========================================
    println!("\n=== PROFILE COMPARISON ===");
    let sample = "Café \"Naïve\" İstanbul ❤️ 東京 <b>Bold</b>";
    println!("Input: {}\n", sample);

    println!(
        "ascii_fast:          {}",
        en.normalize_with_profile(&ascii_fast(), sample).unwrap()
    );

    println!(
        "minimum:             {}",
        en.normalize_with_profile(&minimum(), sample).unwrap()
    );

    println!(
        "machine_translation: {}",
        en.normalize_with_profile(&machine_translation(), sample)
            .unwrap()
    );

    println!(
        "web_scraping:        {}",
        en.normalize_with_profile(&web_scraping(), sample).unwrap()
    );

    println!(
        "search:              {}",
        en.normalize_with_profile(&search(), sample).unwrap()
    );

    println!(
        "social_media:        {}",
        en.normalize_with_profile(&social_media(), sample).unwrap()
    );

    println!(
        "custom_social_media: {}",
        en.normalize_with_profile(&custom_social_media(), sample)
            .unwrap()
    );

    println!(
        "maximum:             {}",
        en.normalize_with_profile(&maximum(), sample).unwrap()
    );

    // ========================================
    // LANGUAGE-AWARE EXAMPLES
    // ========================================
    println!("\n=== LANGUAGE-AWARE NORMALIZATION ===");

    let turkish_text = "İSTANBUL istanbul İzmir";
    println!("\nTurkish dotted-i handling:");
    println!("Input: {}", turkish_text);
    println!(
        "  Turkish locale: {}",
        tr.normalize_with_profile(&search(), turkish_text).unwrap()
    );
    // → "istanbul istanbul izmir" (correct Turkish lowercasing)
    println!(
        "  English locale: {}",
        en.normalize_with_profile(&search(), turkish_text).unwrap()
    );
    // → "i̇stanbul istanbul i̇zmir" (incorrect, shows why language matters)

    let french_text = "Où est l'hôtel à Zürich?";
    println!("\nFrench diacritics:");
    println!("Input: {}", french_text);
    println!(
        "  With RemoveDiacritics: {}",
        en.normalize_with_profile(&search(), french_text).unwrap()
    );
    // → "ou est l'hotel a zurich?"
    println!(
        "  Without (minimum):     {}",
        en.normalize_with_profile(&minimum(), french_text).unwrap()
    );
    // → "Où est l'hôtel à Zürich?"

    let cjk_text = "東京都渋谷区";
    println!("\nCJK segmentation:");
    println!("Input: {}", cjk_text);
    println!(
        "  With UnigramCJK: {}",
        zh.normalize_with_profile(&cjk_search(), cjk_text).unwrap()
    );
    // → "東 京 都 渋 谷 区" (character-level tokens)
    println!(
        "  Without:         {}",
        zh.normalize_with_profile(&minimum(), cjk_text).unwrap()
    );
    // → "東京都渋谷区" (no segmentation)

    println!("\n=== FINAL COMPARISON TABLE ===");
    let input = "Café naïve İstanbul ❤️ 東京 <b>Bold</b> １２３";
    println!(
        "{:20} → {}",
        "ascii_fast",
        en.normalize_with_profile(&ascii_fast(), input).unwrap()
    );
    println!(
        "{:20} → {}",
        "minimum",
        en.normalize_with_profile(&minimum(), input).unwrap()
    );
    println!(
        "{:20} → {}",
        "machine_translation",
        en.normalize_with_profile(&machine_translation(), input)
            .unwrap()
    );
    println!(
        "{:20} → {}",
        "web_scraping",
        en.normalize_with_profile(&web_scraping(), input).unwrap()
    );
    println!(
        "{:20} → {}",
        "search",
        en.normalize_with_profile(&search(), input).unwrap()
    );
    println!(
        "{:20} → {}",
        "social_media",
        en.normalize_with_profile(&social_media(), input).unwrap()
    );
    println!(
        "{:20} → {}",
        "custom_social_media",
        en.normalize_with_profile(&custom_social_media(), input)
            .unwrap()
    );
    println!(
        "{:20} → {}",
        "maximum",
        en.normalize_with_profile(&maximum(), input).unwrap()
    );
}
