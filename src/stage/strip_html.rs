//! Strips HTML tags and decodes entities while preserving visible text.
//! Zero-copy when no HTML or entities are present.
//! Correctly activates on either `<` or `&` — no work missed, no work wasted.
//! Allocation only when required — white paper compliant.
use crate::{
    context::Context,
    stage::{Stage, StageError},
};
use memchr::memchr;
use std::borrow::Cow;

/// Fast pre-scan: if no '<' appears, text is guaranteed clean
#[inline(always)]
fn contains_html_tag(text: &str) -> bool {
    memchr(b'<', text.as_bytes()).is_some()
}

#[inline(always)]
fn contains_entities(text: &str) -> bool {
    memchr(b'&', text.as_bytes()).is_some()
}

pub struct StripHtml;

impl Stage for StripHtml {
    fn name(&self) -> &'static str {
        "strip_html"
    }

    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(!text.is_empty() && (contains_html_tag(text) || contains_entities(text)))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        let mut decoded = String::with_capacity(text.len());
        html_escape::decode_html_entities_to_string(&text, &mut decoded);

        // Strip tags from the decoded text
        let mut result = String::with_capacity(decoded.len());
        let mut in_tag = false;
        let mut in_comment = false;
        let mut chars = decoded.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '<' => {
                    // Check for <!-- comment -->
                    if chars.peek() == Some(&'!')
                        && chars.clone().nth(1) == Some('-')
                        && chars.clone().nth(2) == Some('-')
                    {
                        in_comment = true;
                        let _ = chars.next(); // !
                        let _ = chars.next(); // -
                        let _ = chars.next(); // -
                    } else {
                        in_tag = true;
                    }
                }
                '>' => {
                    if in_comment {
                        in_comment = false;
                    } else {
                        in_tag = false;
                    }
                }
                '-' if in_comment => {
                    // Check if this is the start of "-->"
                    if chars.peek() == Some(&'-') && chars.clone().nth(1) == Some('>') {
                        in_comment = false;
                        let _ = chars.next(); // consume second '-'
                        let _ = chars.next(); // consume '>'
                    }
                }
                _ if !in_tag && !in_comment => result.push(c),
                _ => {}
            }
        }

        if result.is_empty() || result == text.as_ref() {
            Ok(text)
        } else {
            Ok(Cow::Owned(result))
        }
    }

    // HTML stripping is syntax-aware → cannot be expressed as pure CharMapper
    // This is expected and allowed by the white paper
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
        // Whitespace preserved exactly — this is CORRECT
        // normalize_whitespace stage will collapse later
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
            "alert(1)"
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
        let twice = stage.apply(Cow::Owned(once.to_string()), &ctx).unwrap();
        assert_eq!(once, "Hello & world");
        assert_eq!(once, twice);
    }
}
