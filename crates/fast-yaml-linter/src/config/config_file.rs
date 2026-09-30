//! Config file loading, discovery, and merging into `LintConfig`.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use fast_yaml_core::limits::{LimitRangeError, MaxInputBytes};
use fast_yaml_core::{DecodeError, ParseError, Parser, decode_input_owned};
use serde_norway::Value;

use crate::config::rules::value_kind;
use crate::config::{
    IgnorePatterns, IndentSize, Preset, RuleConfigError, RuleName, RulesConfig, YamlFiles,
};
use crate::echo::{KEY_LIMIT, MESSAGE_LIMIT, echo};
use crate::linter::LintConfig;

/// Depth limit for config file discovery walk-up.
const MAX_DISCOVERY_DEPTH: usize = 20;

/// Top-level structure of a `.fast-yaml.yaml` config file.
///
/// With `extends: default` or `extends: relaxed` the rules start from the matching yamllint
/// preset (see [`Preset`]); without `extends` they start from the fast-yaml defaults. `ignore`
/// and `yaml-files` select the files `fy lint` visits and follow yamllint's semantics.
///
/// The `max-input-bytes` key is specific to fast-yaml: an integer number of bytes (no size
/// suffixes) that caps the input the linter accepts. Omit it from files shared with yamllint.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use fast_yaml_linter::ConfigFile;
///
/// let config = ConfigFile::load(Path::new(".fast-yaml.yaml")).unwrap();
/// let (lint_config, files) = config.into_parts();
/// ```
#[derive(Debug, Clone, Default)]
pub struct ConfigFile {
    /// Typed settings of the built-in rules.
    pub rules: RulesConfig,
    /// Input size limit from `max-input-bytes`, or `None` when the file does not set it.
    pub max_input_bytes: Option<MaxInputBytes>,
    /// The `ignore` and `yaml-files` settings.
    pub selection: FileSelection,
}

/// The `ignore` and `yaml-files` settings of a config file.
#[derive(Debug, Clone, Default)]
pub struct FileSelection {
    /// Files excluded from linting (`ignore`), anchored at the config file's directory.
    pub ignore: Option<IgnorePatterns>,
    /// File-name patterns that select the files of a directory walk (`yaml-files`).
    pub yaml_files: Option<YamlFiles>,
}

/// Top-level keys of a config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TopLevelKey {
    /// `rules`
    Rules,
    /// `extends`
    Extends,
    /// `ignore`
    Ignore,
    /// `yaml-files`
    YamlFiles,
    /// `max-input-bytes`
    MaxInputBytes,
}

impl TopLevelKey {
    /// Returns the key as written in the config file.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rules => "rules",
            Self::Extends => "extends",
            Self::Ignore => "ignore",
            Self::YamlFiles => "yaml-files",
            Self::MaxInputBytes => "max-input-bytes",
        }
    }

    fn parse(key: &str) -> Option<Self> {
        [
            Self::Rules,
            Self::Extends,
            Self::Ignore,
            Self::YamlFiles,
            Self::MaxInputBytes,
        ]
        .into_iter()
        .find(|candidate| candidate.as_str() == key)
    }
}

impl std::fmt::Display for TopLevelKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Top-level keys yamllint accepts that fast-yaml does not implement.
const YAMLLINT_TOP_LEVEL_KEYS: [&str; 2] = ["ignore-from-file", "locale"];

