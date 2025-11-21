//! Strips Markdown formatting while preserving visible text and logical structure.
//!
//! # Behavior
//! - **Formatting**: `**bold**` → `bold`, `_italic_` → `italic`, `~~strike~~` → `strike`
//! - **Structure**: Headers, paragraphs, and block quotes are separated by newlines to prevent word-gluing.
//! - **Lists**: Bullets are removed, but items are separated by newlines. Task lists (`[x]`) are preserved as text.
//! - **Tables**: Converted to space-separated text to preserve word boundaries (e.g., `| A | B |` → `A B`).
//! - **Links/Images**: `[text](url)` → `text`, `![alt](url)` → `alt`.
//! - **Passthrough**: HTML tags (`<div>`), Inline Math (`$E=mc^2$`), and Display Math are preserved for subsequent stages.
//!
//! # Performance
//! - **Zero-copy**: Uses a fast byte scan (`memchr`) to skip processing if no Markdown syntax is detected.
//! - **Streaming**: Uses `pulldown-cmark` event iterator, avoiding intermediate AST allocation.
//! - **Allocation**: Allocates a new string only when Markdown syntax is actually removed or modified.

use crate::{
    context::Context,
    stage::{Stage, StageError},
};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::borrow::Cow;

/// Fast pre-scan: checks for common Markdown indicators.
/// If none appear, we skip the parser entirely.
#[inline(always)]
fn contains_markdown_bytes(text: &str) -> bool {
    let bytes = text.as_bytes();
    // Check for common markdown markers.
    // Note: We must check '-' and '+' because they are used for lists.
    // While this might flag hyphenated text as "markdown", correct list stripping is prioritized.
    memchr::memchr3(b'#', b'*', b'_', bytes).is_some()
        || memchr::memchr3(b'`', b'[', b'>', bytes).is_some()
        || memchr::memchr3(b'|', b'~', b'!', bytes).is_some()
        || memchr::memchr2(b'-', b'+', bytes).is_some()
}

pub struct StripMarkdown;

impl Stage for StripMarkdown {
    fn name(&self) -> &'static str {
        "strip_markdown"
    }

    #[inline(always)]
    fn needs_apply(&self, text: &str, _ctx: &Context) -> Result<bool, StageError> {
        Ok(!text.is_empty() && contains_markdown_bytes(text))
    }

    fn apply<'a>(&self, text: Cow<'a, str>, _ctx: &Context) -> Result<Cow<'a, str>, StageError> {
        if text.is_empty() || !contains_markdown_bytes(&text) {
            return Ok(text);
        }

        let mut out = String::with_capacity(text.len());

        // Enable Strikethrough, Tables, Tasklists, Footnotes
        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_FOOTNOTES);
        // We enable MATH to parse it correctly and preserve delimiters
        options.insert(Options::ENABLE_MATH);

        let parser = Parser::new_ext(&text, options);

        for event in parser {
            match event {
                // Content we want to keep
                Event::Text(t)
                | Event::Code(t)
                | Event::Html(t)
                | Event::InlineHtml(t)
                | Event::FootnoteReference(t) => {
                    out.push_str(&t);
                }

                // Mathematical formulas: Preserve syntax for NLP tokenizers
                Event::InlineMath(t) => {
                    out.push('$');
                    out.push_str(&t);
                    out.push('$');
                }
                Event::DisplayMath(t) => {
                    out.push_str("$$");
                    out.push_str(&t);
                    out.push_str("$$");
                }

                // Spacing handling
                Event::SoftBreak => out.push(' '),
                Event::HardBreak | Event::Rule => out.push('\n'),

                // Task Lists: convert [x] to text so NLP sees the status
                Event::TaskListMarker(checked) => {
                    out.push_str(if checked { "[x] " } else { "[ ] " });
                }

                // START: Handle start of blocks to ensure separation from preceding content
                // Collapsed match to avoid clippy warnings and duplication
                Event::Start(
                    Tag::Paragraph
                    | Tag::Heading { .. }
                    | Tag::BlockQuote(_)
                    | Tag::CodeBlock(_)
                    | Tag::List(_)
                    | Tag::Item
                    | Tag::Table(_)
                    | Tag::TableRow,
                ) => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                }

                // END: Handle end of blocks to ensure separation from following content
                Event::End(
                    TagEnd::Paragraph
                    | TagEnd::Heading(_)
                    | TagEnd::BlockQuote(_)
                    | TagEnd::CodeBlock
                    | TagEnd::List(_)
                    | TagEnd::Item
                    | TagEnd::Table
                    | TagEnd::TableHead
                    | TagEnd::TableRow,
                ) => {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }

                // Table cells need a space separator (Column A | Column B)
                Event::End(TagEnd::TableCell) => {
                    if !out.ends_with(|c: char| c.is_whitespace()) {
                        out.push(' ');
                    }
                }

                // Catch-all for other events (Links, Images, Custom containers, etc.)
                // This pattern is now reachable because the explicit Start/End arms above
                // don't match everything (e.g., they don't match Tag::Link).
                _ => {}
            }
        }

        // Optimization: If the output happens to be identical, return original
        if out.len() == text.len() && out == text.as_ref() {
            Ok(text)
        } else {
            // Trim trailing whitespace created by the block logic
            if out.ends_with(char::is_whitespace) {
                let trimmed = out.trim_end();
                Ok(Cow::Owned(trimmed.to_string()))
            } else {
                Ok(Cow::Owned(out))
            }
        }
    }

    #[inline]
    fn as_char_mapper(&self, _ctx: &Context) -> Option<&dyn crate::stage::CharMapper> {
        None
    }

    #[inline]
    fn into_dyn_char_mapper(
        self: std::sync::Arc<Self>,
        _ctx: &Context,
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
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);
        let input = "Just plain text with no markdown at all";

        // "Just plain text..." doesn't contain checked markdown bytes
        assert!(!stage.needs_apply(input, &ctx).unwrap());
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result.as_ref(), input);
    }

    #[test]
    fn test_basic_formatting() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        // Covers Bold, Italic, Strikethrough, Code
        let input = "**Bold** and _Italic_ and ~~Strike~~ and `code`";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert_eq!(result, "Bold and Italic and Strike and code");
    }

    #[test]
    fn test_structure_spacing() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        // Ensures headers are separated from paragraphs by newlines
        let input = r#"
