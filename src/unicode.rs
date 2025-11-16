//! unicode.rs – Universal Unicode utilities
//!
//! This module contains Unicode constants and helpers that are **language-agnostic**.
//! For language-specific rules (Turkish İ, German ß, etc.), see `lang.rs`.
//!
//! # Categories
//! - Format control characters (invisible formatting)
//! - (Future: Whitespace categories, word break properties, etc.)

/// ---------------------------------------------------------------------------
/// Format Control Characters (Unicode Category Cf)
/// ---------------------------------------------------------------------------
///
/// Format control characters are invisible Unicode characters that affect
/// text rendering but not semantic content. They are typically removed in
/// normalization pipelines.
///
/// # Categories Included
///
/// ## Zero-Width Characters
/// - U+200B (ZWSP): Zero-width space
/// - U+200C (ZWNJ): Zero-width non-joiner (used in Persian, Hindi)
/// - U+200D (ZWJ): Zero-width joiner (used in emoji, Arabic)
/// - U+2060: Word joiner
/// - U+FEFF: Zero-width no-break space (BOM)
///
/// ## Bidirectional Formatting
/// - U+200E (LRM): Left-to-right mark
/// - U+200F (RLM): Right-to-left mark
/// - U+202A-E: Directional embeddings and overrides
///
/// ## Mathematical Formatting
/// - U+2061-64: Invisible function application, operators
///
/// ## Legacy
/// - U+206A-F: Deprecated formatting controls
///
/// # Language Independence
///
/// These characters are **NOT language-specific**. They are universal
/// Unicode features and should be handled the same way for all languages.
pub static FORMAT_CONTROLS: &[char] = &[
    '\u{200B}', // Zero-width space
    '\u{200C}', // Zero-width non-joiner
    '\u{200D}', // Zero-width joiner
    '\u{200E}', // Left-to-right mark
    '\u{200F}', // Right-to-left mark
    '\u{202A}', // Left-to-right embedding
    '\u{202B}', // Right-to-left embedding
    '\u{202C}', // Pop directional formatting
    '\u{202D}', // Left-to-right override
    '\u{202E}', // Right-to-left override
    '\u{2060}', // Word joiner
    '\u{2061}', // Function application
    '\u{2062}', // Invisible times
    '\u{2063}', // Invisible separator
    '\u{2064}', // Invisible plus
    '\u{206A}', // Inhibit symmetric swapping
    '\u{206B}', // Activate symmetric swapping
    '\u{206C}', // Inhibit Arabic form shaping
    '\u{206D}', // Activate Arabic form shaping
    '\u{206E}', // National digit shapes
    '\u{206F}', // Nominal digit shapes
    '\u{FEFF}', // Zero-width no-break space (BOM)
];

/// Check if a character is a format control character.
///
/// # Examples
/// ```rust
/// use normy::unicode::is_format_control;
///
/// assert!(is_format_control('\u{200B}')); // Zero-width space
/// assert!(is_format_control('\u{FEFF}')); // BOM
/// assert!(!is_format_control('a'));
/// assert!(!is_format_control(' ')); // Regular space is not a format control
/// ```
#[inline(always)]
pub fn is_format_control(c: char) -> bool {
    FORMAT_CONTROLS.contains(&c)
}

/// Check if text contains any format control characters.
///
/// This is more efficient than iterating and checking each character individually
/// for the common case where no format controls are present.
///
/// # Examples
/// ```rust
/// use normy::unicode::contains_format_controls;
///
/// assert!(contains_format_controls("hello\u{200B}world"));
/// assert!(!contains_format_controls("hello world"));
/// ```
#[inline]
pub fn contains_format_controls(text: &str) -> bool {
    text.chars().any(is_format_control)
}

/// Count the number of format control characters in text.
///
/// # Examples
/// ```rust
/// use normy::unicode::count_format_controls;
///
/// assert_eq!(count_format_controls("hello\u{200B}world\u{200C}"), 2);
/// assert_eq!(count_format_controls("hello"), 0);
/// ```
#[inline]
pub fn count_format_controls(text: &str) -> usize {
    text.chars().filter(|&c| is_format_control(c)).count()
}

/// Remove all format control characters from text.
///
/// This is a convenience function that does the same thing as the
/// `RemoveFormatControls` stage, but as a simple function call.
///
/// # Examples
/// ```rust
/// use normy::unicode::strip_format_controls;
///
/// let cleaned = strip_format_controls("hello\u{200B}world");
/// assert_eq!(cleaned, "helloworld");
/// ```
pub fn strip_format_controls(text: &str) -> String {
    text.chars().filter(|&c| !is_format_control(c)).collect()
}

// ---------------------------------------------------------------------------
// Future: Could add more Unicode utilities here
// ---------------------------------------------------------------------------

// /// Unicode whitespace characters (future)
// pub static UNICODE_WHITESPACE: &[char] = &[...];

// /// Check if character is Unicode whitespace (future)
// pub fn is_unicode_whitespace(c: char) -> bool { ... }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_format_control() {
        // Zero-width
        assert!(is_format_control('\u{200B}'));
        assert!(is_format_control('\u{200C}'));
        assert!(is_format_control('\u{200D}'));
        assert!(is_format_control('\u{2060}'));
        assert!(is_format_control('\u{FEFF}'));

        // BiDi
        assert!(is_format_control('\u{200E}'));
        assert!(is_format_control('\u{200F}'));
        assert!(is_format_control('\u{202A}'));

        // Math
        assert!(is_format_control('\u{2061}'));

        // Not format controls
        assert!(!is_format_control('a'));
        assert!(!is_format_control(' '));
        assert!(!is_format_control('\n'));
        assert!(!is_format_control('\t'));
    }

    #[test]
    fn test_contains_format_controls() {
        assert!(contains_format_controls("hello\u{200B}world"));
        assert!(contains_format_controls("\u{FEFF}text"));
        assert!(contains_format_controls("text\u{200E}"));
        assert!(!contains_format_controls("hello world"));
        assert!(!contains_format_controls(""));
    }

    #[test]
    fn test_count_format_controls() {
        assert_eq!(count_format_controls(""), 0);
        assert_eq!(count_format_controls("hello"), 0);
        assert_eq!(count_format_controls("hello\u{200B}world"), 1);
        assert_eq!(count_format_controls("\u{200B}\u{200C}\u{200D}"), 3);
        assert_eq!(count_format_controls("a\u{200B}b\u{200C}c\u{200D}d"), 3);
    }

    #[test]
    fn test_strip_format_controls() {
        assert_eq!(strip_format_controls("hello"), "hello");
        assert_eq!(strip_format_controls("hello\u{200B}world"), "helloworld");
        assert_eq!(strip_format_controls("\u{FEFF}text"), "text");
        assert_eq!(strip_format_controls("\u{200B}\u{200C}\u{200D}abc"), "abc");
    }

    #[test]
    fn test_all_format_controls_detected() {
        // Verify every character in FORMAT_CONTROLS is detected
        for &c in FORMAT_CONTROLS {
            assert!(
                is_format_control(c),
                "Character U+{:04X} should be detected as format control",
                c as u32
            );
        }
    }
}
