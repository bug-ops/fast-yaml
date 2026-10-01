//! Bounded, escaped rendering of untrusted text for error messages.

use fast_yaml_core::fs::is_terminal_unsafe;

/// Longest echoed rule or option name, in characters.
pub const KEY_LIMIT: usize = 64;

/// Longest echoed diagnostic message that may embed user input, in characters.
pub const MESSAGE_LIMIT: usize = 256;

/// Truncates `text` to `limit` characters and escapes control characters.
pub fn echo(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for (index, c) in text.chars().enumerate() {
        if index == limit {
            out.push('…');
            break;
        }
        if c.is_control() {
            out.extend(c.escape_debug());
        } else if is_terminal_unsafe(c) {
            out.extend(c.escape_unicode());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_control_characters() {
        assert_eq!(echo("a\u{1b}[2J\u{7}b", 64), "a\\u{1b}[2J\\u{7}b");
        assert_eq!(echo("line\nbreak", 64), "line\\nbreak");
    }

    #[test]
    fn escapes_bidi_overrides() {
        assert_eq!(echo("a\u{202e}b\u{2028}", 64), "a\\u{202e}b\\u{2028}");
    }

    #[test]
    fn truncates_with_ellipsis() {
        let long = "x".repeat(100_000);
        let shown = echo(&long, KEY_LIMIT);
        assert_eq!(shown.chars().count(), KEY_LIMIT + 1);
        assert!(shown.ends_with('…'));
        assert_eq!(echo("short", KEY_LIMIT), "short");
    }

    #[test]
    fn truncates_by_characters_not_bytes() {
        assert_eq!(echo("ююю", 2), "юю…");
    }
}
