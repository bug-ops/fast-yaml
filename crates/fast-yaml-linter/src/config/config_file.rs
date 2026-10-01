//! Config file loading, discovery, and merging into `LintConfig`.

use std::io::Read;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use fast_yaml_core::limits::{
    Bounded, Bounds, LimitRangeError, MaxInputBytes, MaxScanAhead, ParseLimits,
};
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

/// Longest chain of config files linked by `extends`, the extending file included.
pub const MAX_EXTENDS_DEPTH: usize = 8;

/// Largest accepted `ignore-from-file` file, in bytes.
const MAX_IGNORE_FILE_BYTES: usize = 1 << 20;

/// Top-level structure of a `.fast-yaml.yaml` config file.
///
/// With `extends: default` or `extends: relaxed` the rules start from the matching yamllint
/// preset (see [`Preset`]); without `extends` they start from the fast-yaml defaults. Any other
/// `extends` value is the path of another config file, resolved against the directory of the
/// file that names it (yamllint resolves it against the working directory). The extended file is
/// loaded first, recursively (at most [`MAX_EXTENDS_DEPTH`] files deep, cycles are rejected), and
/// the rules of the extending file are applied over it like over a preset. `max-input-bytes`,
/// `max-scan-ahead` and `ignore` are inherited when the extending file does not set them;
/// `yaml-files` is not inherited, as in yamllint.
///
/// `ignore` and `yaml-files` select the files `fy lint` visits and follow yamllint's semantics.
/// `ignore-from-file` names one file or a list of files (relative to the config file's
/// directory) whose lines are ignore patterns; it replaces `ignore`, and the two cannot be used
/// together. The patterns are read when the config is loaded, so they end up in
/// [`FileSelection::ignore`], anchored at the config file's directory.
///
/// The `max-input-bytes` key is specific to fast-yaml: an integer number of bytes (no size
/// suffixes) that caps the input the linter accepts. The `max-scan-ahead` key is likewise an
/// integer, in characters, that sets [`MaxScanAhead`]. Omit both from files shared with yamllint.
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
    /// Scan-ahead limit from `max-scan-ahead`, or `None` when the file does not set it.
    pub max_scan_ahead: Option<MaxScanAhead>,
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
    /// `ignore-from-file`
    IgnoreFromFile,
    /// `yaml-files`
    YamlFiles,
    /// `max-input-bytes`
    MaxInputBytes,
    /// `max-scan-ahead`
    MaxScanAhead,
}

