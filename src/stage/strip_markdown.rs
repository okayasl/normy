use crate::{
    context::Context,
    lang::Lang,
    stage::{Stage, StageError, StaticFusableStage, StaticIdentityAdapter},
    testing::stage_contract::StageTestConfig,
};
use memchr::memchr3;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::{borrow::Cow, iter::FusedIterator};

/// Strips Markdown formatting while preserving visible text and logical structure.
///
/// This stage removes Markdown syntax using `pulldown-cmark` with extended options
/// (strikethrough, tables, task lists, footnotes, math) enabled:
///
/// - Inline formatting (bold, italic, strikethrough, links, images) is stripped
/// - Code and math content is emitted literally (including delimiters)
/// - Block structure (headings, lists, quotes, tables) is converted to newlines
/// - Task list markers become `[x] ` / `[ ] `
/// - Tables are linearized with spaces between cells and newlines between rows
///
/// Handles malformed or nested Markdown robustly.
///
/// Zero-copy when no Markdown syntax is detected.
///
/// Static fusion is intentionally disabled — the optimized parser-based implementation
/// is significantly faster than a character-by-character fused iterator.
#[derive(Debug, Default, Clone, Copy)]
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
        let mut out = String::with_capacity(text.len());

        // Enable Strikethrough, Tables, Tasklists, Footnotes, Math
        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_MATH);

        let parser = Parser::new_ext(text.as_ref(), options);
        let mut iter = parser.peekable();

        while let Some(event) = iter.next() {
            match event {
                // TEXT CONTENT
                Event::Text(t)
                | Event::Code(t)
                | Event::Html(t)
                | Event::InlineHtml(t)
                | Event::FootnoteReference(t) => {
                    out.push_str(&t);
                }

                // MATH INLINE
                Event::InlineMath(t) => {
                    out.push('$');
                    out.push_str(&t);
                    out.push('$');
                }

                // MATH BLOCK
                Event::DisplayMath(t) => {
                    out.push_str("$$");
                    out.push_str(&t);
                    out.push_str("$$");
                }

                // LINE BREAKS
                Event::SoftBreak => out.push('\n'),
                Event::HardBreak | Event::Rule => out.push('\n'),

                // TASK LIST MARKERS
                Event::TaskListMarker(checked) => {
                    out.push_str(if checked { "[x] " } else { "[ ] " });
                }

                // BLOCK STARTS → ensure newline separation
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

                // BLOCK ENDS → ensure newline separation
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

                // TABLE CELL HANDLING
                Event::End(TagEnd::TableCell) => {
                    let next = iter.peek();

                    let next_is_row_end = matches!(next, Some(Event::End(TagEnd::TableRow)));
                    let next_is_newline = matches!(next, Some(Event::SoftBreak | Event::HardBreak));
                    let next_is_block_end = matches!(
                        next,
                        Some(Event::End(
                            TagEnd::Paragraph
                                | TagEnd::Table
                                | TagEnd::TableHead
                                | TagEnd::List(_)
                                | TagEnd::Item
                        ))
                    );

                    // Canonical rule:
                    // A space is added only when it will remain stable across re-parsing:
                    //
                    //  NOT end of row
                    //  NOT before newline
                    //  NOT before block end
                    //
                    if !next_is_row_end
                        && !next_is_newline
                        && !next_is_block_end
                        && !out.ends_with(|c: char| c.is_whitespace())
                    {
                        out.push(' ');
                    }
                }

                _ => {}
            }
        }

        // FINAL CANONICAL TRIM: remove trailing whitespace created by block logic
        while out.ends_with(char::is_whitespace) {
            out.pop();
        }

        while out.starts_with(char::is_whitespace) {
            out.remove(0);
        }

        Ok(Cow::Owned(out))
    }
}

#[inline(always)]
fn contains_markdown_bytes(text: &str) -> bool {
    let bytes = text.as_bytes();

    // Fast checks for unambiguous markers
    if memchr3(b'#', b'*', b'_', bytes).is_some() {
        return true; // Headings, bold, italic, HR
    }
    if memchr::memchr2(b'`', b'~', bytes).is_some() {
        return true; // Code, strikethrough
    }
    if memchr::memchr(b'>', bytes).is_some() {
        return true; // Blockquotes
    }
    if memchr::memchr(b'|', bytes).is_some() {
        return true; // Tables
    }

    // Check for links: [text](url) or ![alt](url)
    if memchr::memchr(b'[', bytes).is_some()
        && memchr::memchr(b']', bytes).is_some()
        && memchr::memchr(b'(', bytes).is_some()
    {
        return true;
    }

    // Check for unordered lists or horizontal rules with hyphens
    if has_unordered_list_marker(bytes) || has_horizontal_rule(bytes) {
        return true;
    }

    // Check for ordered lists
    if has_ordered_list_marker(bytes) {
        return true;
    }

    false
}