/// Errors from config file loading.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigFileError {
    /// I/O error reading config file.
    #[error("failed to read config file '{}'", .path.display())]
    Io {
        /// Path that failed.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },

    /// The config file is not UTF-8 text (unsupported encoding or invalid bytes).
    #[error("failed to decode config file '{}'", .path.display())]
    Decode {
        /// Path that failed.
        path: PathBuf,
        /// Why the bytes could not be decoded.
        source: DecodeError,
    },

    /// The config text parsed under the core parser but failed in `serde_norway`, for example
    /// on duplicate keys, several documents or a malformed scalar.
    #[error("failed to parse config file '{}'", .path.display())]
    Parse {
        /// Path that failed.
        path: PathBuf,
        /// Underlying parse error.
        source: serde_norway::Error,
    },

    /// The core parser rejected the config file: a syntax error, or nesting depth or alias
    /// expansion beyond the default parser limits.
    #[error("failed to parse config file '{}'", .path.display())]
    Rejected {
        /// Path that failed.
        path: PathBuf,
        /// Why the parser rejected the file.
        source: ParseError,
    },

    /// The `rules:` section is invalid.
    #[error("invalid rules in config file '{}'", .path.display())]
    InvalidRules {
        /// Path that failed.
        path: PathBuf,
        /// What is wrong with the rules.
        source: RuleConfigError,
    },

    /// The file is not a mapping.
    #[error("config file '{}': expected a mapping with a 'rules' key", .path.display())]
    NotAMapping {
        /// Path that failed.
        path: PathBuf,
    },

    /// A top-level key is supported by yamllint but not by fast-yaml.
    #[error(
        "config file '{}': top-level key '{}' is supported by yamllint but not implemented by fast-yaml",
        .path.display(),
        echo(.key, KEY_LIMIT)
    )]
    UnsupportedKey {
        /// Path that failed.
        path: PathBuf,
        /// The unsupported key.
        key: String,
    },

    /// `max-input-bytes` is not a positive integer (negative, fractional, suffixed or not a number).
    #[error(
        "config file '{}': 'max-input-bytes' must be a positive integer number of bytes without a size suffix, got {found}",
        .path.display()
    )]
    MaxInputBytesNotPositive {
        /// Path that failed.
        path: PathBuf,
        /// The rejected value as written.
        found: String,
    },

    /// `max-input-bytes` is outside the accepted range.
    #[error("config file '{}': invalid 'max-input-bytes'", .path.display())]
    MaxInputBytesOutOfRange {
        /// Path that failed.
        path: PathBuf,
        /// The accepted range.
        source: LimitRangeError,
    },

    /// A top-level key is not recognized.
    #[error(
        "config file '{}': unknown top-level key '{}', expected 'rules', 'extends', 'ignore', 'yaml-files' or 'max-input-bytes'",
        .path.display(),
        echo(.key, KEY_LIMIT)
    )]
    UnknownKey {
        /// Path that failed.
        path: PathBuf,
        /// The unknown key.
        key: String,
    },

    /// The value of `extends`, `ignore` or `yaml-files` is invalid.
    #[error(
        "config file '{}': invalid '{key}': {}",
        .path.display(),
        echo(.message, MESSAGE_LIMIT)
    )]
    InvalidKey {
        /// Path that failed.
        path: PathBuf,
        /// The key with the invalid value.
        key: TopLevelKey,
        /// What is wrong with the value.
        message: String,
    },
}

/// Values of the top-level keys, collected before any of them is applied.
#[derive(Default)]
struct TopLevel {
    rules: Value,
    extends: Option<Preset>,
    ignore: Option<Vec<String>>,
    yaml_files: Option<Vec<String>>,
    max_input_bytes: Option<MaxInputBytes>,
}

fn string_items(value: Value, what: &str) -> Result<Vec<String>, String> {
    let Value::Sequence(items) = value else {
        return Err(format!("expected a list of {what}"));
    };
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| match item {
            Value::String(text) => Ok(text),
            _ => Err(format!("item {index} is not a string")),
        })
        .collect()
}

fn ignore_lines(value: Value) -> Result<Vec<String>, String> {
    match value {
        Value::String(text) => Ok(text.lines().map(str::to_owned).collect()),
        other => string_items(other, "patterns or a string with one pattern per line"),
    }
}

fn preset_of(value: &Value) -> Result<Preset, String> {
    let Value::String(name) = value else {
        return Err("expected 'default' or 'relaxed'".to_owned());
    };
    name.parse::<Preset>()
        .map_err(|error| format!("{error}; extending a config file is not implemented"))
}

/// Canonical directory of the config file, the anchor of `ignore` patterns.
fn config_root(path: &Path) -> Result<PathBuf, ConfigFileError> {
    let dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    dir.canonicalize().map_err(|source| ConfigFileError::Io {
        path: path.to_owned(),
        source,
    })
}

