use anyhow::{Context, Result};
use fast_yaml_core::{Emitter, EmitterConfig};

use crate::config::CommonConfig;
use crate::io::{InputSource, OutputWriter};

/// Format command implementation
pub struct FormatCommand {
    config: CommonConfig,
    strip_comments: bool,
}

impl FormatCommand {
    pub const fn new(config: CommonConfig, strip_comments: bool) -> Self {
        Self {
            config,
            strip_comments,
        }
    }

    /// Execute format command
    pub fn execute(&self, input: &InputSource, output: &OutputWriter) -> Result<()> {
        if !self.strip_comments && yaml_has_comments(input.as_str()) {
            anyhow::bail!(
                "warning: YAML comments will be stripped by the formatter. \
                 Use --strip-comments to suppress this error."
            );
        }

        let emitter_config = EmitterConfig::new()
            .with_indent(self.config.formatter.indent() as usize)
            .with_width(self.config.formatter.width());

        let formatted = Emitter::format_with_config(input.as_str(), &emitter_config)
            .context("Failed to format YAML")?;

        output.write(&formatted)?;

        Ok(())
    }
}

/// Returns true if the YAML input contains at least one comment.
///
/// Scans line by line and tracks single-quoted and double-quoted string regions
/// to avoid false positives from `#` inside string literals.
fn yaml_has_comments(input: &str) -> bool {
    for line in input.lines() {
        let mut in_single = false;
        let mut in_double = false;

        for (_, ch) in line.char_indices() {
            match ch {
                '\'' if !in_double => in_single = !in_single,
                '"' if !in_single => in_double = !in_double,
                '#' if !in_single && !in_double => return true,
                _ => {}
            }
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
    fn test_yaml_has_comments_no_comment() {
        assert!(!yaml_has_comments("key: value\nother: 123"));
    }
}
