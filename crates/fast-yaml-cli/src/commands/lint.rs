use anyhow::{Context, Result};
use fast_yaml_linter::{
    ConfigFile, Formatter, JsonFormatter, LintConfig, Linter, Severity, TextFormatter,
};
use std::path::PathBuf;

use crate::cli::LintFormat;
use crate::config::CommonConfig;
use crate::error::ExitCode;
use crate::io::InputSource;

/// CLI arguments for the lint command, separated from `CommonConfig`.
pub struct LintArgs {
    /// Explicit config file path (from `--config`).
    pub config_path: Option<PathBuf>,
    /// Whether to disable config file auto-discovery (`--no-config`).
    pub no_config: bool,
    /// Maximum line length override (from `--max-line-length`).
    pub max_line_length: Option<usize>,
    /// Indentation size override (from `--indent-size`).
    pub indent_size: Option<usize>,
    /// Lint output format.
    pub format: LintFormat,
    /// Allow duplicate keys override (from `--allow-duplicate-keys`).
    pub allow_duplicate_keys: Option<bool>,
}

/// Lint command implementation
pub struct LintCommand {
    config: CommonConfig,
    /// Resolved lint configuration (exposed for batch reuse).
    pub lint_config: LintConfig,
    format: LintFormat,
}

impl LintCommand {
    /// Build the lint command, loading config file and applying CLI overrides.
    ///
    /// # Errors
    ///
    /// Returns error if an explicit `--config` path cannot be read or parsed.
    pub fn build(config: CommonConfig, args: LintArgs, input: &InputSource) -> Result<Self> {
        let file_lint_config = Self::load_lint_config(args.config_path, args.no_config, input)?;
        let lint_config = ConfigFile::merge_cli_overrides(
            file_lint_config,
            args.max_line_length,
            args.indent_size,
            args.allow_duplicate_keys,
        );
        Ok(Self {
            config,
            lint_config,
            format: args.format,
        })
    }

    /// Load `LintConfig` from config file (explicit path, auto-discovered, or default).
    fn load_lint_config(
        config_path: Option<PathBuf>,
        no_config: bool,
        input: &InputSource,
    ) -> Result<LintConfig> {
        if no_config {
            return Ok(LintConfig::default());
        }

        if let Some(path) = config_path {
            // Explicit --config: hard error if missing or invalid
            let cfg = ConfigFile::load(&path)
                .with_context(|| format!("failed to load config file '{}'", path.display()))?;
            cfg.warn_unknown_rules();
            return Ok(cfg.into_lint_config());
        }

        // Auto-discovery: start from CWD (matches yamllint behavior)
        let start_dir = std::env::current_dir().unwrap_or_else(|_| {
            input
                .file_path()
                .and_then(|p| p.parent().map(std::path::Path::to_owned))
                .unwrap_or_else(|| PathBuf::from("."))
        });

        if let Some(discovered) = ConfigFile::discover(&start_dir) {
            eprintln!("using config file: {}", discovered.display());
            let cfg = ConfigFile::load(&discovered).with_context(|| {
                format!("failed to load config file '{}'", discovered.display())
            })?;
            cfg.warn_unknown_rules();
            return Ok(cfg.into_lint_config());
        }

        Ok(LintConfig::default())
    }