# Heading 1
Paragraph text.
## Heading 2
More text.
"#;
        let result = stage.apply(Cow::Borrowed(input.trim()), &ctx).unwrap();
        let expected = "Heading 1\nParagraph text.\nHeading 2\nMore text.";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_lists_and_nested_items() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = r#"
- Item 1
- Item 2
  - Nested A
  - Nested B
"#;
        let result = stage.apply(Cow::Borrowed(input.trim()), &ctx).unwrap();
        // Bullets are removed, structure preserved via newlines
        let expected = "Item 1\nItem 2\nNested A\nNested B";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_task_lists() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = "- [x] Done\n- [ ] Todo";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        // Important for NLP: preserves the "checked" status as text
        let expected = "[x] Done\n[ ] Todo";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_tables() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = r#"
| Header A | Header B |
|----------|----------|
| Cell 1   | Cell 2   |
| Cell 3   | Cell 4   |
"#;
        let result = stage.apply(Cow::Borrowed(input.trim()), &ctx).unwrap();

        // Ensures cells are space-separated and rows are newline-separated
        let expected = "Header A Header B \nCell 1 Cell 2 \nCell 3 Cell 4";
        assert_eq!(result.trim(), expected);
    }

    #[test]
    fn test_links_and_images() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = "Click [here](https://example.com) to see ![A Cat](cat.png).";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        assert_eq!(result, "Click here to see A Cat.");
    }

    #[test]
    fn test_passthrough_elements() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        // HTML should be preserved for the `strip_html` stage
        // Math should be preserved for tokenization, including delimiters
        let input = "Text <div>HTML</div> $E=mc^2$ end.";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        assert_eq!(result, "Text <div>HTML</div> $E=mc^2$ end.");
    }

    #[test]
    fn test_blockquotes() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = "> Quote line 1\n> Quote line 2";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        // Soft breaks become spaces, block ends with newline
        assert_eq!(result, "Quote line 1 Quote line 2");
    }

    #[test]
    fn test_complex_nested_structure() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = r#"
# Title

Intro text with **bold**.

1. List item
   > With a quote inside
2. Second item

Final paragraph.
"#;
        let result = stage.apply(Cow::Borrowed(input.trim()), &ctx).unwrap();

        // Logic check:
        // Title\n
        // Intro text with bold.\n
        // List item\n
        // With a quote inside\n (Quote block ensures newline before)
        // Second item\n
        // Final paragraph.
        let expected = "Title\nIntro text with bold.\nList item\nWith a quote inside\nSecond item\nFinal paragraph.";
        assert_eq!(result, expected);
    }
}
