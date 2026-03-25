//! Config file loading, discovery, and merging into `LintConfig`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::Severity;
use crate::config::{RuleConfig, RuleOption};
use crate::linter::LintConfig;

/// All known rule codes. Used to validate rule names from config files.
const KNOWN_RULE_CODES: &[&str] = &[
    "duplicate-key",
    "invalid-anchor",
    "undefined-alias",
    "indentation",
    "line-length",
    "trailing-whitespace",
    "document-start",
    "document-end",
    "empty-values",
    "new-line-at-end-of-file",
    "braces",
    "brackets",
    "colons",
    "commas",
    "hyphens",
    "comments",
    "comments-indentation",
    "empty-lines",
    "new-lines",
    "octal-values",
    "truthy",
    "quoted-strings",
    "key-ordering",
    "float-values",
];

/// Depth limit for config file discovery walk-up.
const MAX_DISCOVERY_DEPTH: usize = 20;

/// Top-level structure of a `.fast-yaml.yaml` config file.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigFile {
    /// Map of rule code to per-rule configuration.
    #[serde(default)]
    pub rules: HashMap<String, ConfigFileRule>,
}

/// Per-rule configuration entry from the config file.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigFileRule {
    /// Whether this rule is enabled.
    pub enabled: Option<bool>,
    /// Override the rule's default severity.
    pub severity: Option<ConfigFileSeverity>,
    /// Rule-specific options.
    #[serde(flatten)]
    pub options: HashMap<String, ConfigFileValue>,
}

/// Severity value as parsed from the config file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigFileSeverity {
    /// Critical error severity.
    Error,
    /// Warning severity.
    Warning,
    /// Informational severity.
    Info,
    /// Hint severity.
    Hint,
}

impl From<ConfigFileSeverity> for Severity {
    fn from(s: ConfigFileSeverity) -> Self {
        match s {
            ConfigFileSeverity::Error => Self::Error,
            ConfigFileSeverity::Warning => Self::Warning,
            ConfigFileSeverity::Info => Self::Info,
            ConfigFileSeverity::Hint => Self::Hint,
        }
    }
}

/// Untyped option value from a config file rule block.
///
/// Variant ordering matters for `serde(untagged)`: `Bool` must come before
/// `Int` because YAML integers are not booleans, but serde tries variants in
/// order and stops at first match.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ConfigFileValue {
    /// Boolean value (true/false).
    Bool(bool),
    /// Integer value.
    Int(i64),
    /// String value.
    String(String),
    /// List of strings.
    StringList(Vec<String>),
}

/// Errors from config file loading.
#[derive(Debug, thiserror::Error)]
pub enum ConfigFileError {
    /// I/O error reading config file.
    #[error("failed to read config file '{path}': {source}")]
    Io {
        /// Path that failed.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },

    /// YAML parse error in config file.
    #[error("failed to parse config file '{path}': {source}")]
    Parse {
        /// Path that failed.
        path: PathBuf,
        /// Underlying parse error.
        source: serde_norway::Error,
    },
}

impl ConfigFile {
    /// Load and parse a config file from disk.
    ///
    /// # Errors
    ///
    /// Returns `ConfigFileError` on I/O or parse failure.
    pub fn load(path: &Path) -> Result<Self, ConfigFileError> {
        let content = std::fs::read_to_string(path).map_err(|source| ConfigFileError::Io {
            path: path.to_owned(),
            source,
        })?;
        serde_norway::from_str(&content).map_err(|source| ConfigFileError::Parse {
            path: path.to_owned(),
            source,
        })
    }