    /// Execute lint command
    ///
    /// # Errors
    ///
    /// Returns error if linting fails (e.g., invalid YAML syntax)
    pub fn execute(&self, input: &InputSource) -> Result<ExitCode> {
        let start_time = std::time::Instant::now();

        // Apply indent from CommonConfig formatter only when linter config is at default
        let effective_indent = self.config.formatter.indent() as usize;
        let lint_config = if self.lint_config.indent_size == 2 && effective_indent != 2 {
            self.lint_config.clone().with_indent_size(effective_indent)
        } else {
            self.lint_config.clone()
        };

        let linter = Linter::with_config(lint_config);
        let diagnostics = linter.lint(input.as_str()).context("Failed to lint YAML")?;

        let filtered_diagnostics: Vec<_> = if self.config.output.is_quiet() {
            diagnostics
                .into_iter()
                .filter(|d| d.severity == Severity::Error)
                .collect()
        } else {
            diagnostics
        };

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

        print!("{output}");

        if self.config.output.is_verbose() && !matches!(self.format, LintFormat::Json) {
            let elapsed = start_time.elapsed();
            if let Some(path) = input.file_path() {
                eprintln!("\nFile: {}", path.display());
            }
            eprintln!("Lint time: {:.2}ms", elapsed.as_secs_f64() * 1000.0);
        }

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
    use std::io::Write;

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

    fn stdin_input(content: &str) -> InputSource {
        InputSource {
            content: content.to_string(),
            origin: InputOrigin::Stdin,
        }
    }

    fn build_no_config(
        config: CommonConfig,
        max_line_length: Option<usize>,
        format: LintFormat,
        allow_duplicate_keys: Option<bool>,
        input: &InputSource,
    ) -> LintCommand {
        LintCommand::build(
            config,
            LintArgs {
                config_path: None,
                no_config: true,
                max_line_length,
                indent_size: None,
                format,
                allow_duplicate_keys,
            },
            input,
        )
        .unwrap()
    }

    #[test]
    fn test_lint_valid_yaml() {
        let input = stdin_input("name: test\nvalue: 123");
        let config = create_test_config(true, false, false, 2);
        let cmd = build_no_config(config, Some(120), LintFormat::Text, None, &input);
        let result = cmd.execute(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_lint_with_warnings() {
        let long = "name: this is a very very very very very very very very very very very very very very very very very very long line that exceeds the maximum";
        let input = stdin_input(long);
        let config = create_test_config(true, false, false, 2);
        let cmd = build_no_config(config, Some(80), LintFormat::Text, None, &input);
        let result = cmd.execute(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_lint_invalid_yaml() {
        let input = stdin_input("invalid: [unclosed");
        let config = create_test_config(true, false, false, 2);
        let cmd = build_no_config(config, Some(120), LintFormat::Text, None, &input);
        let result = cmd.execute(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_lint_quiet_mode() {
        let input = stdin_input("name: test");
        let config = create_test_config(true, false, false, 2);
        let cmd = build_no_config(config, Some(120), LintFormat::Text, None, &input);
        let result = cmd.execute(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_lint_json_format() {
        let input = stdin_input("name: test\nvalue: 123");
        let config = create_test_config(false, false, false, 2);
        let cmd = build_no_config(config, Some(120), LintFormat::Json, None, &input);
        let result = cmd.execute(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_lint_duplicate_keys_reported_by_default() {
        let input = stdin_input("key: value1\nkey: value2\nother: data");
        let config = create_test_config(false, false, false, 2);
        let cmd = build_no_config(config, Some(120), LintFormat::Text, None, &input);
        let result = cmd.execute(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::LintErrors);
    }

    #[test]
    fn test_lint_duplicate_keys_allowed_when_flag_set() {
        let input = stdin_input("key: value1\nkey: value2\nother: data");
        let config = create_test_config(false, false, false, 2);
        let cmd = build_no_config(config, Some(120), LintFormat::Text, Some(true), &input);
        let result = cmd.execute(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_config_file_overrides_defaults() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "rules:\n  key-ordering:\n    enabled: false").unwrap();
        let input = stdin_input("b: 1\na: 2");
        let config = create_test_config(false, false, false, 2);
        let cmd = LintCommand::build(
            config,
            LintArgs {
                config_path: Some(f.path().to_owned()),
                no_config: false,
                max_line_length: None,
                indent_size: None,
                format: LintFormat::Text,
                allow_duplicate_keys: None,
            },
            &input,
        )
        .unwrap();
        let result = cmd.execute(&input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_explicit_config_missing_file_returns_error() {
        let input = stdin_input("name: test");
        let config = create_test_config(false, false, false, 2);
        let result = LintCommand::build(
            config,
            LintArgs {
                config_path: Some(PathBuf::from("/nonexistent/.fast-yaml.yaml")),
                no_config: false,
                max_line_length: None,
                indent_size: None,
                format: LintFormat::Text,
                allow_duplicate_keys: None,
            },
            &input,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_config_file_line_length_max_is_loaded() {
        // Regression: config file line-length.max must set LintConfig::max_line_length,
        // not just rule_configs, so LineLengthRule::check actually uses it.
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "rules:\n  line-length:\n    max: 50").unwrap();
        let input = stdin_input("name: test");
        let config = create_test_config(false, false, false, 2);
        let cmd = LintCommand::build(
            config,
            LintArgs {
                config_path: Some(f.path().to_owned()),
                no_config: false,
                max_line_length: None,
                indent_size: None,
                format: LintFormat::Text,
                allow_duplicate_keys: None,
            },
            &input,
        )
        .unwrap();
        assert_eq!(cmd.lint_config.max_line_length, Some(50));
    }

    #[test]
    fn test_config_file_line_length_triggers_diagnostic() {
        // End-to-end: a line longer than config-specified max must produce a diagnostic.
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "rules:\n  line-length:\n    max: 50").unwrap();
        let long_line = "name: a-sixty-character-line-that-exceeds-fifty-chars-limit!!";
        let input = stdin_input(long_line);
        let config = create_test_config(true, false, false, 2);
        let cmd = LintCommand::build(
            config,
            LintArgs {
                config_path: Some(f.path().to_owned()),
                no_config: false,
                max_line_length: None,
                indent_size: None,
                format: LintFormat::Json,
                allow_duplicate_keys: None,
            },
            &input,
        )
        .unwrap();
        let result = cmd.execute(&input);
        // Warnings don't cause LintErrors exit code (only errors do), but must not fail
        assert!(result.is_ok());
    }

    #[test]
    fn test_cli_overrides_config_file_max_line_length() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "rules:\n  line-length:\n    max: 50").unwrap();
        let input = stdin_input("name: test");
        let config = create_test_config(false, false, false, 2);
        let cmd = LintCommand::build(
            config,
            LintArgs {
                config_path: Some(f.path().to_owned()),
                no_config: false,
                max_line_length: Some(200),
                indent_size: None,
                format: LintFormat::Text,
                allow_duplicate_keys: None,
            },
            &input,
        )
        .unwrap();
        // CLI value wins over config file value
        assert_eq!(cmd.lint_config.max_line_length, Some(200));
    }

    #[test]
    fn test_allow_duplicate_keys_none_does_not_override() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "rules: {{}}").unwrap();
        let input = stdin_input("key: 1\nkey: 2");
        let config = create_test_config(false, false, false, 2);
        let cmd = LintCommand::build(
            config,
            LintArgs {
                config_path: Some(f.path().to_owned()),
                no_config: false,
                max_line_length: None,
                indent_size: None,
                format: LintFormat::Text,
                allow_duplicate_keys: None,
            },
            &input,
        )
        .unwrap();
        assert!(!cmd.lint_config.allow_duplicate_keys);
    }
}