impl ConfigFile {
    /// Load and parse a config file from disk.
    ///
    /// # Errors
    ///
    /// Returns `ConfigFileError` on I/O failure, when the core parser rejects the file (syntax error or
    /// the default [`fast_yaml_core::limits::ParseLimits`] exceeded), or when `rules:` contains an
    /// unknown rule, an unknown or mistyped option, or an invalid severity.
    pub fn load(path: &Path) -> Result<Self, ConfigFileError> {
        let bytes = std::fs::read(path).map_err(|source| ConfigFileError::Io {
            path: path.to_owned(),
            source,
        })?;
        let content = decode_input_owned(bytes).map_err(|source| ConfigFileError::Decode {
            path: path.to_owned(),
            source,
        })?;
        // serde_norway has no depth or alias limits, so the core parser vets the text first.
        if let Err(source) = Parser::parse_all(&content) {
            return Err(ConfigFileError::Rejected {
                path: path.to_owned(),
                source,
            });
        }
        let parse_error = |source| ConfigFileError::Parse {
            path: path.to_owned(),
            source,
        };
        let entries = match serde_norway::from_str(&content).map_err(parse_error)? {
            Value::Null => serde_norway::Mapping::new(),
            Value::Mapping(entries) => entries,
            _ => {
                return Err(ConfigFileError::NotAMapping {
                    path: path.to_owned(),
                });
            }
        };
        let top = Self::collect_keys(path, entries)?;
        let invalid_rules = |source| ConfigFileError::InvalidRules {
            path: path.to_owned(),
            source,
        };
        let rules = if let Some(preset) = top.extends {
            let mut rules = preset.rules();
            rules.apply_over_preset(top.rules).map_err(invalid_rules)?;
            rules
        } else {
            let mut rules = RulesConfig::default();
            rules.apply(top.rules).map_err(invalid_rules)?;
            rules
        };
        let invalid_key =
            |key, error: crate::config::InvalidPathPattern| ConfigFileError::InvalidKey {
                path: path.to_owned(),
                key,
                message: error.to_string(),
            };
        let ignore = top
            .ignore
            .map(|lines| {
                IgnorePatterns::new(&config_root(path)?, &lines)
                    .map_err(|error| invalid_key(TopLevelKey::Ignore, error))
            })
            .transpose()?;
        let yaml_files = top
            .yaml_files
            .map(|lines| {
                YamlFiles::new(&lines).map_err(|error| invalid_key(TopLevelKey::YamlFiles, error))
            })
            .transpose()?;
        Ok(Self {
            rules,
            max_input_bytes: top.max_input_bytes,
            selection: FileSelection { ignore, yaml_files },
        })
    }

