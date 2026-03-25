use anyhow::{Context, Result};
use fast_yaml_linter::{Formatter, JsonFormatter, LintConfig, Linter, Severity, TextFormatter};

use crate::cli::LintFormat;
use crate::config::CommonConfig;
use crate::error::ExitCode;
use crate::io::InputSource;

/// Lint command implementation
pub struct LintCommand {
    config: CommonConfig,
    max_line_length: usize,
    format: LintFormat,
    allow_duplicate_keys: bool,
}

impl LintCommand {
    pub const fn new(
        config: CommonConfig,
        max_line_length: usize,
        format: LintFormat,
        allow_duplicate_keys: bool,
    ) -> Self {
        Self {
            config,
            max_line_length,
            format,
            allow_duplicate_keys,
        }
    }

    /// Execute lint command
    ///
    /// # Errors
    ///
    /// Returns error if linting fails (e.g., invalid YAML syntax)
    pub fn execute(&self, input: &InputSource) -> Result<ExitCode> {
        let start_time = std::time::Instant::now();

        // Configure linter using formatter indent from config
        let lint_config = LintConfig::new()
            .with_max_line_length(Some(self.max_line_length))
            .with_indent_size(self.config.formatter.indent() as usize)
            .with_allow_duplicate_keys(self.allow_duplicate_keys);

        // Create linter with all default rules (registered via with_config → with_default_rules)
        let linter = Linter::with_config(lint_config);

        // Run linter
        let diagnostics = linter.lint(input.as_str()).context("Failed to lint YAML")?;

        // Filter diagnostics based on quiet mode (errors only)
        let filtered_diagnostics: Vec<_> = if self.config.output.is_quiet() {
            diagnostics
                .into_iter()
                .filter(|d| d.severity == Severity::Error)
                .collect()
        } else {
            diagnostics
        };

        // Format output
        let output = match self.format {
            LintFormat::Text => {
                let mut formatter = TextFormatter::new();
                formatter.use_color = self.config.output.use_color();
                formatter.format(&filtered_diagnostics, input.as_str())
            }
            LintFormat::Json => {
                let formatter = JsonFormatter::new(true);
                formatter.format(&filtered_diagnostics, input.as_str())
            }
        };

        // Print output
        print!("{output}");

        // Print verbose info
        if self.config.output.is_verbose() && !matches!(self.format, LintFormat::Json) {
            let elapsed = start_time.elapsed();
            if let Some(path) = input.file_path() {
                eprintln!("\nFile: {}", path.display());
            }
            eprintln!("Lint time: {:.2}ms", elapsed.as_secs_f64() * 1000.0);
        }

        // Determine exit code
        let has_errors = filtered_diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);

        if has_errors {
            Ok(ExitCode::LintErrors)
        } else {
            Ok(ExitCode::Success)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FormatterConfig, OutputConfig};
    use crate::io::input::InputOrigin;

    fn create_test_config(quiet: bool, verbose: bool, use_color: bool, indent: u8) -> CommonConfig {
        CommonConfig::new()
            .with_output(
                OutputConfig::new()
                    .with_quiet(quiet)
                    .with_verbose(verbose)
                    .with_color(use_color),
            )
            .with_formatter(FormatterConfig::new().with_indent(indent))
    }

    #[test]
    fn test_lint_valid_yaml() {
        let input = InputSource {
            content: "name: test\nvalue: 123".to_string(),
            origin: InputOrigin::Stdin,
        };

        let config = create_test_config(true, false, false, 2);
        let cmd = LintCommand::new(config, 120, LintFormat::Text, false);
        let result = cmd.execute(&input);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_lint_with_warnings() {
        let input = InputSource {
            content: "name: this is a very very very very very very very very very very very very very very very very very very long line that exceeds the maximum".to_string(),
            origin: InputOrigin::Stdin,
        };

        let config = create_test_config(true, false, false, 2);
        let cmd = LintCommand::new(config, 80, LintFormat::Text, false);
        let result = cmd.execute(&input);

        // Should succeed (warnings don't cause failure)
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_lint_invalid_yaml() {
        let input = InputSource {
            content: "invalid: [unclosed".to_string(),
            origin: InputOrigin::Stdin,
        };

        let config = create_test_config(true, false, false, 2);
        let cmd = LintCommand::new(config, 120, LintFormat::Text, false);
        let result = cmd.execute(&input);

        // Should fail with parse error
        assert!(result.is_err());
    }

    #[test]
    fn test_lint_quiet_mode() {
        let input = InputSource {
            content: "name: test".to_string(),
            origin: InputOrigin::Stdin,
        };

        let config = create_test_config(true, false, false, 2);
        let cmd = LintCommand::new(config, 120, LintFormat::Text, false);
        let result = cmd.execute(&input);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_lint_json_format() {
        let input = InputSource {
            content: "name: test\nvalue: 123".to_string(),
            origin: InputOrigin::Stdin,
        };

        let config = create_test_config(false, false, false, 2);
        let cmd = LintCommand::new(config, 120, LintFormat::Json, false);
        let result = cmd.execute(&input);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_lint_duplicate_keys_reported_by_default() {
        let input = InputSource {
            content: "key: value1\nkey: value2\nother: data".to_string(),
            origin: InputOrigin::Stdin,
        };

        let config = create_test_config(false, false, false, 2);
        let cmd = LintCommand::new(config, 120, LintFormat::Text, false);
        let result = cmd.execute(&input);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::LintErrors);
    }

    #[test]
    fn test_lint_duplicate_keys_allowed_when_flag_set() {
        let input = InputSource {
            content: "key: value1\nkey: value2\nother: data".to_string(),
            origin: InputOrigin::Stdin,
        };

        let config = create_test_config(false, false, false, 2);
        let cmd = LintCommand::new(config, 120, LintFormat::Text, true);
        let result = cmd.execute(&input);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }
}