/// Detects horizontal rules: ---, ***, ___ (three or more at line start)
/// Must be followed by whitespace/newline/end (not text like "---hello")
#[inline(always)]
fn has_horizontal_rule(bytes: &[u8]) -> bool {
    // Check from start of line
    if bytes.len() >= 3
        && ((bytes[0] == b'-' && bytes[1] == b'-' && bytes[2] == b'-')
            || (bytes[0] == b'*' && bytes[1] == b'*' && bytes[2] == b'*')
            || (bytes[0] == b'_' && bytes[1] == b'_' && bytes[2] == b'_'))
    {
        // Must be followed by whitespace, newline, or end of string
        if bytes.len() == 3
            || bytes[3] == b'\n'
            || bytes[3] == b' '
            || bytes[3] == b'\t'
            || bytes[3] == b'\r'
        {
            return true;
        }
    }

    // Check after newlines
    for i in 1..bytes.len().saturating_sub(2) {
        if bytes[i - 1] == b'\n'
            && ((bytes[i] == b'-' && bytes[i + 1] == b'-' && bytes[i + 2] == b'-')
                || (bytes[i] == b'*' && bytes[i + 1] == b'*' && bytes[i + 2] == b'*')
                || (bytes[i] == b'_' && bytes[i + 1] == b'_' && bytes[i + 2] == b'_'))
        {
            let next_idx = i + 3;
            if next_idx >= bytes.len()
                || bytes[next_idx] == b'\n'
                || bytes[next_idx] == b' '
                || bytes[next_idx] == b'\t'
                || bytes[next_idx] == b'\r'
            {
                return true;
            }
        }
    }

    false
}

#[inline(always)]
fn has_unordered_list_marker(bytes: &[u8]) -> bool {
    // Must be at line start or after newline
    if (bytes.first() == Some(&b'-') || bytes.first() == Some(&b'+')) && bytes.get(1) == Some(&b' ')
    {
        return true;
    }
    for i in 1..bytes.len().saturating_sub(1) {
        if bytes[i - 1] == b'\n'
            && (bytes[i] == b'-' || bytes[i] == b'+')
            && bytes.get(i + 1) == Some(&b' ')
        {
            return true;
        }
    }
    false
}

#[inline(always)]
fn has_ordered_list_marker(bytes: &[u8]) -> bool {
    let mut i = 0;

    // Check from start of text
    if bytes.first().is_some_and(|b| b.is_ascii_digit()) {
        // Scan consecutive digits
        let mut j = 1;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        // Check for ". " after digits
        if bytes.get(j) == Some(&b'.') && bytes.get(j + 1) == Some(&b' ') {
            return true;
        }
    }

    // Check after newlines
    while i < bytes.len() {
        if bytes[i] == b'\n' && i + 3 < bytes.len() {
            let next = i + 1;
            if bytes[next].is_ascii_digit() {
                // Scan consecutive digits
                let mut j = next + 1;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
                // Check for ". " after digits
                if bytes.get(j) == Some(&b'.') && bytes.get(j + 1) == Some(&b' ') {
                    return true;
                }
            }
        }
        i += 1;
    }

    false
}

impl StaticFusableStage for StripMarkdown {
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

impl StageTestConfig for StripMarkdown {
    fn one_to_one_languages() -> &'static [Lang] {
        &[]
    }