impl TopLevelKey {
    /// Returns the key as written in the config file.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rules => "rules",
            Self::Extends => "extends",
            Self::Ignore => "ignore",
            Self::IgnoreFromFile => "ignore-from-file",
            Self::YamlFiles => "yaml-files",
            Self::MaxInputBytes => "max-input-bytes",
            Self::MaxScanAhead => "max-scan-ahead",
        }
    }

    fn parse(key: &str) -> Option<Self> {
        [
            Self::Rules,
            Self::Extends,
            Self::Ignore,
            Self::IgnoreFromFile,
            Self::YamlFiles,
            Self::MaxInputBytes,
            Self::MaxScanAhead,
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
const YAMLLINT_TOP_LEVEL_KEYS: [&str; 1] = ["locale"];

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

    /// A limit key is not a positive integer (negative, fractional, suffixed or not a number).
    #[error(
        "config file '{}': '{key}' must be a positive integer without a size suffix, got {found}",
        .path.display()
    )]
    LimitNotPositive {
        /// Path that failed.
        path: PathBuf,
        /// The limit key.
        key: TopLevelKey,
        /// The rejected value as written.
        found: String,
    },

    /// A limit key is outside the accepted range.
    #[error("config file '{}': invalid '{key}'", .path.display())]
    LimitOutOfRange {
        /// Path that failed.
        path: PathBuf,
        /// The limit key.
        key: TopLevelKey,
        /// The accepted range.
        source: LimitRangeError,
    },

    /// A top-level key is not recognized.
    #[error(
        "config file '{}': unknown top-level key '{}', expected 'rules', 'extends', 'ignore', 'ignore-from-file', 'yaml-files', 'max-input-bytes' or 'max-scan-ahead'",
        .path.display(),
        echo(.key, KEY_LIMIT)
    )]
    UnknownKey {
        /// Path that failed.
        path: PathBuf,
        /// The unknown key.
        key: String,
    },

    /// The file named by `extends` could not be loaded.
    #[error("config file '{}': failed to load the file named by 'extends'", .path.display())]
    Extended {
        /// The extending config file.
        path: PathBuf,
        /// Why the extended file failed.
        source: Box<Self>,
    },

    /// A config file is its own ancestor through `extends`.
    #[error("config file '{}': 'extends' leads back to this file", .path.display())]
    ExtendsCycle {
        /// The file met twice.
        path: PathBuf,
    },

    /// The `extends` chain is longer than [`MAX_EXTENDS_DEPTH`] files.
    #[error("config file '{}': 'extends' is nested more than {MAX_EXTENDS_DEPTH} files deep", .path.display())]
    ExtendsTooDeep {
        /// The first file beyond the limit.
        path: PathBuf,
    },

    /// The value of `extends`, `ignore`, `ignore-from-file` or `yaml-files` is invalid.
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
    extends: Option<Extends>,
    ignore: Option<Vec<String>>,
    ignore_from_file: Option<Vec<String>>,
    yaml_files: Option<Vec<String>>,
    max_input_bytes: Option<MaxInputBytes>,
    max_scan_ahead: Option<MaxScanAhead>,
}

