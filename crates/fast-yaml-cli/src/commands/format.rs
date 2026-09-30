use anyhow::{Context, Result};
use fast_yaml_core::{Emitter, EmitterConfig, strip_bom};

use crate::config::CommonConfig;
use crate::io::{InputSource, OutputWriter};

/// Error message shown when the formatter would silently drop YAML comments.
pub const COMMENTS_STRIPPED_MSG: &str = "file contains YAML comments that the formatter would strip; use --strip-comments to allow this";

/// Format command implementation
pub struct FormatCommand {
    config: CommonConfig,
    strip_comments: bool,
    dry_run: bool,
}

impl FormatCommand {
    pub const fn new(config: CommonConfig, strip_comments: bool) -> Self {
        Self {
            config,
            strip_comments,
            dry_run: false,
        }
    }

    /// Validates and formats the input but never writes the result.
    #[must_use]
    pub const fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Execute format command
    pub fn execute(&self, input: &InputSource, output: &OutputWriter) -> Result<()> {
        if !self.strip_comments && yaml_has_comments(input.as_str()) {
            anyhow::bail!(COMMENTS_STRIPPED_MSG);
        }

        let emitter_config = EmitterConfig::new()
            .with_indent(self.config.formatter.indent() as usize)
            .with_width(self.config.formatter.width());

        let formatted = Emitter::format_with_config(input.as_str(), &emitter_config)
            .context("Failed to format YAML")?;

        if !self.dry_run {
            output.write(&formatted)?;
        }

        Ok(())
    }
}

/// Scanner state carried across lines.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scan {
    Plain,
    Double,
    Single,
    /// Inside a block scalar whose parent node sits at the given column.
    Block(isize),
}

/// Returns true if the text after `|`/`>` is only chomping/indent indicators.
fn is_block_header(rest: &[char]) -> bool {
    let tail = rest
        .iter()
        .skip_while(|c| matches!(c, '+' | '-' | '0'..='9'));
    tail.clone().next().is_none_or(|c| c.is_whitespace())
}