    /// Walk up the directory tree from `start_dir` looking for `.fast-yaml.yaml`
    /// or `.fast-yaml.yml`. Returns the first found path, or `None` if not found.
    ///
    /// Uses iterative `parent()` instead of `canonicalize()` to avoid following
    /// symlinks across filesystems. Depth is capped at `MAX_DISCOVERY_DEPTH`.
    pub fn discover(start_dir: &Path) -> Option<PathBuf> {
        let mut dir = start_dir.to_owned();
        for _ in 0..MAX_DISCOVERY_DEPTH {
            for name in [".fast-yaml.yaml", ".fast-yaml.yml"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            if !dir.pop() {
                return None;
            }
        }
        None
    }

    /// Validate rule names against the known set. Emit warnings to stderr for
    /// unknown names so users get feedback on typos.
    pub fn warn_unknown_rules(&self) {
        for name in self.rules.keys() {
            if !KNOWN_RULE_CODES.contains(&name.as_str()) {
                eprintln!("warning: unknown rule '{name}' in config file");
            }
        }
    }

    /// Convert into a `LintConfig`, applying all `rules:` entries.
    #[must_use]
    pub fn into_lint_config(self) -> LintConfig {
        let mut config = LintConfig::default();

        for (rule_name, rule_cfg) in self.rules {
            if !KNOWN_RULE_CODES.contains(&rule_name.as_str()) {
                continue; // already warned above
            }

            let enabled = rule_cfg.enabled.unwrap_or(true);
            let mut rc = if enabled {
                RuleConfig::new()
            } else {
                RuleConfig::disabled()
            };

            if let Some(sev) = rule_cfg.severity {
                rc = rc.with_severity(sev.into());
            }

            for (key, val) in rule_cfg.options {
                // Special case: line-length.max maps to the top-level LintConfig field because
                // LineLengthRule reads config.max_line_length directly, not rule_configs.
                if rule_name == "line-length"
                    && key == "max"
                    && let ConfigFileValue::Int(max) = &val
                    && let Ok(max_usize) = usize::try_from(*max)
                {
                    config.max_line_length = Some(max_usize);
                }
                // Special case: indentation.indent-size maps to the top-level LintConfig field
                // because IndentationRule reads config.indent_size directly, not rule_configs.
                if rule_name == "indentation"
                    && key == "indent-size"
                    && let ConfigFileValue::Int(size) = &val
                    && let Ok(size_usize) = usize::try_from(*size)
                {
                    config.indent_size = size_usize;
                }
                let opt = match val {
                    ConfigFileValue::Bool(b) => RuleOption::Bool(b),
                    ConfigFileValue::Int(i) => RuleOption::Int(i),
                    ConfigFileValue::String(s) => RuleOption::String(s),
                    ConfigFileValue::StringList(v) => RuleOption::StringList(v),
                };
                rc = rc.with_option(key, opt);
            }

            config = config.with_rule_config(rule_name, rc);
        }

        config
    }

    /// Apply CLI flag overrides on top of a config-derived `LintConfig`.
    /// Only overrides fields where the CLI option was explicitly provided
    /// (`Some(_)` values).
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // cannot be const: contains if-let with Option
    pub fn merge_cli_overrides(
        mut config: LintConfig,
        max_line_length: Option<usize>,
        indent_size: Option<usize>,
        allow_duplicate_keys: Option<bool>,
    ) -> LintConfig {
        if let Some(v) = max_line_length {
            config.max_line_length = Some(v);
        }
        if let Some(v) = indent_size {
            config.indent_size = v;
        }
        if let Some(v) = allow_duplicate_keys {
            config.allow_duplicate_keys = v;
        }
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn test_load_valid_config() {
        let f = write_temp(
            "rules:\n  line-length:\n    enabled: true\n    max: 100\n  key-ordering:\n    enabled: false\n",
        );
        let cfg = ConfigFile::load(f.path()).unwrap();
        assert!(cfg.rules.contains_key("line-length"));
        assert_eq!(cfg.rules["line-length"].enabled, Some(true));
        assert_eq!(cfg.rules["key-ordering"].enabled, Some(false));
    }

    #[test]
    fn test_load_missing_file_returns_error() {
        let result = ConfigFile::load(Path::new("/nonexistent/path/.fast-yaml.yaml"));
        assert!(matches!(result, Err(ConfigFileError::Io { .. })));
    }

    #[test]
    fn test_load_invalid_yaml_returns_parse_error() {
        let f = write_temp("rules: [broken yaml: {");
        let result = ConfigFile::load(f.path());
        assert!(matches!(result, Err(ConfigFileError::Parse { .. })));
    }

    #[test]
    fn test_into_lint_config_disables_rule() {
        let f = write_temp("rules:\n  key-ordering:\n    enabled: false\n");
        let cfg = ConfigFile::load(f.path()).unwrap();
        let lint_config = cfg.into_lint_config();
        assert!(!lint_config.is_rule_enabled("key-ordering"));
    }

    #[test]
    fn test_into_lint_config_sets_options() {
        let f = write_temp("rules:\n  line-length:\n    max: 100\n");
        let cfg = ConfigFile::load(f.path()).unwrap();
        let lint_config = cfg.into_lint_config();
        let rc = lint_config.get_rule_config("line-length").unwrap();
        assert_eq!(rc.options.get_usize("max"), Some(100));
    }

    #[test]
    fn test_line_length_max_sets_top_level_field() {
        // Regression: line-length.max from config file must propagate to LintConfig::max_line_length
        // because LineLengthRule reads that field directly, not rule_configs.
        let f = write_temp("rules:\n  line-length:\n    max: 50\n");
        let cfg = ConfigFile::load(f.path()).unwrap();
        let lint_config = cfg.into_lint_config();
        assert_eq!(lint_config.max_line_length, Some(50));
    }

    #[test]
    fn test_line_length_max_actually_affects_linting() {
        use crate::Linter;
        // A 60-character line should trigger a diagnostic when max=50 is set via config file.
        let long_line = "name: a-sixty-character-line-that-exceeds-fifty-chars-limit!!";
        assert_eq!(long_line.len(), 61);

        let f = write_temp("rules:\n  line-length:\n    max: 50\n");
        let cfg = ConfigFile::load(f.path()).unwrap();
        let lint_config = cfg.into_lint_config();

        let linter = Linter::with_config(lint_config);
        let diagnostics = linter.lint(long_line).unwrap();
        assert!(
            diagnostics.iter().any(|d| d.code.as_str() == "line-length"),
            "expected line-length diagnostic for 61-char line with max=50"
        );
    }

    #[test]
    fn test_line_length_default_not_triggered_for_short_line() {
        use crate::Linter;
        // Without config, default max_line_length is Some(80). A 60-char line should not trigger.
        let short_line = "name: this-line-is-about-sixty-characters-long-no-more-here";
        assert!(short_line.len() < 80);

        let f = write_temp("rules: {}");
        let cfg = ConfigFile::load(f.path()).unwrap();
        let lint_config = cfg.into_lint_config();

        let linter = Linter::with_config(lint_config);
        let diagnostics = linter.lint(short_line).unwrap();
        assert!(
            !diagnostics.iter().any(|d| d.code.as_str() == "line-length"),
            "expected no line-length diagnostic for <80-char line with default config"
        );
    }

    #[test]
    fn test_indentation_indent_size_sets_top_level_field() {
        // Regression: indentation.indent-size from config file must propagate to
        // LintConfig::indent_size because IndentationRule reads that field directly.
        let f = write_temp("rules:\n  indentation:\n    indent-size: 4\n");
        let cfg = ConfigFile::load(f.path()).unwrap();
        let lint_config = cfg.into_lint_config();
        assert_eq!(lint_config.indent_size, 4);
    }

    #[test]
    fn test_indentation_indent_size_actually_affects_linting() {
        use crate::Linter;
        // With indent-size: 4 set via config, 2-space indented YAML should produce a diagnostic.
        let yaml = "list:\n  - item\n";

        let f = write_temp("rules:\n  indentation:\n    indent-size: 4\n");
        let cfg = ConfigFile::load(f.path()).unwrap();
        let lint_config = cfg.into_lint_config();

        let linter = Linter::with_config(lint_config);
        let diagnostics = linter.lint(yaml).unwrap();
        assert!(
            diagnostics.iter().any(|d| d.code.as_str() == "indentation"),
            "expected indentation diagnostic for 2-space indent with indent-size=4 config"
        );
    }

    #[test]
    fn test_indentation_default_not_triggered_for_2space() {
        use crate::Linter;
        // Without config override, default indent_size is 2. 2-space indented YAML is valid.
        let yaml = "list:\n  - item\n";

        let f = write_temp("rules: {}");
        let cfg = ConfigFile::load(f.path()).unwrap();
        let lint_config = cfg.into_lint_config();

        let linter = Linter::with_config(lint_config);
        let diagnostics = linter.lint(yaml).unwrap();
        assert!(
            !diagnostics.iter().any(|d| d.code.as_str() == "indentation"),
            "expected no indentation diagnostic for 2-space indent with default config"
        );
    }

    #[test]
    fn test_merge_cli_overrides_takes_precedence() {
        let base = LintConfig::default();
        let result = ConfigFile::merge_cli_overrides(base, Some(200), Some(4), Some(true));
        assert_eq!(result.max_line_length, Some(200));
        assert_eq!(result.indent_size, 4);
        assert!(result.allow_duplicate_keys);
    }

    #[test]
    fn test_merge_cli_overrides_none_does_not_override() {
        let base = LintConfig::new()
            .with_max_line_length(Some(42))
            .with_indent_size(3);
        let result = ConfigFile::merge_cli_overrides(base, None, None, None);
        assert_eq!(result.max_line_length, Some(42));
        assert_eq!(result.indent_size, 3);
    }

    #[test]
    fn test_discover_finds_config_in_same_dir() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".fast-yaml.yaml");
        std::fs::write(&config_path, "rules: {}").unwrap();

        let found = ConfigFile::discover(dir.path());
        assert_eq!(found, Some(config_path));
    }

