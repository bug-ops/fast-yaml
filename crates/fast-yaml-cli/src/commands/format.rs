use anyhow::{Context, Result};
use fast_yaml_core::{Emitter, EmitterConfig};

use crate::config::CommonConfig;
use crate::io::{InputSource, OutputWriter};

/// Format command implementation
pub struct FormatCommand {
    config: CommonConfig,
}

impl FormatCommand {
    pub const fn new(config: CommonConfig) -> Self {
        Self { config }
    }

    /// Execute format command
    pub fn execute(&self, input: &InputSource, output: &OutputWriter) -> Result<()> {
        // Create emitter config from formatter settings
        let emitter_config = EmitterConfig::new()
            .with_indent(self.config.formatter.indent() as usize)
            .with_width(self.config.formatter.width());

        // Use format_with_config which automatically selects streaming for large files
        let formatted = Emitter::format_with_config(input.as_str(), &emitter_config)
            .context("Failed to format YAML")?;

        // Write output
        output.write(&formatted)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FormatterConfig;
    use crate::io::input::InputOrigin;
    use tempfile::NamedTempFile;

    #[test]
    fn test_format_simple_yaml() {
        let input = InputSource {
            content: "name:    test\nvalue:   123".to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_file = NamedTempFile::new().unwrap();
        let output =
            OutputWriter::from_args(Some(temp_file.path().to_path_buf()), false, None).unwrap();

        let config = CommonConfig::new()
            .with_formatter(FormatterConfig::new().with_indent(2).with_width(80));
        let cmd = FormatCommand::new(config);
        assert!(cmd.execute(&input, &output).is_ok());

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

        let config = CommonConfig::new()
            .with_formatter(FormatterConfig::new().with_indent(2).with_width(80));
        let cmd = FormatCommand::new(config);
        assert!(cmd.execute(&input, &output).is_err());
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
        let cmd = FormatCommand::new(config);
        assert!(cmd.execute(&input, &output).is_ok());

        let formatted = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(formatted.contains("parent:"));
    }
}