    fn collect_keys(
        path: &Path,
        entries: serde_norway::Mapping,
    ) -> Result<TopLevel, ConfigFileError> {
        let mut top = TopLevel::default();
        for (key, value) in entries {
            let key = match key {
                Value::String(key) => key,
                other => format!("{other:?}"),
            };
            let Some(known) = TopLevelKey::parse(&key) else {
                return Err(if YAMLLINT_TOP_LEVEL_KEYS.contains(&key.as_str()) {
                    ConfigFileError::UnsupportedKey {
                        path: path.to_owned(),
                        key,
                    }
                } else {
                    ConfigFileError::UnknownKey {
                        path: path.to_owned(),
                        key,
                    }
                });
            };
            let invalid = |message| ConfigFileError::InvalidKey {
                path: path.to_owned(),
                key: known,
                message,
            };
            match known {
                TopLevelKey::Rules => top.rules = value,
                TopLevelKey::Extends => top.extends = Some(preset_of(&value).map_err(invalid)?),
                TopLevelKey::Ignore => top.ignore = Some(ignore_lines(value).map_err(invalid)?),
                TopLevelKey::YamlFiles => {
                    top.yaml_files =
                        Some(string_items(value, "file name patterns").map_err(invalid)?);
                }
                TopLevelKey::MaxInputBytes => {
                    top.max_input_bytes = Some(parse_max_input_bytes(path, &value)?);
                }
            }
        }
        Ok(top)
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

    /// Splits into the `LintConfig` of the configured rules and the file selection.
    ///
    /// The file selection is not part of a `LintConfig`, so it is returned explicitly for the
    /// caller that discovers files.
    #[must_use]
    pub fn into_parts(self) -> (LintConfig, FileSelection) {
        (
            LintConfig {
                rules: self.rules,
                max_input_bytes: self.max_input_bytes.unwrap_or_default(),
                ..LintConfig::default()
            },
            self.selection,
        )
    }

    /// Apply CLI flag overrides on top of a config-derived `LintConfig`.
    /// Only overrides fields where the CLI option was explicitly provided
    /// (`Some(_)` values). `allow_duplicate_keys: Some(true)` disables the
    /// `duplicate-key` rule; `Some(false)` leaves it as configured.
    #[must_use]
    pub fn merge_cli_overrides(
        mut config: LintConfig,
        max_line_length: Option<NonZeroUsize>,
        indent_size: Option<IndentSize>,
        allow_duplicate_keys: Option<bool>,
    ) -> LintConfig {
        if let Some(max) = max_line_length {
            config.rules.line_length.options.max = Some(max);
        }
        if let Some(size) = indent_size {
            config.rules.indentation.options.indent_size = size;
        }
        if allow_duplicate_keys == Some(true) {
            config.rules.set_enabled(RuleName::DuplicateKey, false);
        }
        config
    }
}

fn parse_max_input_bytes(path: &Path, value: &Value) -> Result<MaxInputBytes, ConfigFileError> {
    let bytes = value
        .as_u64()
        .ok_or_else(|| ConfigFileError::MaxInputBytesNotPositive {
            path: path.to_owned(),
            found: match value {
                Value::Number(n) => n.to_string(),
                Value::String(text) => format!("'{}'", echo(text, KEY_LIMIT)),
                other => value_kind(other).to_owned(),
            },
        })?;
    usize::try_from(bytes)
        .ok()
        .map_or(
            Err(LimitRangeError {
                value: usize::MAX,
                max: MaxInputBytes::MAX.get(),
            }),
            MaxInputBytes::new,
        )
        .map_err(|source| ConfigFileError::MaxInputBytesOutOfRange {
            path: path.to_owned(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Linter;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    fn load_str(content: &str) -> Result<ConfigFile, ConfigFileError> {
        ConfigFile::load(write_temp(content).path())
    }

    fn rule_error(error: ConfigFileError) -> String {
        match error {
            ConfigFileError::InvalidRules { source, .. } => source.to_string(),
            other => panic!("expected InvalidRules, got {other:?}"),
        }
    }

    fn indent(size: u64) -> IndentSize {
        IndentSize::try_from(size).unwrap()
    }

    #[test]
    fn test_load_valid_config() {
        let cfg = load_str(
            "rules:\n  line-length:\n    enabled: true\n    max: 100\n  key-ordering:\n    enabled: false\n",
        )
        .unwrap();
        assert!(cfg.rules.line_length.enabled);
        assert_eq!(cfg.rules.line_length.options.max, NonZeroUsize::new(100));
        assert!(!cfg.rules.key_ordering.enabled);
    }

    #[test]
    fn test_max_input_bytes_key() {
        let cfg = load_str("max-input-bytes: 4096\n").unwrap();
        assert_eq!(cfg.max_input_bytes, Some(MaxInputBytes::new(4096).unwrap()));
        assert_eq!(
            cfg.into_parts().0.max_input_bytes,
            MaxInputBytes::new(4096).unwrap()
        );
        let unset = load_str("rules: {}\n").unwrap();
        assert_eq!(unset.max_input_bytes, None);
        assert_eq!(unset.into_parts().0.max_input_bytes, MaxInputBytes::DEFAULT);
    }

    #[test]
    fn test_max_input_bytes_rejects_invalid_values() {
        for (text, found) in [
            ("max-input-bytes: 1MiB\n", "got '1MiB'"),
            ("max-input-bytes: -5\n", "got -5"),
            ("max-input-bytes: 1.5\n", "got 1.5"),
            ("max-input-bytes: [1]\n", "got a list"),
        ] {
            let err = load_str(text).unwrap_err();
            assert!(
                matches!(err, ConfigFileError::MaxInputBytesNotPositive { .. }),
                "{text}: {err:?}"
            );
            let message = err.to_string();
            assert!(
                message.contains("positive integer") && message.contains(found),
                "{message}"
            );
        }
        for text in [
            "max-input-bytes: 0\n",
            "max-input-bytes: 4294967296000000\n",
        ] {
            let err = load_str(text).unwrap_err();
            assert!(
                matches!(err, ConfigFileError::MaxInputBytesOutOfRange { .. }),
                "{text}: {err:?}"
            );
        }
    }

    #[test]
    fn test_max_input_bytes_out_of_range_message_names_the_range() {
        let err = load_str("max-input-bytes: 0\n").unwrap_err();
        let source = std::error::Error::source(&err).unwrap().to_string();
        assert!(source.contains("between 1 and"), "{source}");
    }

    #[test]
    fn test_max_input_bytes_beyond_u64_is_a_parse_error_naming_the_key() {
        let err = load_str("max-input-bytes: 99999999999999999999\n").unwrap_err();
        assert!(matches!(err, ConfigFileError::Parse { .. }), "{err:?}");
        let source = std::error::Error::source(&err).unwrap().to_string();
        assert!(source.contains("max-input-bytes"), "{source}");
    }

    #[test]
    fn test_load_missing_file_returns_error() {
        let result = ConfigFile::load(Path::new("/nonexistent/path/.fast-yaml.yaml"));
        assert!(matches!(result, Err(ConfigFileError::Io { .. })));
    }

    #[test]
    fn test_load_utf16_config_returns_decode_error() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&[0xFF, 0xFE, b'r', 0x00]).unwrap();
        let err = ConfigFile::load(f.path()).unwrap_err();
        assert!(matches!(err, ConfigFileError::Decode { .. }), "{err:?}");
        let source = std::error::Error::source(&err).unwrap().to_string();
        assert!(source.contains("UTF-16LE"), "{source}");
    }

    #[test]
    fn test_load_invalid_yaml_returns_rejected_error() {
        assert!(matches!(
            load_str("rules: [broken yaml: {"),
            Err(ConfigFileError::Rejected { .. })
        ));
    }

    #[test]
    fn test_top_level_typo_is_rejected() {
        let err = load_str("rulez:\n  line-length: {max: 10}\n").unwrap_err();
        assert!(matches!(err, ConfigFileError::UnknownKey { .. }), "{err:?}");
        let message = err.to_string();
        assert!(
            message.contains("rulez") && message.contains("'rules'"),
            "{message}"
        );
    }

    #[test]
    fn test_yamllint_top_level_keys_are_unsupported() {
        for key in ["ignore-from-file", "locale"] {
            let err = load_str(&format!("{key}: default\nrules: {{}}\n")).unwrap_err();
            assert!(
                matches!(err, ConfigFileError::UnsupportedKey { .. }),
                "{key}: {err:?}"
            );
            let message = err.to_string();
            assert!(
                message.contains(key) && message.contains("yamllint"),
                "{message}"
            );
        }
    }

    #[test]
    fn test_non_mapping_file_is_rejected() {
        assert!(matches!(
            load_str("- a\n- b\n"),
            Err(ConfigFileError::NotAMapping { .. })
        ));
        assert!(matches!(
            load_str("5\n"),
            Err(ConfigFileError::NotAMapping { .. })
        ));
    }

    #[test]
    fn test_rules_must_be_a_mapping() {
        let err = load_str("rules: 5\n").unwrap_err();
        assert!(rule_error(err).contains("mapping of rule names"));
    }

    #[test]
    fn test_hostile_key_is_bounded_and_escaped() {
        let long = "k".repeat(100_000);
        let err = load_str(&format!("rules:\n  ? {long}\n  : 1\n")).unwrap_err();
        assert!(rule_error(err).len() < 1_000);
        let err = load_str("rules:\n  \"a\\e[2Jb\": 1\n").unwrap_err();
        let message = rule_error(err);
        assert!(!message.contains('\u{1b}'), "{message:?}");
        let err = load_str(&format!("? {long}\n: 1\n")).unwrap_err();
        assert!(err.to_string().len() < 1_000);
    }

    #[test]
    fn test_load_without_rules_section_uses_defaults() {
        assert_eq!(load_str("").unwrap().rules, RulesConfig::default());
        assert_eq!(load_str("rules:\n").unwrap().rules, RulesConfig::default());
        assert_eq!(load_str("rules: {}").unwrap().rules, RulesConfig::default());
    }

    #[test]
    fn test_unknown_rule_is_hard_error() {
        let err = load_str("rules:\n  unknown-rule-xyz:\n    enabled: true\n").unwrap_err();
        assert!(matches!(
            err,
            ConfigFileError::InvalidRules {
                source: RuleConfigError::UnknownRule(_),
                ..
            }
        ));
        assert!(rule_error(err).contains("unknown-rule-xyz"));
    }

    #[test]
    fn test_option_typo_names_rule_and_key() {
        let err = load_str("rules:\n  quoted-strings:\n    quote-type: singel\n").unwrap_err();
        let message = rule_error(err);
        assert!(message.contains("quoted-strings"), "{message}");
        assert!(message.contains("quote-type"), "{message}");
    }

    #[test]
    fn test_into_parts_disables_rule() {
        let lint_config = load_str("rules:\n  key-ordering:\n    enabled: false\n")
            .unwrap()
            .into_parts()
            .0;
        assert!(!lint_config.is_rule_enabled("key-ordering"));
    }

    #[test]
    fn test_line_length_max_actually_affects_linting() {
        let long_line = "name: a-sixty-character-line-that-exceeds-fifty-chars-limit!!";
        assert_eq!(long_line.len(), 61);
        let lint_config = load_str("rules:\n  line-length:\n    max: 50\n")
            .unwrap()
            .into_parts()
            .0;
        let diagnostics = Linter::with_config(lint_config).lint(long_line).unwrap();
        assert!(diagnostics.iter().any(|d| d.code.as_str() == "line-length"));
    }

    #[test]
    fn test_line_length_default_not_triggered_for_short_line() {
        let short_line = "name: this-line-is-about-sixty-characters-long-no-more-here";
        let lint_config = load_str("rules: {}").unwrap().into_parts().0;
        let diagnostics = Linter::with_config(lint_config).lint(short_line).unwrap();
        assert!(!diagnostics.iter().any(|d| d.code.as_str() == "line-length"));
    }

    #[test]
    fn test_indentation_indent_size_actually_affects_linting() {
        let yaml = "list:\n  - item\n";
        let lint_config = load_str("rules:\n  indentation:\n    indent-size: 4\n")
            .unwrap()
            .into_parts()
            .0;
        let diagnostics = Linter::with_config(lint_config).lint(yaml).unwrap();
        assert!(diagnostics.iter().any(|d| d.code.as_str() == "indentation"));
    }

    #[test]
    fn test_indentation_default_not_triggered_for_2space() {
        let yaml = "list:\n  - item\n";
        let lint_config = load_str("rules: {}").unwrap().into_parts().0;
        let diagnostics = Linter::with_config(lint_config).lint(yaml).unwrap();
        assert!(!diagnostics.iter().any(|d| d.code.as_str() == "indentation"));
    }

    #[test]
    fn test_merge_cli_overrides_takes_precedence() {
        let result = ConfigFile::merge_cli_overrides(
            LintConfig::default(),
            NonZeroUsize::new(200),
            Some(indent(4)),
            Some(true),
        );
        assert_eq!(result.rules.line_length.options.max, NonZeroUsize::new(200));
        assert_eq!(result.rules.indentation.options.indent_size.get(), 4);
        assert!(!result.rules.duplicate_key.enabled);
    }

    #[test]
    fn test_merge_cli_overrides_none_does_not_override() {
        let base = LintConfig::new()
            .with_max_line_length(NonZeroUsize::new(42))
            .with_indent_size(indent(3));
        let result = ConfigFile::merge_cli_overrides(base, None, None, Some(false));
        assert_eq!(result.rules.line_length.options.max, NonZeroUsize::new(42));
        assert_eq!(result.rules.indentation.options.indent_size.get(), 3);
        assert!(result.rules.duplicate_key.enabled);
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
    fn load_alias_bomb_config_rejected() {
        use std::fmt::Write as _;
        let mut yaml = format!("a0: &a0 \"{}\"\n", "x".repeat(1024));
        for i in 1..=8 {
            let refs = vec![format!("*a{}", i - 1); 9].join(",");
            writeln!(yaml, "a{i}: &a{i} [{refs}]").unwrap();
        }
        assert!(matches!(
            load_str(&yaml),
            Err(ConfigFileError::Rejected { .. })
        ));
    }

    fn load_in(dir: &tempfile::TempDir, content: &str) -> Result<ConfigFile, ConfigFileError> {
        let path = dir.path().join(".fast-yaml.yaml");
        std::fs::write(&path, content).unwrap();
        ConfigFile::load(&path)
    }

    fn invalid_key(error: ConfigFileError) -> (TopLevelKey, String) {
        match error {
            ConfigFileError::InvalidKey { key, message, .. } => (key, message),
            other => panic!("expected InvalidKey, got {other:?}"),
        }
    }

    #[test]
    fn extends_starts_from_the_preset() {
        let default = load_str("extends: default\n").unwrap();
        assert_eq!(default.rules, Preset::Default.rules());
        let relaxed = load_str("extends: relaxed\nrules:\n  line-length: {max: 120}\n").unwrap();
        assert_eq!(
            relaxed.rules.line_length.options.max,
            NonZeroUsize::new(120)
        );
        assert_eq!(
            relaxed.rules.line_length.severity,
            Some(crate::Severity::Warning)
        );
        assert!(!relaxed.rules.comments.enabled);
    }

    #[test]
    fn key_order_does_not_matter_for_extends() {
        let first = load_str("extends: relaxed\nrules:\n  colons: disable\n").unwrap();
        let last = load_str("rules:\n  colons: disable\nextends: relaxed\n").unwrap();
        assert_eq!(first.rules, last.rules);
        assert!(!last.rules.colons.enabled);
    }

    #[test]
    fn rules_override_enables_a_preset_disabled_rule_with_error_severity() {
        for rules in [
            "quoted-strings: enable",
            "quoted-strings: {quote-type: single}",
            "quoted-strings: warning",
        ] {
            let cfg = load_str(&format!("extends: default\nrules:\n  {rules}\n")).unwrap();
            assert!(cfg.rules.quoted_strings.enabled, "{rules}");
            assert_eq!(
                cfg.rules.quoted_strings.options.required,
                crate::rules::QuoteRequirement::Always,
                "{rules}"
            );
        }
        let cfg = load_str("extends: relaxed\nrules:\n  truthy: {check-keys: false}\n").unwrap();
        assert!(cfg.rules.truthy.enabled);
        assert_eq!(cfg.rules.truthy.severity, Some(crate::Severity::Error));
        let cfg = load_str("extends: default\nrules:\n  document-end: enable\n").unwrap();
        assert_eq!(
            cfg.rules.document_end.severity,
            Some(crate::Severity::Error)
        );
    }

    #[test]
    fn without_extends_a_mapping_does_not_enable_a_disabled_rule() {
        let cfg = load_str("rules:\n  braces: {enabled: false}\n").unwrap();
        assert!(!cfg.rules.braces.enabled);
        assert_eq!(
            cfg.rules.quoted_strings,
            RulesConfig::default().quoted_strings
        );
    }

    #[test]
    fn invalid_extends_is_reported() {
        for (content, needle) in [
            (
                "extends: ./base.yaml\n",
                "extending a config file is not implemented",
            ),
            ("extends: strict\n", "'default' or 'relaxed'"),
            ("extends: [default]\n", "expected 'default' or 'relaxed'"),
        ] {
            let (key, message) = invalid_key(load_str(content).unwrap_err());
            assert_eq!(key, TopLevelKey::Extends);
            assert!(message.contains(needle), "{content}: {message}");
        }
    }

    #[test]
    fn ignore_accepts_a_block_string_or_a_list() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        for content in [
            "ignore: |\n  vendor/\n  # comment\n  *.tmp.yaml\n",
            "ignore: ['vendor/', '*.tmp.yaml']\n",
        ] {
            let ignore = load_in(&dir, content).unwrap().selection.ignore.unwrap();
            assert!(
                ignore.matches(&root.join("vendor/a.yaml"), false),
                "{content}"
            );
            assert!(
                ignore.matches(&root.join("x/a.tmp.yaml"), false),
                "{content}"
            );
            assert!(!ignore.matches(&root.join("a.yaml"), false), "{content}");
        }
    }

    #[test]
    fn invalid_ignore_and_yaml_files_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        for (content, key) in [
            ("ignore: 5\n", TopLevelKey::Ignore),
            ("ignore: [a, 1]\n", TopLevelKey::Ignore),
            ("ignore: ~\n", TopLevelKey::Ignore),
            ("ignore: ['{a']\n", TopLevelKey::Ignore),
            ("yaml-files: '*.yaml'\n", TopLevelKey::YamlFiles),
            ("yaml-files: [1]\n", TopLevelKey::YamlFiles),
            ("yaml-files: ['{a']\n", TopLevelKey::YamlFiles),
        ] {
            let (found, message) = invalid_key(load_in(&dir, content).unwrap_err());
            assert_eq!(found, key, "{content}");
            assert!(!message.is_empty());
        }
    }

    #[test]
    fn yaml_files_match_file_names() {
        let cfg = load_str("yaml-files: ['*.yaml.j2']\n").unwrap();
        let files = cfg.selection.yaml_files.unwrap();
        assert!(files.matches(Path::new("dir/a.yaml.j2")));
        assert!(!files.matches(Path::new("dir/a.yaml")));
        assert!(cfg.selection.ignore.is_none());
    }

    #[test]
    fn load_multi_document_config_errors() {
        assert!(matches!(
            load_str("---\nrules: {}\n---\nrules: {}\n"),
            Err(ConfigFileError::Parse { .. })
        ));
    }
}
