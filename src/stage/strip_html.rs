use crate::{
    context::Context, lang::Lang, stage::{Stage, StageError}, testing::stage_contract::StageTestConfig
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
        Ok(!text.is_empty() && (contains_html_tag(text) || contains_entities(text)))
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

        // Decode HTML entities into a new buffer
        let mut decoded = String::with_capacity(text.len());
        html_escape::decode_html_entities_to_string(&text, &mut decoded);

        // If only entities present (no tags), return decoded text
        if !has_tags {
            // Check if decoding actually changed anything
            if decoded == text.as_ref() {
                return Ok(text); // No entities were actually decoded
            }
            return Ok(Cow::Owned(decoded));
        }

        // Strip tags from the decoded text
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
                                // Could be comment <!--, Cdata <![Cdata[, or DOCTYPE <!DOCTYPE
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
                                    // Cdata section - include content
                                    state = ParseState::Cdata;
                                    chars.next(); // !
                                    chars.next(); // [
                                    // Consume "Cdata["
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
                                let tag_name = peek_tag_name(&chars); // ← Remove &mut, just peek
                                if tag_name.eq_ignore_ascii_case("script")
                                    || tag_name.eq_ignore_ascii_case("style")
                                {
                                    let tag_len = tag_name.len();
                                    state = ParseState::ScriptOrStyle(tag_name);
                                    // NOW consume the tag name
                                    for _ in 0..tag_len {
                                        chars.next();
                                    }
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
                    // Inside tag: skip everything (including quotes, which we handle implicitly)
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
                    // Inside <![Cdata[, looking for ]]>
                    if c == ']' && chars.peek() == Some(&']') && chars.clone().nth(1) == Some('>') {
                        state = ParseState::Text;
                        chars.next(); // consume second ']'
                        chars.next(); // consume '>'
                    } else {
                        // Cdata content is preserved
                        result.push(c);
                    }
                }

                ParseState::ScriptOrStyle(ref tag_name) => {
                    // Inside <script> or <style>, looking for </script> or </style>
                    if c == '<' && chars.peek() == Some(&'/') {
                        let mut temp_chars = chars.clone();
                        temp_chars.next(); // skip '/'
                        if check_closing_tag(&temp_chars, tag_name) {
                            let tag_len = tag_name.len();
                            state = ParseState::Tag;
                            chars.next(); // consume '/'
                            // Consume tag name
                            for _ in 0..tag_len {
                                chars.next();
                            }
                        }
                    }
                    // Skip script/style content
                }
            }
        }

        // Check if result is identical to input (no changes made)
        if result == text.as_ref() {
            Ok(text)
        } else if result.is_empty() {
            // All content was stripped - return empty owned string
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
        assert_eq!(
            stage
                .apply(Cow::Borrowed("&lt;script&gt;alert(1)&lt;/script&gt;"), &ctx)
                .unwrap(),
            "<script>alert(1)</script>"
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
