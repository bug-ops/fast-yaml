use anyhow::{Context, Result};
use fast_yaml_core::{Emitter, EmitterConfig, Parser};

use crate::io::{InputSource, OutputWriter};

/// Format command implementation
pub struct FormatCommand {
    indent: u8,
    width: usize,
}

impl FormatCommand {
    pub const fn new(indent: u8, width: usize) -> Self {
        Self { indent, width }
    }

    /// Execute format command
    pub fn execute(&self, input: &InputSource, output: &OutputWriter) -> Result<()> {
        // Parse YAML
        let value = Parser::parse_str(input.as_str())
            .context("Failed to parse YAML")?
            .ok_or_else(|| anyhow::anyhow!("Empty YAML document"))?;

        // Create emitter config
        let config = EmitterConfig::new()
            .with_indent(self.indent as usize)
            .with_width(self.width);

        // Emit formatted YAML
        let formatted = Emitter::emit_str_with_config(&value, &config)
            .context("Failed to emit YAML")?;

        // Write output
        output.write(&formatted)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::input::InputOrigin;
    use tempfile::NamedTempFile;

    #[test]
    fn test_format_simple_yaml() {
        let input = InputSource {
            content: "name:    test\nvalue:   123".to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_file = NamedTempFile::new().unwrap();
        let output = OutputWriter::from_args(
            Some(temp_file.path().to_path_buf()),
            false,
            None,
        )
        .unwrap();

        let cmd = FormatCommand::new(2, 80);
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

        let cmd = FormatCommand::new(2, 80);
        assert!(cmd.execute(&input, &output).is_err());
    }

    #[test]
    fn test_format_with_custom_indent() {
        let input = InputSource {
            content: "parent:\n  child: value".to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_file = NamedTempFile::new().unwrap();
        let output = OutputWriter::from_args(
            Some(temp_file.path().to_path_buf()),
            false,
            None,
        )
        .unwrap();

        let cmd = FormatCommand::new(4, 80);
        assert!(cmd.execute(&input, &output).is_ok());

        let formatted = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(formatted.contains("parent:"));
    }
}
