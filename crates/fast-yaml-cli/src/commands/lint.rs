use anyhow::{Context, Result};
use fast_yaml_linter::{Formatter, JsonFormatter, LintConfig, Linter, Severity, TextFormatter};

use crate::cli::LintFormat;
use crate::error::ExitCode;
use crate::io::InputSource;

/// Lint command implementation
pub struct LintCommand {
    max_line_length: usize,
    indent_size: usize,
    format: LintFormat,
    use_color: bool,
    quiet: bool,
    verbose: bool,
}

impl LintCommand {
    pub const fn new(
        max_line_length: usize,
        indent_size: usize,
        format: LintFormat,
        use_color: bool,
        quiet: bool,
        verbose: bool,
    ) -> Self {
        Self {
            max_line_length,
            indent_size,
            format,
            use_color,
            quiet,
            verbose,
        }
    }

    /// Execute lint command
    ///
    /// # Errors
    ///
    /// Returns error if linting fails (e.g., invalid YAML syntax)
    pub fn execute(&self, input: &InputSource) -> Result<ExitCode> {
        let start_time = std::time::Instant::now();

        // Configure linter
        let config = LintConfig::new()
            .with_max_line_length(Some(self.max_line_length))
            .with_indent_size(self.indent_size);

        // Create linter with all rules
        let mut linter = Linter::with_config(config);

        // Add all default rules
        linter
            .add_rule(Box::new(fast_yaml_linter::rules::DuplicateKeysRule))
            .add_rule(Box::new(fast_yaml_linter::rules::LineLengthRule))
            .add_rule(Box::new(fast_yaml_linter::rules::TrailingWhitespaceRule))
            .add_rule(Box::new(fast_yaml_linter::rules::DocumentStartRule))
            .add_rule(Box::new(fast_yaml_linter::rules::DocumentEndRule))
            .add_rule(Box::new(fast_yaml_linter::rules::EmptyValuesRule))
            .add_rule(Box::new(fast_yaml_linter::rules::NewLineAtEndOfFileRule));

        // Run linter
        let diagnostics = linter.lint(input.as_str()).context("Failed to lint YAML")?;

        // Filter diagnostics based on quiet mode (errors only)
        let filtered_diagnostics: Vec<_> = if self.quiet {
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
                formatter.use_color = self.use_color;
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
        if self.verbose && !matches!(self.format, LintFormat::Json) {
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
    use crate::io::input::InputOrigin;

    #[test]
    fn test_lint_valid_yaml() {
        let input = InputSource {
            content: "name: test\nvalue: 123".to_string(),
            origin: InputOrigin::Stdin,
        };

        let cmd = LintCommand::new(120, 2, LintFormat::Text, false, true, false);
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

        let cmd = LintCommand::new(80, 2, LintFormat::Text, false, true, false);
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

        let cmd = LintCommand::new(120, 2, LintFormat::Text, false, true, false);
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

        let cmd = LintCommand::new(120, 2, LintFormat::Text, false, true, false);
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

        let cmd = LintCommand::new(120, 2, LintFormat::Json, false, false, false);
        let result = cmd.execute(&input);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }
}
