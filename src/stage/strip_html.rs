use crate::{
    context::Context,
    lang::Lang,
    stage::{CharMapper, Stage, StageError, StageIter},
    testing::stage_contract::StageTestConfig,
};
use memchr::memchr;
use std::{borrow::Cow, iter::Empty};

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
        let has_tags = contains_html_tag(&text);
        let has_entities = contains_entities(&text);

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
                                // <!-- comment -->
                                if chars.clone().nth(1) == Some('-')
                                    && chars.clone().nth(2) == Some('-')
                                {
                                    state = ParseState::Comment;
                                    chars.next(); // consume '!'
                                    chars.next(); // consume first '-'
                                    chars.next(); // consume second '-'
                                    continue;
                                }

                                // <![CDATA[ ... ]]>
                                let mut probe = chars.clone();
                                probe.next(); // skip the '!' we already peeked
                                if probe.next() == Some('[') {
                                    let chunk: String = probe.clone().take(6).collect();
                                    if chunk.eq_ignore_ascii_case("CDATA[") {
                                        state = ParseState::Cdata;
                                        chars.next(); // '!'
                                        chars.next(); // '['
                                        for _ in 0..6 {
                                            let _ = chars.next();
                                        } // "CDATA["
                                        continue;
                                    }
                                }

                                // Anything else starting with <! is a declaration/doctype → skip
                                state = ParseState::Tag;
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
                    if c == '"' || c == '\'' {
                        // Skip over quoted attribute values (including escaped quotes)
                        let quote = c;
                        while let Some(ch) = chars.next() {
                            if ch == '\\' {
                                // Skip escaped character
                                let _ = chars.next();
                            } else if ch == quote {
                                break;
                            }
                        }
                    } else if c == '>' {
                        state = ParseState::Text;
                    }
                    // otherwise just skip the character
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
    fn as_char_mapper(&self, _: &Context) -> Option<&dyn CharMapper> {
        None
    }

    #[inline(always)]
    fn into_dyn_char_mapper(
        self: std::sync::Arc<Self>,
        _: &Context,
    ) -> Option<std::sync::Arc<dyn CharMapper>> {
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
    // After the tag name we must see whitespace or '>'
    matches!(
        temp_chars.peek(),
        Some('>') | Some(' ') | Some('\t') | Some('\n') | Some('\r') | None
    )
}

impl StageIter for StripHtml {
    type Iter<'a> = Empty<char>;
}

// UNIVERSAL CONTRACT COMPLIANCE
impl StageTestConfig for StripHtml {
    fn one_to_one_languages() -> &'static [Lang] {
        &[]
    }

    fn samples(_lang: Lang) -> &'static [&'static str] {
        &[
            "<p>Hello &amp; world</p>",
            "Price: &euro;99",
            "<script>alert(1)</script>",
            "Normal text with > and & in prose &amp; such",
            "<div class=\"test\">content</div>",
            "&lt;escaped&gt;",
        ]
    }

    fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
        &["plain text", "hello world", "test123", ""]
    }

    fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
        &[
            ("<p>Hello</p>", "Hello"),
            ("&amp;", "&"),
            ("&lt;test&gt;", ""), // Entities decoded → tag stripped
            ("<b>bold</b>", "bold"),
            ("Price: &euro;99", "Price: €99"), // Entity decoded, no tags
        ]
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

    #[test]
    fn test_quoted_attributes_comprehensive() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);

        // Single quotes with >
        assert_eq!(
            stage
                .apply(Cow::Borrowed("<div title='x > y'>content</div>"), &ctx)
                .unwrap(),
            "content"
        );

        // Double quotes with >
        assert_eq!(
            stage
                .apply(Cow::Borrowed(r#"<div title="x > y">content</div>"#), &ctx)
                .unwrap(),
            "content"
        );

        // Multiple attributes
        assert_eq!(
            stage
                .apply(
                    Cow::Borrowed(r#"<div class="test" title="x > y" id="main">content</div>"#),
                    &ctx
                )
                .unwrap(),
            "content"
        );

        // Nested quotes
        assert_eq!(
            stage
                .apply(
                    Cow::Borrowed(r#"<div title='He said "hello"'>content</div>"#),
                    &ctx
                )
                .unwrap(),
            "content"
        );
    }

    #[test]
    fn test_escaped_quotes() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);

        let input = r#"<div title="He said \"hello\"">content</div>"#;
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "content");
    }

    #[test]
    fn test_script_style_with_attributes() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);

        let input = r#"<script type="text/javascript" src="file.js">alert(1);</script>text"#;
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "text");

        let input = r#"<style type="text/css">body{}</style>text"#;
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "text");
    }

    #[test]
    fn test_self_closing_tags() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);

        assert_eq!(
            stage
                .apply(Cow::Borrowed("<img src='test.jpg' />text"), &ctx)
                .unwrap(),
            "text"
        );
        assert_eq!(
            stage.apply(Cow::Borrowed("<br/>text"), &ctx).unwrap(),
            "text"
        );
        assert_eq!(
            stage.apply(Cow::Borrowed("<hr />text"), &ctx).unwrap(),
            "text"
        );
    }

    #[test]
    fn test_case_insensitive_special_tags() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);

        // Mixed case script
        assert_eq!(
            stage
                .apply(Cow::Borrowed("<ScRiPt>alert(1)</ScRiPt>text"), &ctx)
                .unwrap(),
            "text"
        );

        // Mixed case style
        assert_eq!(
            stage
                .apply(Cow::Borrowed("<STYLE>body{}</STYLE>text"), &ctx)
                .unwrap(),
            "text"
        );

        // Mixed case CDATA
        assert_eq!(
            stage
                .apply(Cow::Borrowed("<![CdAtA[content]]>text"), &ctx)
                .unwrap(),
            "contenttext"
        );
    }

    #[test]
    fn test_closing_tag_boundary() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);

        // Should NOT close at </scriptx>
        let input = "<script>code</scriptx>more";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "");

        // Should accept whitespace before >
        let input = "<script>code</script >text";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "text");
    }

    #[test]
    fn test_empty_and_valueless_attributes() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);

        let input = r#"<input disabled checked value="">text"#;
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "text");

        let input = r#"<button disabled>text</button>"#;
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "text");
    }

    #[test]
    fn test_consecutive_tags() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);

        assert_eq!(
            stage
                .apply(Cow::Borrowed("</div><div><span></span></div>"), &ctx)
                .unwrap(),
            ""
        );
        assert_eq!(
            stage
                .apply(Cow::Borrowed("<b><i><u>text</u></i></b>"), &ctx)
                .unwrap(),
            "text"
        );
    }

    #[test]
    fn test_whitespace_preservation() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);

        // Multiple spaces
        assert_eq!(
            stage
                .apply(Cow::Borrowed("<p>Hello   world</p>"), &ctx)
                .unwrap(),
            "Hello   world"
        );

        // Newlines between tags
        assert_eq!(
            stage
                .apply(Cow::Borrowed("<p>Line1</p>\n<p>Line2</p>"), &ctx)
                .unwrap(),
            "Line1\nLine2"
        );
    }

    #[test]
    fn test_attributes_without_quotes() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);

        let input = "<div class=test id=main>content</div>";
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "content");
    }

    #[test]
    fn test_mixed_entities_and_tags() {
        let stage = StripHtml;
        let ctx = Context::new(ENG);

        let input = "&lt;p&gt;<b>bold</b>&lt;/p&gt;";
        // Entity-encoded tags are decoded, then ALL tags (decoded + original) are stripped
        assert_eq!(stage.apply(Cow::Borrowed(input), &ctx).unwrap(), "bold");
    }
}