    fn samples(_lang: Lang) -> &'static [&'static str] {
        &[
            "# Title\n\n**bold** _italic_ ~~strike~~",
            "- [x] Done\n- [ ] Todo",
            "| A | B |\n| - | - |\n| 1 | 2 |",
            "[link](url) ![img](x.png)",
            "> Quote\n\n`code` $E=mc^2$ $$\\frac{1}{2}$$",
            "```rust\nfn main() {}\n```",
            "---Horizontal rule",
            "1. Ordered\n2. List",
            "---\nHorizontal rule\n---", // Tests newline preservation around HR
            "10. Multi-digit\n11. List", // Multi-digit ordered lists
        ]
    }

    fn should_pass_through(_lang: Lang) -> &'static [&'static str] {
        &[
            "plain text",
            "hello world",
            "test123",
            "",
            "[x] Done",        // Task list marker (preserved)
            "[ ] Todo",        // Task list marker (preserved)
            "pre-processing",  // Hyphen in word
            "I ate 2. pizzas", // Period after number mid-sentence
            "array[0]",        // Bracket notation
        ]
    }

    fn should_transform(_lang: Lang) -> &'static [(&'static str, &'static str)] {
        &[
            ("**bold**", "bold"),
            ("_italic_", "italic"),
            ("~~strike~~", "strike"),
            ("# Header", "Header"),
            ("`code`", "code"),
            ("`*text*`", "*text*"),
            ("[link](url)", "link"),
            ("- [x] Task", "[x] Task"),
            ("| A | B |\n| --- | --- |", "A B"), // Valid table header
            ("$E=mc^2$", "$E=mc^2$"),
            ("```rust\ncode\n```", "code"),
            (
                "| Header A | Header B |\n|----------|----------|\n| Cell 1 | Cell 2 |\n| Cell 3 | Cell 4 |",
                "Header A Header B\nCell 1 Cell 2\nCell 3 Cell 4",
            ),
        ]
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::assert_stage_contract;

    #[test]
    fn universal_contract_compliance() {
        assert_stage_contract!(StripMarkdown);
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
        let expected = "Header A Header B\nCell 1 Cell 2\nCell 3 Cell 4";
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

        // Soft breaks within blockquotes are preserved as newlines
        assert_eq!(result, "Quote line 1\nQuote line 2");
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

    #[test]
    fn test_idempotency_debug() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = "- [x] Task list\n- [ ] Pending";
        let once = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        eprintln!("ONCE: {:?}", once);

        let twice = stage.apply(once.clone(), &ctx).unwrap();
        eprintln!("TWICE: {:?}", twice);

        assert_eq!(once, twice);
    }

    #[test]
    fn test_strikethrough() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        assert_eq!(
            stage
                .apply(Cow::Borrowed("~~deleted text~~"), &ctx)
                .unwrap(),
            "deleted text"
        );

        assert_eq!(
            stage
                .apply(Cow::Borrowed("Keep ~~delete~~ this"), &ctx)
                .unwrap(),
            "Keep delete this"
        );
    }

    #[test]
    fn test_code_blocks() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        // Fenced code block
        let input = "```rust\nfn main() {}\n```";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert!(result.contains("fn main()"));

        // Code block with no language
        let input = "```\ncode here\n```";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert!(result.contains("code here"));
    }

    #[test]
    fn test_horizontal_rules() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = "Before\n\n---\n\nAfter";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        // Rules become newlines
        assert!(result.contains("Before"));
        assert!(result.contains("After"));
    }

    #[test]
    fn test_ordered_lists() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = "1. First\n2. Second\n3. Third";

        assert!(stage.needs_apply(input, &ctx).unwrap());
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();

        // At document start without blank lines, pulldown-cmark may treat as plain text
        // This is fine - the important thing is idempotency
        assert_eq!(result, "First\nSecond\nThird");

        // Verify idempotency
        let twice = stage.apply(result.clone(), &ctx).unwrap();
        assert_eq!(result, twice);
    }

    #[test]
    fn test_mixed_inline_formatting() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        // Bold + italic
        assert_eq!(
            stage
                .apply(Cow::Borrowed("***bold and italic***"), &ctx)
                .unwrap(),
            "bold and italic"
        );

        // Bold with code inside
        assert_eq!(
            stage
                .apply(Cow::Borrowed("**bold with `code` inside**"), &ctx)
                .unwrap(),
            "bold with code inside"
        );

        // Nested formatting
        assert_eq!(
            stage
                .apply(Cow::Borrowed("**bold _and italic_**"), &ctx)
                .unwrap(),
            "bold and italic"
        );
    }

    #[test]
    fn test_math_display_modes() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        // Inline math
        assert_eq!(
            stage
                .apply(Cow::Borrowed("Formula $E=mc^2$ here"), &ctx)
                .unwrap(),
            "Formula $E=mc^2$ here"
        );

        // Display math
        assert_eq!(
            stage
                .apply(Cow::Borrowed("$$\\frac{1}{2}$$"), &ctx)
                .unwrap(),
            "$$\\frac{1}{2}$$"
        );

        // Multiple inline
        assert_eq!(
            stage.apply(Cow::Borrowed("$x$ and $y$"), &ctx).unwrap(),
            "$x$ and $y$"
        );
    }

    #[test]
    fn test_escaped_markdown() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = r"\*not bold\* and \# not heading";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        // Backslash escapes should be handled by pulldown-cmark
        assert!(result.contains("*not bold*") || result.contains("not bold"));
    }

    #[test]
    fn test_empty_elements() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        // Empty bold
        let result = stage.apply(Cow::Borrowed("****"), &ctx).unwrap();
        assert_eq!(result.trim(), "");

        // Empty link
        let result = stage.apply(Cow::Borrowed("[](url)"), &ctx).unwrap();
        assert_eq!(result.trim(), "");
    }

    #[test]
    fn test_reference_style_links() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = "Click [here][ref]\n\n[ref]: https://example.com";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert!(result.contains("Click here"));
        assert!(!result.contains("[ref]"));
    }

    #[test]
    fn test_autolinks() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = "Visit <https://example.com> today";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert!(result.contains("https://example.com"));
    }

    #[test]
    fn test_nested_lists() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = "- Level 1\n  - Level 2\n    - Level 3";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert!(result.contains("Level 1"));
        assert!(result.contains("Level 2"));
        assert!(result.contains("Level 3"));
    }

    #[test]
    fn test_heading_levels() {
        let stage = StripMarkdown;
        let ctx = Context::new(ENG);

        let input = "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6";
        let result = stage.apply(Cow::Borrowed(input), &ctx).unwrap();
        assert!(result.contains("H1"));
        assert!(result.contains("H2"));
        assert!(result.contains("H6"));
    }
}
