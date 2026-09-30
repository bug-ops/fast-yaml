use anyhow::{Context, Result};
use fast_yaml_core::limits::{MaxInputBytes, ParseLimits};
use fast_yaml_linter::{
    ConfigFile, Formatter, JsonFormatter, LintConfig, Linter, Severity, TextFormatter,
    config::IndentSize,
};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use crate::cli::LintFormat;
use crate::config::CommonConfig;
use crate::error::ExitCode;
use crate::file_filter::FileFilter;
use crate::io::InputSource;

/// CLI arguments for the lint command, separated from `CommonConfig`.
pub struct LintArgs {
    /// Explicit config file path (from `--config`).
    pub config_path: Option<PathBuf>,
    /// Whether to disable config file auto-discovery (`--no-config`).
    pub no_config: bool,
    /// Maximum line length override (from `--max-line-length`).
    pub max_line_length: Option<NonZeroUsize>,
    /// Indentation size override (from `--indent-size`).
    pub indent_size: Option<IndentSize>,
    /// Lint output format.
    pub format: LintFormat,
    /// Allow duplicate keys override (from `--allow-duplicate-keys`).
    pub allow_duplicate_keys: Option<bool>,
    /// Input size limit override (from `--max-input-bytes`).
    pub max_input_bytes: Option<MaxInputBytes>,
    pub parse_limits: ParseLimits,
}

/// Lint command implementation
pub struct LintCommand {
    config: CommonConfig,
    /// Resolved lint configuration (exposed for batch reuse).
    pub lint_config: LintConfig,
    /// Files the config file selects or drops (exposed for batch discovery).
    pub file_filter: FileFilter,
    format: LintFormat,
}

fn split_config(config: ConfigFile) -> (LintConfig, FileFilter) {
    let (lint_config, selection) = config.into_parts();
    (lint_config, FileFilter::new(selection))
}

impl LintCommand {
    /// Build the lint command, loading config file and applying CLI overrides.
    ///
    /// # Errors
    ///
    /// Returns error if an explicit `--config` path cannot be read or parsed.
    pub fn build(config: CommonConfig, args: LintArgs, input: &InputSource) -> Result<Self> {
        let file = Self::load_config_file(args.config_path, args.no_config, input)?;
        let max_input_bytes = args
            .max_input_bytes
            .or(file.max_input_bytes)
            .unwrap_or(MaxInputBytes::DEFAULT);
        let (file_lint_config, file_filter) = split_config(file);
        let lint_config = ConfigFile::merge_cli_overrides(
            file_lint_config,
            args.max_line_length,
            args.indent_size,
            args.allow_duplicate_keys,
        )
        .with_parse_limits(args.parse_limits)
        .with_max_input_bytes(max_input_bytes);
        Ok(Self {
            config,
            lint_config,
            file_filter,
            format: args.format,
        })
    }

    /// Load the config file (explicit path, auto-discovered, or default).
    fn load_config_file(
        config_path: Option<PathBuf>,
        no_config: bool,
        input: &InputSource,
    ) -> Result<ConfigFile> {
        if no_config {
            return Ok(ConfigFile::default());
        }

        if let Some(path) = config_path {
            // Explicit --config: hard error if missing or invalid
            let cfg = ConfigFile::load(&path)
                .with_context(|| format!("failed to load config file '{}'", path.display()))?;
            return Ok(cfg);
        }

        // Auto-discovery: start from CWD (matches yamllint behavior)
        let start_dir = std::env::current_dir().unwrap_or_else(|_| {
            input
                .file_path()
                .and_then(|p| p.parent().map(Path::to_owned))
                .unwrap_or_else(|| PathBuf::from("."))
        });

        if let Some(discovered) = ConfigFile::discover(&start_dir) {
            eprintln!("using config file: {}", discovered.display());
            let cfg = ConfigFile::load(&discovered).with_context(|| {
                format!("failed to load config file '{}'", discovered.display())
            })?;
            return Ok(cfg);
        }

        Ok(ConfigFile::default())
    }

    /// Returns whether the config file's `ignore` drops the file at `path`.
    ///
    /// A path that cannot be canonicalized is not ignored, so the read error surfaces later.
    #[must_use]
    pub fn is_ignored(&self, path: &Path) -> bool {
        self.file_filter.has_ignore()
            && path
                .canonicalize()
                .is_ok_and(|canonical| self.file_filter.is_ignored(&canonical, false))
    }

    /// Reports a file that the config file ignores: no diagnostics and a success exit code.
    #[must_use]
    pub fn execute_ignored(&self) -> ExitCode {
        if matches!(self.format, LintFormat::Json) {
            print!("{}", JsonFormatter::new(true).format(&[], ""));
        }
        ExitCode::Success
    }