/// What `extends` names.
enum Extends {
    Preset(Preset),
    /// A config file, as written.
    File(PathBuf),
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

fn extends_of(value: &Value) -> Result<Extends, String> {
    match value {
        Value::String(name) if name.is_empty() => Err("the path is empty".to_owned()),
        Value::String(name) => Ok(name
            .parse::<Preset>()
            .map_or_else(|_| Extends::File(PathBuf::from(name)), Extends::Preset)),
        _ => Err("expected 'default', 'relaxed' or the path of a config file".to_owned()),
    }
}

fn ignore_file_names(value: Value) -> Result<Vec<String>, String> {
    match value {
        Value::String(name) => Ok(vec![name]),
        other => string_items(other, "file names or one file name"),
    }
}

/// Directory of the config file, where relative paths in it start.
fn config_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// Lines of the `ignore-from-file` files, each at most [`MAX_IGNORE_FILE_BYTES`] long.
fn ignore_file_lines(path: &Path, names: &[String]) -> Result<Vec<String>, ConfigFileError> {
    let mut lines = Vec::new();
    for name in names {
        let file_path = config_dir(path).join(name);
        let io = |source| ConfigFileError::Io {
            path: file_path.clone(),
            source,
        };
        let mut bytes = Vec::new();
        std::fs::File::open(&file_path)
            .and_then(|file| {
                file.take(MAX_IGNORE_FILE_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
            })
            .map_err(io)?;
        if bytes.len() > MAX_IGNORE_FILE_BYTES {
            return Err(ConfigFileError::InvalidKey {
                path: path.to_owned(),
                key: TopLevelKey::IgnoreFromFile,
                message: format!(
                    "'{}' is larger than {MAX_IGNORE_FILE_BYTES} bytes",
                    echo(name, KEY_LIMIT)
                ),
            });
        }
        let text = decode_input_owned(bytes).map_err(|source| ConfigFileError::Decode {
            path: file_path.clone(),
            source,
        })?;
        lines.extend(text.lines().map(str::to_owned));
    }
    Ok(lines)
}

/// Canonical directory of the config file, the anchor of `ignore` patterns.
fn config_root(path: &Path) -> Result<PathBuf, ConfigFileError> {
    config_dir(path)
        .canonicalize()
        .map_err(|source| ConfigFileError::Io {
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
    ///
    /// The file must be valid YAML 1.2, like every input of `fy`: a tab as the indentation of a
    /// flow collection's continuation line is rejected, and the error names its line and column.
    /// Indent with spaces.
    pub fn load(path: &Path) -> Result<Self, ConfigFileError> {
        Self::load_chain(path, &mut Vec::new())
    }

    /// Loads `path`, with `chain` holding the canonical paths of the files it is extending from.
    fn load_chain(path: &Path, chain: &mut Vec<PathBuf>) -> Result<Self, ConfigFileError> {
        let canonical = path.canonicalize().map_err(|source| ConfigFileError::Io {
            path: path.to_owned(),
            source,
        })?;
        if chain.contains(&canonical) {
            return Err(ConfigFileError::ExtendsCycle {
                path: path.to_owned(),
            });
        }
        if chain.len() >= MAX_EXTENDS_DEPTH {
            return Err(ConfigFileError::ExtendsTooDeep {
                path: path.to_owned(),
            });
        }
        chain.push(canonical);
        let loaded = Self::load_file(path, chain);
        chain.pop();
        loaded
    }

    fn load_file(path: &Path, chain: &mut Vec<PathBuf>) -> Result<Self, ConfigFileError> {
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
        let (rules, base) = match top.extends {
            Some(Extends::Preset(preset)) => {
                let mut rules = preset.rules();
                rules.apply_over_preset(top.rules).map_err(invalid_rules)?;
                (rules, None)
            }
            Some(Extends::File(name)) => {
                let base =
                    Self::load_chain(&config_dir(path).join(name), chain).map_err(|source| {
                        ConfigFileError::Extended {
                            path: path.to_owned(),
                            source: Box::new(source),
                        }
                    })?;
                let mut rules = base.rules.clone();
                rules.apply_over_preset(top.rules).map_err(invalid_rules)?;
                (rules, Some(base))
            }
            None => {
                let mut rules = RulesConfig::default();
                rules.apply(top.rules).map_err(invalid_rules)?;
                (rules, None)
            }
        };
        let invalid_key =
            |key, error: crate::config::InvalidPathPattern| ConfigFileError::InvalidKey {
                path: path.to_owned(),
                key,
                message: error.to_string(),
            };
        let ignore = match (top.ignore, top.ignore_from_file) {
            (Some(_), Some(_)) => {
                return Err(ConfigFileError::InvalidKey {
                    path: path.to_owned(),
                    key: TopLevelKey::IgnoreFromFile,
                    message: "cannot be used together with 'ignore'".to_owned(),
                });
            }
            (Some(lines), None) => Some((TopLevelKey::Ignore, lines)),
            (None, Some(names)) => Some((
                TopLevelKey::IgnoreFromFile,
                ignore_file_lines(path, &names)?,
            )),
            (None, None) => None,
        }
        .map(|(key, lines)| {
            IgnorePatterns::new(&config_root(path)?, &lines)
                .map_err(|error| invalid_key(key, error))
        })
        .transpose()?;
        let yaml_files = top
            .yaml_files
            .map(|lines| {
                YamlFiles::new(&lines).map_err(|error| invalid_key(TopLevelKey::YamlFiles, error))
            })
            .transpose()?;
        let inherited = base.unwrap_or_default();
        Ok(Self {
            rules,
            max_input_bytes: top.max_input_bytes.or(inherited.max_input_bytes),
            max_scan_ahead: top.max_scan_ahead.or(inherited.max_scan_ahead),
            selection: FileSelection {
                ignore: ignore.or(inherited.selection.ignore),
                yaml_files,
            },
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
                TopLevelKey::Extends => top.extends = Some(extends_of(&value).map_err(invalid)?),
                TopLevelKey::Ignore => top.ignore = Some(ignore_lines(value).map_err(invalid)?),
                TopLevelKey::IgnoreFromFile => {
                    top.ignore_from_file = Some(ignore_file_names(value).map_err(invalid)?);
                }
                TopLevelKey::YamlFiles => {
                    top.yaml_files =
                        Some(string_items(value, "file name patterns").map_err(invalid)?);
                }
                TopLevelKey::MaxInputBytes => {
                    top.max_input_bytes = Some(parse_limit(path, known, &value)?);
                }
                TopLevelKey::MaxScanAhead => {
                    top.max_scan_ahead = Some(parse_limit(path, known, &value)?);
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
                parse_limits: ParseLimits {
                    max_scan_ahead: self.max_scan_ahead.unwrap_or_default(),
                    ..ParseLimits::default()
                },
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

fn parse_limit<K: Bounds>(
    path: &Path,
    key: TopLevelKey,
    value: &Value,
) -> Result<Bounded<K>, ConfigFileError> {
    let number = value
        .as_u64()
        .ok_or_else(|| ConfigFileError::LimitNotPositive {
            path: path.to_owned(),
            key,
            found: match value {
                Value::Number(n) => n.to_string(),
                Value::String(text) => format!("'{}'", echo(text, KEY_LIMIT)),
                other => value_kind(other).to_owned(),
            },
        })?;
    usize::try_from(number)
        .ok()
        .map_or(
            Err(LimitRangeError {
                value: usize::MAX,
                min: 1,
                max: K::MAX,
            }),
            Bounded::new,
        )
        .map_err(|source| ConfigFileError::LimitOutOfRange {
            path: path.to_owned(),
            key,
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
                matches!(err, ConfigFileError::LimitNotPositive { .. }),
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
                matches!(err, ConfigFileError::LimitOutOfRange { .. }),
                "{text}: {err:?}"
            );
        }
    }

    #[test]
    fn test_max_scan_ahead_key() {
        let cfg = load_str("max-scan-ahead: 4096\n").unwrap();
        assert_eq!(cfg.max_scan_ahead, Some(MaxScanAhead::new(4096).unwrap()));
        assert_eq!(
            cfg.into_parts().0.parse_limits.max_scan_ahead,
            MaxScanAhead::new(4096).unwrap()
        );
        let unset = load_str("rules: {}\n").unwrap();
        assert_eq!(unset.max_scan_ahead, None);
        assert_eq!(
            unset.into_parts().0.parse_limits.max_scan_ahead,
            MaxScanAhead::DEFAULT
        );
        for text in ["max-scan-ahead: 4MiB\n", "max-scan-ahead: -1\n"] {
            assert!(matches!(
                load_str(text).unwrap_err(),
                ConfigFileError::LimitNotPositive {
                    key: TopLevelKey::MaxScanAhead,
                    ..
                }
            ));
        }
        assert!(matches!(
            load_str("max-scan-ahead: 0\n").unwrap_err(),
            ConfigFileError::LimitOutOfRange {
                key: TopLevelKey::MaxScanAhead,
                ..
            }
        ));
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
    fn test_tab_indented_flow_collection_is_rejected_with_its_position() {
        let err = load_str("rules: {\n\tline-length: {max: 10}\n}\n").unwrap_err();
        assert!(matches!(err, ConfigFileError::Rejected { .. }), "{err:?}");
        let message = format!("{err:?}");
        assert!(message.contains("tab"), "{message}");
        assert!(
            message.contains("line: 2") || message.contains("line 2"),
            "{message}"
        );
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
        let err = load_str("locale: en_US.UTF-8\nrules: {}\n").unwrap_err();
        assert!(
            matches!(err, ConfigFileError::UnsupportedKey { .. }),
            "{err:?}"
        );
        let message = err.to_string();
        assert!(
            message.contains("locale") && message.contains("yamllint"),
            "{message}"
        );
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
            ("extends: ''\n", "the path is empty"),
            ("extends: [default]\n", "or the path of a config file"),
            ("extends: 5\n", "or the path of a config file"),
        ] {
            let (key, message) = invalid_key(load_str(content).unwrap_err());
            assert_eq!(key, TopLevelKey::Extends);
            assert!(message.contains(needle), "{content}: {message}");
        }
    }

    fn write_file(dir: &tempfile::TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn extends_a_file_applies_its_rules_first() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir,
            "base.yaml",
            "rules:\n  line-length: {max: 100}\n  key-ordering: {enabled: true}\n  colons: disable\n",
        );
        let path = write_file(
            &dir,
            "main.yaml",
            "extends: base.yaml\nrules:\n  line-length: {allow-non-breakable-words: false}\n  colons: enable\n",
        );
        let cfg = ConfigFile::load(&path).unwrap();
        assert_eq!(cfg.rules.line_length.options.max, NonZeroUsize::new(100));
        assert!(!cfg.rules.line_length.options.allow_non_breakable_words);
        assert!(cfg.rules.key_ordering.enabled);
        assert!(cfg.rules.colons.enabled);
    }

    #[test]
    fn extends_resolves_relative_to_the_extending_file_and_nests() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            &dir,
            "shared/root.yaml",
            "extends: relaxed\nrules:\n  line-length: {max: 90}\n",
        );
        write_file(
            &dir,
            "shared/mid.yaml",
            "extends: root.yaml\nrules:\n  key-ordering: enable\n",
        );
        let path = write_file(
            &dir,
            "project/.fast-yaml.yaml",
            "extends: ../shared/mid.yaml\n",
        );
        let cfg = ConfigFile::load(&path).unwrap();
        assert_eq!(cfg.rules.line_length.options.max, NonZeroUsize::new(90));
        assert!(cfg.rules.key_ordering.enabled);
        assert!(!cfg.rules.document_start.enabled);
    }

    #[test]
    fn extends_inherits_limits_and_ignore_but_not_yaml_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        write_file(
            &dir,
            "base.yaml",
            "max-input-bytes: 4096\nmax-scan-ahead: 2048\nignore: ['vendor/']\nyaml-files: ['*.cfg']\n",
        );
        let own = write_file(
            &dir,
            "own.yaml",
            "extends: base.yaml\nmax-input-bytes: 8192\n",
        );
        let cfg = ConfigFile::load(&own).unwrap();
        assert_eq!(cfg.max_input_bytes, Some(MaxInputBytes::new(8192).unwrap()));
        assert_eq!(cfg.max_scan_ahead, Some(MaxScanAhead::new(2048).unwrap()));
        assert!(cfg.selection.yaml_files.is_none());
        let ignore = cfg.selection.ignore.unwrap();
        assert!(ignore.matches(&root.join("vendor/a.yaml"), false));
        let overriding = write_file(
            &dir,
            "over.yaml",
            "extends: base.yaml\nignore: ['other/']\n",
        );
        let ignore = ConfigFile::load(&overriding)
            .unwrap()
            .selection
            .ignore
            .unwrap();
        assert!(!ignore.matches(&root.join("vendor/a.yaml"), false));
        assert!(ignore.matches(&root.join("other/a.yaml"), false));
    }

    #[test]
    fn extends_a_missing_file_names_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "main.yaml", "extends: nowhere.yaml\n");
        let err = ConfigFile::load(&path).unwrap_err();
        let ConfigFileError::Extended { source, .. } = &err else {
            panic!("expected Extended, got {err:?}");
        };
        assert!(
            matches!(**source, ConfigFileError::Io { ref path, .. } if path.ends_with("nowhere.yaml")),
            "{source:?}"
        );
        assert!(err.to_string().contains("'extends'"));
    }

    #[test]
    fn extends_cycles_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let first = write_file(&dir, "a.yaml", "extends: b.yaml\n");
        write_file(&dir, "b.yaml", "extends: a.yaml\n");
        let mut error = ConfigFile::load(&first).unwrap_err();
        while let ConfigFileError::Extended { source, .. } = error {
            error = *source;
        }
        assert!(
            matches!(error, ConfigFileError::ExtendsCycle { .. }),
            "{error:?}"
        );
        let selfish = write_file(&dir, "self.yaml", "extends: ./self.yaml\n");
        let mut error = ConfigFile::load(&selfish).unwrap_err();
        while let ConfigFileError::Extended { source, .. } = error {
            error = *source;
        }
        assert!(
            matches!(error, ConfigFileError::ExtendsCycle { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn extends_depth_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let last = MAX_EXTENDS_DEPTH + 1;
        for level in 0..last {
            write_file(
                &dir,
                &format!("{level}.yaml"),
                &format!("extends: {}.yaml\n", level + 1),
            );
        }
        write_file(&dir, &format!("{last}.yaml"), "rules: {}\n");
        let mut error = ConfigFile::load(&dir.path().join("0.yaml")).unwrap_err();
        while let ConfigFileError::Extended { source, .. } = error {
            error = *source;
        }
        assert!(
            matches!(error, ConfigFileError::ExtendsTooDeep { .. }),
            "{error:?}"
        );
        let within = MAX_EXTENDS_DEPTH - 1;
        write_file(&dir, &format!("{within}.yaml"), "rules: {}\n");
        assert!(ConfigFile::load(&dir.path().join("0.yaml")).is_ok());
    }

    #[test]
    fn ignore_from_file_reads_patterns_next_to_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        write_file(&dir, ".yamlignore", "vendor/\n# comment\n*.tmp.yaml\n");
        write_file(&dir, "more.txt", "build/\n");
        for content in [
            "ignore-from-file: .yamlignore\n",
            "ignore-from-file: [.yamlignore, more.txt]\n",
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
        let both = load_in(&dir, "ignore-from-file: [.yamlignore, more.txt]\n")
            .unwrap()
            .selection
            .ignore
            .unwrap();
        assert!(both.matches(&root.join("build/a.yaml"), false));
    }

    #[test]
    fn ignore_from_file_errors() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir, ".yamlignore", "vendor/\n");
        let (key, message) = invalid_key(
            load_in(&dir, "ignore: ['a']\nignore-from-file: .yamlignore\n").unwrap_err(),
        );
        assert_eq!(key, TopLevelKey::IgnoreFromFile);
        assert!(message.contains("together"), "{message}");
        let err = load_in(&dir, "ignore-from-file: missing.txt\n").unwrap_err();
        assert!(matches!(err, ConfigFileError::Io { .. }), "{err:?}");
        let (key, _) = invalid_key(load_in(&dir, "ignore-from-file: 5\n").unwrap_err());
        assert_eq!(key, TopLevelKey::IgnoreFromFile);
        write_file(
            &dir,
            "huge.txt",
            &"a\n".repeat(MAX_IGNORE_FILE_BYTES / 2 + 1),
        );
        let (key, message) =
            invalid_key(load_in(&dir, "ignore-from-file: huge.txt\n").unwrap_err());
        assert_eq!(key, TopLevelKey::IgnoreFromFile);
        assert!(message.contains("larger than"), "{message}");
    }

    #[test]
    fn extended_ignore_from_file_is_inherited() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        write_file(&dir, "shared/.yamlignore", "vendor/\n");
        write_file(&dir, "shared/base.yaml", "ignore-from-file: .yamlignore\n");
        let path = write_file(&dir, "main.yaml", "extends: shared/base.yaml\n");
        let ignore = ConfigFile::load(&path).unwrap().selection.ignore.unwrap();
        assert!(ignore.matches(&root.join("shared/vendor/a.yaml"), false));
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
            assert_ne!(message, "");
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