    #[test]
    fn test_discover_finds_config_in_parent() {
        let parent = tempfile::tempdir().unwrap();
        let child = parent.path().join("subdir");
        std::fs::create_dir(&child).unwrap();
        let config_path = parent.path().join(".fast-yaml.yaml");
        std::fs::write(&config_path, "rules: {}").unwrap();

        let found = ConfigFile::discover(&child);
        assert_eq!(found, Some(config_path));
    }

    #[test]
    fn test_discover_returns_none_when_not_found() {
        let found = ConfigFile::discover(Path::new("/"));
        assert!(found.is_none());
    }

    #[test]
    fn test_warn_unknown_rules_does_not_panic() {
        let f = write_temp("rules:\n  unknown-rule-xyz:\n    enabled: true\n");
        let cfg = ConfigFile::load(f.path()).unwrap();
        // Should not panic, warns to stderr
        cfg.warn_unknown_rules();
    }

    #[test]
    fn test_config_file_value_bool_ordering() {
        let f = write_temp("rules:\n  truthy:\n    allow-bool-values: true\n");
        let cfg = ConfigFile::load(f.path()).unwrap();
        let rule = &cfg.rules["truthy"];
        assert!(matches!(
            rule.options.get("allow-bool-values"),
            Some(ConfigFileValue::Bool(true))
        ));
    }

    #[test]
    fn test_config_file_severity_deserialization() {
        let f = write_temp("rules:\n  line-length:\n    severity: warning\n");
        let cfg = ConfigFile::load(f.path()).unwrap();
        let rule = &cfg.rules["line-length"];
        assert!(matches!(rule.severity, Some(ConfigFileSeverity::Warning)));
    }
}
