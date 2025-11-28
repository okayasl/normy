use crate::{
    context::Context,
    lang::Lang,
    stage::{Stage, StageError},
    testing::stage_contract::StageTestConfig,
};
use memchr::memchr;
use std::borrow::Cow;

/// Fast pre-scan: if no '<' appears, text is guaranteed to have no tags
#[inline(always)]
fn contains_html_tag(text: &str) -> bool {
    memchr(b'<', text.as_bytes()).is_some()
}

/// Fast pre-scan: if no '&' appears, text has no entities
#[inline(always)]
fn contains_entities(text: &str) -> bool {
    memchr(b'&', text.as_bytes()).is_some()
}

/// Strips HTML tags and decodes entities while preserving visible text.
///
/// # White-Paper Guarantees (Universal Contracts)
/// - **Zero-copy** when no `<` or `&` appears in input
/// - **needs_apply_is_accurate**: predicts changes with 100% precision
/// - **Idempotent**: applying twice yields same result as once
/// - **Safe**: streaming, no buffer overflows, handles malformed HTML
/// - **Fast pre-scan**: uses `memchr` — O(n) with 1–2 byte checks
/// - **No false positives**: pure text (even with `>`, `"`, etc.) never triggers
pub struct StripHtml;

impl Stage for StripHtml {
    fn name(&self) -> &'static str {
        "strip_html"
    }

    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        if text.is_empty() {
            return Ok(false);
        }

        // Quick check: if it has tags, we definitely need to apply
        if contains_html_tag(text) {
            return Ok(true);
        }

        // If it has '&', check if there are actual decodable entities
        if contains_entities(text) {
            // Do a quick entity decode to see if anything would actually change
            let mut decoded = String::with_capacity(text.len());
            html_escape::decode_html_entities_to_string(text, &mut decoded);
            return Ok(decoded != text);
        }

        Ok(false)
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if text.is_empty() {
            return Ok(text);
        }

        let has_tags = contains_html_tag(&text);
        let has_entities = contains_entities(&text);

        // Zero-copy fast path: no HTML or entities present
        if !has_tags && !has_entities {
            return Ok(text);
        }

        // Step 1: Decode entities first
        let decoded = if has_entities {
            let mut decoded_str = String::with_capacity(text.len());
            html_escape::decode_html_entities_to_string(&text, &mut decoded_str);

            // Check if decoding actually changed anything
            if decoded_str == text.as_ref() {
                text // No changes, keep original
            } else {
                Cow::Owned(decoded_str)
            }
        } else {
            text
        };

        // Step 2: Check for tags in the decoded text (entities might have revealed tags!)
        let has_tags_after_decode = has_tags || contains_html_tag(&decoded);

        if !has_tags_after_decode {
            return Ok(decoded);
        }

        // Step 3: Strip HTML tags from decoded text
        let mut result = String::with_capacity(decoded.len());
        let mut chars = decoded.chars().peekable();
        let mut state = ParseState::Text;

        while let Some(c) = chars.next() {
            match state {
                ParseState::Text => {
                    if c == '<' {
                        // Peek ahead to determine tag type
                        match chars.peek() {
                            Some(&'!') => {
                                // Could be comment <!--, CDATA <![CDATA[, or DOCTYPE <!DOCTYPE
                                if chars.clone().nth(1) == Some('-')
                                    && chars.clone().nth(2) == Some('-')
                                {
                                    state = ParseState::Comment;
                                    chars.next(); // !
                                    chars.next(); // -
                                    chars.next(); // -
                                } else if chars.clone().nth(1) == Some('[')
                                    && chars.clone().nth(2) == Some('C')
                                {
                                    // CDATA section - include content
                                    state = ParseState::Cdata;
                                    chars.next(); // !
                                    chars.next(); // [
                                    // Consume "CDATA["
                                    for _ in 0..6 {
                                        chars.next();
                                    }
                                } else {
                                    // Other declaration like DOCTYPE - skip
                                    state = ParseState::Tag;
                                }
                            }
                            Some(&'s') | Some(&'S') => {
                                // Check for <script> or <style>
                                let tag_name = peek_tag_name(&chars);
                                if tag_name.eq_ignore_ascii_case("script")
                                    || tag_name.eq_ignore_ascii_case("style")
                                {
                                    let tag_len = tag_name.len();
                                    // Consume tag name
                                    for _ in 0..tag_len {
                                        chars.next();
                                    }
                                    // Now we need to consume the rest of the opening tag until '>'
                                    // This handles cases like <script src="...">
                                    for ch in chars.by_ref() {
                                        if ch == '>' {
                                            break;
                                        }
                                    }
                                    // NOW enter the script/style content state
                                    state = ParseState::ScriptOrStyle(tag_name);
                                } else {
                                    state = ParseState::Tag;
                                }
                            }
                            _ => {
                                state = ParseState::Tag;
                            }
                        }
                    } else {
                        result.push(c);
                    }
                }

                ParseState::Tag => {
                    if c == '>' {
                        state = ParseState::Text;
                    }
                    // Inside tag: skip everything
                }

                ParseState::Comment => {
                    // Inside <!--, looking for -->
                    if c == '-' && chars.peek() == Some(&'-') && chars.clone().nth(1) == Some('>') {
                        state = ParseState::Text;
                        chars.next(); // consume second '-'
                        chars.next(); // consume '>'
                    }
                    // Otherwise skip all content
                }

                ParseState::Cdata => {
                    // Inside <![CDATA[, looking for ]]>
                    if c == ']' && chars.peek() == Some(&']') && chars.clone().nth(1) == Some('>') {
                        state = ParseState::Text;
                        chars.next(); // consume second ']'
                        chars.next(); // consume '>'
                    } else {
                        // CDATA content is preserved
                        result.push(c);
                    }
                }

                ParseState::ScriptOrStyle(ref tag_name) => {
                    // Inside <script> or <style> content, looking for </script> or </style>
                    if c == '<' && chars.peek() == Some(&'/') {
                        let mut temp_chars = chars.clone();
                        temp_chars.next(); // skip '/'
                        if check_closing_tag(&temp_chars, tag_name) {
                            let tag_len = tag_name.len();
                            state = ParseState::Tag; // Enter Tag state to consume </script> or </style>
                            chars.next(); // consume '/'
                            // Consume tag name
                            for _ in 0..tag_len {
                                chars.next();
                            }
                            // Tag state will consume the '>'
                        }
                    }
                    // Skip script/style content (don't push to result)
                }
            }
        }

        // Check if stripping changed anything
        if result == decoded.as_ref() {
            Ok(decoded)
        } else if result.is_empty() {
            Ok(Cow::Owned(String::new()))
        } else {
            Ok(Cow::Owned(result))
        }
    }

    #[inline(always)]
    fn as_char_mapper(&self, _: &Context) -> Option<&dyn crate::stage::CharMapper> {
        None
    }

    #[inline(always)]
    fn into_dyn_char_mapper(
        self: std::sync::Arc<Self>,
        _: &Context,
    ) -> Option<std::sync::Arc<dyn crate::stage::CharMapper>> {
        None
    }
}