/// Returns true if the YAML input contains at least one comment.
///
/// A `#` is a comment only at line start or after whitespace, outside quoted scalars
/// (which may span lines) and block scalar bodies. Quotes open a scalar only at a value start.
pub fn yaml_has_comments(input: &str) -> bool {
    let mut state = Scan::Plain;
    let mut flow_depth = 0usize;

    for line in strip_bom(input).lines() {
        let chars: Vec<char> = line.chars().collect();
        let indent = chars.iter().take_while(|c| **c == ' ').count();

        if let Scan::Block(parent) = state {
            if chars.iter().all(|c| c.is_whitespace()) || indent.cast_signed() > parent {
                continue;
            }
            state = Scan::Plain;
        }

        let mut parent = indent.cast_signed();
        let mut value_start = true;
        let mut after_dash = false;
        let mut block_header = false;
        let mut i = indent;

        let is_marker = |m: &str| {
            indent == 0 && line.starts_with(m) && chars.get(3).is_none_or(|c| c.is_whitespace())
        };
        if state == Scan::Plain && (is_marker("---") || is_marker("...")) {
            parent = -1;
            i = 3;
        }

        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied();

            match state {
                Scan::Double => {
                    match c {
                        '\\' => i += 1,
                        '"' => state = Scan::Plain,
                        _ => {}
                    }
                    i += 1;
                    continue;
                }
                Scan::Single => {
                    if c == '\'' {
                        if next == Some('\'') {
                            i += 1;
                        } else {
                            state = Scan::Plain;
                        }
                    }
                    i += 1;
                    continue;
                }
                Scan::Plain | Scan::Block(_) => {}
            }

            if c == ' ' || c == '\t' {
                i += 1;
                continue;
            }
            if c == '#' {
                if i == 0 || matches!(chars[i - 1], ' ' | '\t') {
                    return true;
                }
                value_start = false;
                i += 1;
                continue;
            }

            if value_start && after_dash && !matches!(c, '|' | '>' | '-' | '?' | '&' | '!' | '*') {
                parent = i.cast_signed();
                after_dash = false;
            }
            let indicator_end = next.is_none_or(|n| n == ' ' || n == '\t');

            match c {
                '"' | '\'' if value_start => {
                    state = if c == '"' { Scan::Double } else { Scan::Single };
                    value_start = false;
                }
                '-' | '?' if value_start && indicator_end => {
                    parent = i.cast_signed();
                    after_dash = true;
                }
                ':' if indicator_end || flow_depth > 0 => value_start = true,
                '|' | '>' if value_start && is_block_header(&chars[i + 1..]) => {
                    block_header = true;
                    value_start = false;
                }
                '&' | '!' | '*' if value_start => {
                    while i < chars.len() && !matches!(chars[i], ' ' | '\t') {
                        i += 1;
                    }
                    continue;
                }
                '[' | '{' if value_start => flow_depth += 1,
                ',' if flow_depth > 0 => value_start = true,
                ']' | '}' if flow_depth > 0 => {
                    flow_depth -= 1;
                    value_start = false;
                }
                _ => value_start = false,
            }
            i += 1;
        }

        if block_header && state == Scan::Plain {
            state = Scan::Block(parent);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FormatterConfig;
    use crate::io::input::InputOrigin;
    use tempfile::NamedTempFile;

    fn make_cmd(strip_comments: bool) -> FormatCommand {
        let config = CommonConfig::new()
            .with_formatter(FormatterConfig::new().with_indent(2).with_width(80));
        FormatCommand::new(config, strip_comments)
    }

    #[test]
    fn test_format_simple_yaml() {
        let input = InputSource {
            content: "name:    test\nvalue:   123".to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_file = NamedTempFile::new().unwrap();
        let output =
            OutputWriter::from_args(Some(temp_file.path().to_path_buf()), false, None).unwrap();

        assert!(make_cmd(false).execute(&input, &output).is_ok());

        let formatted = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(formatted.contains("name:"));
        assert!(formatted.contains("value:"));
    }

    #[test]
    fn test_format_invalid_yaml() {
        let input = InputSource {
            content: "invalid: [".to_string(),
            origin: InputOrigin::Stdin,
        };
        let output = OutputWriter::stdout();
        assert!(make_cmd(false).execute(&input, &output).is_err());
    }

    #[test]
    fn test_format_with_custom_indent() {
        let input = InputSource {
            content: "parent:\n  child: value".to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_file = NamedTempFile::new().unwrap();
        let output =
            OutputWriter::from_args(Some(temp_file.path().to_path_buf()), false, None).unwrap();

        let config = CommonConfig::new()
            .with_formatter(FormatterConfig::new().with_indent(4).with_width(80));
        assert!(
            FormatCommand::new(config, false)
                .execute(&input, &output)
                .is_ok()
        );

        let formatted = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(formatted.contains("parent:"));
    }

    #[test]
    fn test_format_with_comments_no_flag_errors() {
        let input = InputSource {
            content: "# top-level comment\nname: test".to_string(),
            origin: InputOrigin::Stdin,
        };
        let output = OutputWriter::stdout();
        let err = make_cmd(false).execute(&input, &output).unwrap_err();
        assert!(err.to_string().contains("--strip-comments"));
    }

    #[test]
    fn test_format_with_comments_strip_flag_succeeds() {
        let input = InputSource {
            content: "# top-level comment\nname: test".to_string(),
            origin: InputOrigin::Stdin,
        };
        let temp_file = NamedTempFile::new().unwrap();
        let output =
            OutputWriter::from_args(Some(temp_file.path().to_path_buf()), false, None).unwrap();
        assert!(make_cmd(true).execute(&input, &output).is_ok());
    }

    #[test]
    fn test_yaml_has_comments_detects_inline() {
        assert!(yaml_has_comments("key: value # inline"));
    }

    #[test]
    fn test_yaml_has_comments_ignores_hash_in_string() {
        assert!(!yaml_has_comments("key: \"value # not a comment\""));
        assert!(!yaml_has_comments("key: 'value # not a comment'"));
    }

    #[test]
    fn test_yaml_has_comments_apostrophe_in_plain_scalar() {
        assert!(!yaml_has_comments("msg: don't\n"));
        assert!(yaml_has_comments("msg: don't # note\n"));
    }

    #[test]
    fn test_yaml_has_comments_escaped_quote() {
        assert!(yaml_has_comments("x: \"a\\\"b\" # c\n"));
        assert!(!yaml_has_comments("x: \"a\\\" # b\"\n"));
        assert!(yaml_has_comments("x: 'it''s' # c\n"));
        assert!(!yaml_has_comments("x: 'it''s # b'\n"));
    }

    #[test]
    fn test_yaml_has_comments_hash_without_space() {
        assert!(!yaml_has_comments("url: http://x/#frag\n"));
        assert!(yaml_has_comments("url: http://x/#frag # c\n"));
    }

    #[test]
    fn test_yaml_has_comments_block_scalar() {
        assert!(!yaml_has_comments(
            "s: |\n  # not a comment\n  text\nk: v\n"
        ));
        assert!(yaml_has_comments("s: |\n  # not a comment\nk: v # real\n"));
        assert!(yaml_has_comments("s: | # header comment\n  text\n"));
        assert!(!yaml_has_comments("- |\n  # x\n- a\n"));
        assert!(yaml_has_comments("- key: >-\n    # x\n  other: 1 # c\n"));
        assert!(yaml_has_comments("s: |\n  text\n# after\n"));
    }

    #[test]
    fn test_yaml_has_comments_flow_and_multiline_quotes() {
        assert!(!yaml_has_comments("a: [\"x # y\", 'z # w']\n"));
        assert!(yaml_has_comments("a: [\"x\", 'z'] # c\n"));
        assert!(!yaml_has_comments("a: \"line one\n  # inside\n  end\"\n"));
    }

    #[test]
    fn test_yaml_has_comments_bom_prefix() {
        assert!(yaml_has_comments("\u{FEFF}# header\nkey: v\n"));
        assert!(!yaml_has_comments("\u{FEFF}key: v\n"));
    }

    #[test]
    fn test_yaml_has_comments_compact_flow() {
        assert!(!yaml_has_comments("{\"a\":\"x # y\"}\n"));
        assert!(yaml_has_comments("{\"a\":\"x # y\"} # c\n"));
    }

    #[test]
    fn test_yaml_has_comments_tab_and_crlf() {
        assert!(yaml_has_comments("key: v\t# c\n"));
        assert!(yaml_has_comments("key: v\r\n# c\r\n"));
        assert!(!yaml_has_comments("key: v\r\nb: 1\r\n"));
    }

    #[test]
    fn test_yaml_has_comments_non_header_pipe() {
        assert!(yaml_has_comments("a: x\n  |not a header\nb: 1 # c\n"));
    }

    #[test]
    fn test_yaml_has_comments_no_comment() {
        assert!(!yaml_has_comments("key: value\nother: 123"));
    }
}