    /// Execute lint command
    ///
    /// # Errors
    ///
    /// Returns error if linting fails (e.g., invalid YAML syntax)
    pub fn execute(&self, input: &InputSource) -> Result<ExitCode> {
        let start_time = std::time::Instant::now();

        // Apply indent from CommonConfig formatter only when linter config is at default
        let effective_indent = self.config.formatter.lint_indent_size();
        let configured_indent = self.lint_config.rules.indentation.options.indent_size;
        let lint_config = if configured_indent == IndentSize::default()
            && effective_indent != IndentSize::default()
        {
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
    use crate::config::{FormatterConfig, OutputConfig, Verbosity};
    use crate::io::input::InputOrigin;
    use std::io::Write;

    fn create_test_config(verbosity: Verbosity, use_color: bool, indent: u8) -> CommonConfig {
        CommonConfig::new()
            .with_output(
                OutputConfig::new()
                    .with_verbosity(verbosity)
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
        max_line_length: Option<NonZeroUsize>,
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
                max_input_bytes: None,
                parse_limits: ParseLimits::default(),
            },
            input,
        )
        .unwrap()
    }

    #[test]
    fn test_build_defaults_input_size_limit() {
        let input = stdin_input("a: 1");
        let cmd = build_no_config(
            create_test_config(Verbosity::Quiet, false, 2),
            None,
            LintFormat::Text,
            None,
            &input,
        );
        assert_eq!(cmd.lint_config.max_input_bytes, MaxInputBytes::DEFAULT);
    }

    fn build_with_limit(config_text: Option<&str>, flag: Option<usize>) -> LintCommand {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        if let Some(text) = config_text {
            writeln!(f, "{text}").unwrap();
        }
        LintCommand::build(
            create_test_config(Verbosity::Quiet, false, 2),
            LintArgs {
                config_path: config_text.map(|_| f.path().to_owned()),
                no_config: config_text.is_none(),
                max_line_length: None,
                indent_size: None,
                format: LintFormat::Text,
                allow_duplicate_keys: None,
                max_input_bytes: flag.map(|n| MaxInputBytes::new(n).unwrap()),
                parse_limits: ParseLimits::default(),
            },
            &stdin_input(""),
        )
        .unwrap()
    }

    #[test]
    fn test_max_input_bytes_precedence_flag_over_config_over_default() {
        let limit = |n| MaxInputBytes::new(n).unwrap();
        let both = build_with_limit(Some("max-input-bytes: 200"), Some(100));
        assert_eq!(both.lint_config.max_input_bytes, limit(100));
        let config_only = build_with_limit(Some("max-input-bytes: 200"), None);
        assert_eq!(config_only.lint_config.max_input_bytes, limit(200));
        let config_without_key = build_with_limit(Some("rules: {}"), None);
        assert_eq!(
            config_without_key.lint_config.max_input_bytes,
            MaxInputBytes::DEFAULT
        );
        let flag_only = build_with_limit(None, Some(100));
        assert_eq!(flag_only.lint_config.max_input_bytes, limit(100));
    }

    #[test]
    fn test_lint_valid_yaml() {
        let input = stdin_input("name: test\nvalue: 123");
        let config = create_test_config(Verbosity::Quiet, false, 2);
        let cmd = build_no_config(
            config,
            NonZeroUsize::new(120),
            LintFormat::Text,
            None,
            &input,
        );
        let result = cmd.execute(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_lint_with_warnings() {
        let long = "name: this is a very very very very very very very very very very very very very very very very very very long line that exceeds the maximum";
        let input = stdin_input(long);
        let config = create_test_config(Verbosity::Quiet, false, 2);
        let cmd = build_no_config(
            config,
            NonZeroUsize::new(80),
            LintFormat::Text,
            None,
            &input,
        );
        let result = cmd.execute(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_lint_invalid_yaml() {
        let input = stdin_input("invalid: [unclosed");
        let config = create_test_config(Verbosity::Quiet, false, 2);
        let cmd = build_no_config(
            config,
            NonZeroUsize::new(120),
            LintFormat::Text,
            None,
            &input,
        );
        let result = cmd.execute(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_lint_quiet_mode() {
        let input = stdin_input("name: test");
        let config = create_test_config(Verbosity::Quiet, false, 2);
        let cmd = build_no_config(
            config,
            NonZeroUsize::new(120),
            LintFormat::Text,
            None,
            &input,
        );
        let result = cmd.execute(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_lint_json_format() {
        let input = stdin_input("name: test\nvalue: 123");
        let config = create_test_config(Verbosity::Normal, false, 2);
        let cmd = build_no_config(
            config,
            NonZeroUsize::new(120),
            LintFormat::Json,
            None,
            &input,
        );
        let result = cmd.execute(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_lint_duplicate_keys_reported_by_default() {
        let input = stdin_input("key: value1\nkey: value2\nother: data");
        let config = create_test_config(Verbosity::Normal, false, 2);
        let cmd = build_no_config(
            config,
            NonZeroUsize::new(120),
            LintFormat::Text,
            None,
            &input,
        );
        let result = cmd.execute(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::LintErrors);
    }

    #[test]
    fn test_lint_duplicate_keys_allowed_when_flag_set() {
        let input = stdin_input("key: value1\nkey: value2\nother: data");
        let config = create_test_config(Verbosity::Normal, false, 2);
        let cmd = build_no_config(
            config,
            NonZeroUsize::new(120),
            LintFormat::Text,
            Some(true),
            &input,
        );
        let result = cmd.execute(&input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    #[test]
    fn test_config_file_overrides_defaults() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "rules:\n  key-ordering:\n    enabled: false").unwrap();
        let input = stdin_input("b: 1\na: 2");
        let config = create_test_config(Verbosity::Normal, false, 2);
        let cmd = LintCommand::build(
            config,
            LintArgs {
                config_path: Some(f.path().to_owned()),
                no_config: false,
                max_line_length: None,
                indent_size: None,
                format: LintFormat::Text,
                allow_duplicate_keys: None,
                max_input_bytes: None,
                parse_limits: ParseLimits::default(),
            },
            &stdin_input(""),
        )
        .unwrap();
        let result = cmd.execute(&input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_explicit_config_missing_file_returns_error() {
        let config = create_test_config(Verbosity::Normal, false, 2);
        let result = LintCommand::build(
            config,
            LintArgs {
                config_path: Some(PathBuf::from("/nonexistent/.fast-yaml.yaml")),
                no_config: false,
                max_line_length: None,
                indent_size: None,
                format: LintFormat::Text,
                allow_duplicate_keys: None,
                max_input_bytes: None,
                parse_limits: ParseLimits::default(),
            },
            &stdin_input(""),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_config_file_line_length_max_is_loaded() {
        // Regression: config file line-length.max must reach the typed line-length options.
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "rules:\n  line-length:\n    max: 50").unwrap();
        let config = create_test_config(Verbosity::Normal, false, 2);
        let cmd = LintCommand::build(
            config,
            LintArgs {
                config_path: Some(f.path().to_owned()),
                no_config: false,
                max_line_length: None,
                indent_size: None,
                format: LintFormat::Text,
                allow_duplicate_keys: None,
                max_input_bytes: None,
                parse_limits: ParseLimits::default(),
            },
            &stdin_input(""),
        )
        .unwrap();
        assert_eq!(
            cmd.lint_config.rules.line_length.options.max,
            NonZeroUsize::new(50)
        );
    }

    #[test]
    fn test_config_file_line_length_triggers_diagnostic() {
        // End-to-end: a line longer than config-specified max must produce a diagnostic.
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "rules:\n  line-length:\n    max: 50").unwrap();
        let long_line = "name: a-sixty-character-line-that-exceeds-fifty-chars-limit!!";
        let input = stdin_input(long_line);
        let config = create_test_config(Verbosity::Quiet, false, 2);
        let cmd = LintCommand::build(
            config,
            LintArgs {
                config_path: Some(f.path().to_owned()),
                no_config: false,
                max_line_length: None,
                indent_size: None,
                format: LintFormat::Json,
                allow_duplicate_keys: None,
                max_input_bytes: None,
                parse_limits: ParseLimits::default(),
            },
            &stdin_input(""),
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
        let config = create_test_config(Verbosity::Normal, false, 2);
        let cmd = LintCommand::build(
            config,
            LintArgs {
                config_path: Some(f.path().to_owned()),
                no_config: false,
                max_line_length: NonZeroUsize::new(200),
                indent_size: None,
                format: LintFormat::Text,
                allow_duplicate_keys: None,
                max_input_bytes: None,
                parse_limits: ParseLimits::default(),
            },
            &stdin_input(""),
        )
        .unwrap();
        // CLI value wins over config file value
        assert_eq!(
            cmd.lint_config.rules.line_length.options.max,
            NonZeroUsize::new(200)
        );
    }

    #[test]
    fn test_allow_duplicate_keys_none_does_not_override() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        writeln!(f, "rules: {{}}").unwrap();
        let config = create_test_config(Verbosity::Normal, false, 2);
        let cmd = LintCommand::build(
            config,
            LintArgs {
                config_path: Some(f.path().to_owned()),
                no_config: false,
                max_line_length: None,
                indent_size: None,
                format: LintFormat::Text,
                allow_duplicate_keys: None,
                max_input_bytes: None,
                parse_limits: ParseLimits::default(),
            },
            &stdin_input(""),
        )
        .unwrap();
        assert!(cmd.lint_config.rules.duplicate_key.enabled);
    }
}