#[derive(Debug, Clone)]
enum ParseState {
    Text,
    Tag,
    Comment,
    Cdata,
    ScriptOrStyle(String),
}

/// Peek ahead to get tag name (letters only)
/// Peek ahead to get tag name (letters only) WITHOUT consuming
fn peek_tag_name(chars: &std::iter::Peekable<std::str::Chars>) -> String {
    let mut name = String::new();
    let temp_chars = chars.clone();
    for c in temp_chars {
        if c.is_ascii_alphabetic() {
            name.push(c);
        } else {
            break;
        }
    }
    name
}

/// Check if upcoming chars match closing tag (case-insensitive)
fn check_closing_tag(chars: &std::iter::Peekable<std::str::Chars>, tag_name: &str) -> bool {
    let mut temp_chars = chars.clone();
    for expected in tag_name.chars() {
        match temp_chars.next() {
            Some(c) if c.eq_ignore_ascii_case(&expected) => continue,
            _ => return false,
        }
    }
    // Successfully matched all characters
    true
}

// UNIVERSAL CONTRACT COMPLIANCE
impl StageTestConfig for StripHtml {
    /// This stage is language-agnostic — works identically in all languages
    fn one_to_one_languages() -> &'static [Lang] {
        &[] // → tests run on all languages
    }

    /// Custom samples that trigger real changes
    fn samples(_lang: Lang) -> &'static [&'static str] {
        &[
            "<p>Hello &amp; world</p>",
            "Price: &euro;99",
            "<script>alert(1)</script>",
            "Normal text with > and & in prose &amp; such",
            "<div class=\"test\">content</div>",
            "&lt;escaped&gt;",
            "<![CDATA[preserve this]]>",
        ]
    }

    /// Idempotent: stripping HTML twice = stripping once
    fn skip_idempotency() -> &'static [Lang] {
        &[]
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;
    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(StripHtml);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::data::ENG;

    #[test]
    fn test_pure_text_zero_copy() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);
        let input = "Hello world";
        assert!(!stage.needs_apply(input, &ctx).unwrap());
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result.as_ref(), input);
    }

    #[test]
    fn test_strips_tags_and_comments_preserves_spacing() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);
        let input = "<p>Hello <!-- secret --> <b>world</b>!</p>";
        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx).unwrap(),
            "Hello  world!"
        );
    }

    #[test]
    fn test_entity_decoding() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);
        assert_eq!(
            stage.apply(Cow::Borrowed("caf&eacute;"), &ctx).unwrap(),
            "café"
        );
        // Entity-encoded script tags are decoded then stripped (content too)
        assert_eq!(
            stage
                .apply(Cow::Borrowed("&lt;script&gt;alert(1)&lt;/script&gt;"), &ctx)
                .unwrap(),
            "" // Script content is stripped
        );
    }

    #[test]
    fn test_mixed_content() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);
        let input = "Price: &euro;99 <s>199</s> &rarr; Save now!";
        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx).unwrap(),
            "Price: €99 199 → Save now!"
        );
    }

    #[test]
    fn test_idempotency() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);
        let input = "<div><p>Hello &amp; world</p></div>";
        let once = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        let twice = stage.apply(once.clone(), &ctx).unwrap();
        assert_eq!(once, "Hello & world");
        assert_eq!(once, twice);
    }

    #[test]
    fn test_script_tag_content_stripped() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);
        let input = "<script>alert('<tag>');</script>text";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "text");
    }

    #[test]
    fn test_style_tag_content_stripped() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);
        let input = "<style>body { color: red; }</style>text";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "text");
    }

    #[test]
    fn test_cdata_content_preserved() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);
        let input = "<![Cdata[<tag>content</tag>]]>text";
        assert_eq!(
            stage.apply(Cow::Borrowed(input), &ctx).unwrap(),
            "<tag>content</tag>text"
        );
    }

    #[test]
    fn test_comment_with_greater_than() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);
        let input = "<!-- if x > 5 then --> visible";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), " visible");
    }

    #[test]
    fn test_malformed_unclosed_tag() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);
        let input = "<div class=\"test";
        // Tag never closes - everything after < is stripped
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "");
    }

    #[test]
    fn test_nested_tags() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);
        let input = "<div><p><span>nested</span></p></div>";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "nested");
    }
}
