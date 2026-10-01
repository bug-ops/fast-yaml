//! Typed configuration for every built-in rule, generated from one rule table.

use std::cell::OnceCell;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_norway::{Mapping, Value};

use crate::DiagnosticCode;
use crate::ParseSeverityError;
use crate::Severity;
use crate::config::ignore_source::{ListError, ignore_file_lines, ignore_file_names, ignore_lines};
use crate::config::{CanonicalPath, IgnorePatterns, InvalidPathPattern, Preset};
use crate::echo::{KEY_LIMIT, MESSAGE_LIMIT, echo};
use crate::rules::NON_STANDARD_BOOLS;
use crate::rules::{
    BracesRule, BracketsRule, ColonsRule, CommasRule, CommentsIndentationRule, CommentsRule,
    DocumentEndRule, DocumentStartRule, DuplicateKeysRule, EmptyLinesRule, EmptyValuesRule,
    FloatValuesRule, HyphensRule, IndentationRule, InvalidAnchorsRule, KeyOrderingRule,
    LineLengthRule, LintDirectiveRule, NewLineAtEndOfFileRule, NewLinesRule, OctalValuesRule,
    QuotedStringsRule, Rule, SetValuesRule, TrailingWhitespaceRule, TruthyRule,
};
use crate::rules::{
    ColonsOptions, CommasOptions, CommentsOptions, DocumentEndOptions, DocumentStartOptions,
    DuplicateKeysOptions, EmptyLinesOptions, EmptyValuesOptions, FloatValuesOptions,
    FlowCollectionOptions, HyphensOptions, IndentationOptions, InvalidAnchorsOptions,
    KeyOrderingOptions, LineLengthOptions, NewLinesOptions, OctalValuesOptions,
    QuotedStringsOptions, TruthyOptions,
};
use fast_yaml_core::fs::DisplayPath;

/// Options of a built-in rule.
///
/// Implementors are plain structs with one typed field per option. They must
/// round-trip through serde so that a partial configuration can be overlaid on the
/// current values.
pub trait RuleOptions:
    Serialize + DeserializeOwned + Default + Clone + fmt::Debug + PartialEq + Eq
{
    /// Option names that yamllint accepts for this rule but fast-yaml does not implement.
    const YAMLLINT_UNSUPPORTED: &'static [&'static str] = &[];

    /// Option names where `null` is a meaningful value rather than an error.
    const NULLABLE: &'static [&'static str] = &[];

    /// Lists options whose current value cannot take effect together with the others.
    ///
    /// Only conflicts on an option that the applied patch sets are reported, so a value left
    /// over from an earlier patch never blocks changing another option.
    fn conflicts(&self) -> Vec<OptionConflict> {
        Vec::new()
    }
}

/// An option whose value cannot take effect together with the other options.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OptionConflict {
    /// Name of the option to blame.
    pub key: &'static str,
    /// Why the combination is rejected.
    pub message: String,
}

/// Marker options type for rules that have no options.
///
/// Deserializing anything but an empty mapping is an error.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::NoOptions;
///
/// assert!(serde_norway::from_str::<NoOptions>("{}").is_ok());
/// assert!(serde_norway::from_str::<NoOptions>("{x: 1}").is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoOptions {}

impl RuleOptions for NoOptions {}

/// Enablement, severity override and typed options of one rule.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::Severity;
/// use fast_yaml_linter::config::{NoOptions, RuleSettings};
///
/// let mut settings = RuleSettings::<NoOptions>::default();
/// assert!(settings.enabled);
/// assert_eq!(settings.severity_or(Severity::Info), Severity::Info);
/// settings.severity = Some(Severity::Error);
/// assert_eq!(settings.severity_or(Severity::Info), Severity::Error);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSettings<O> {
    /// Whether the rule runs.
    pub enabled: bool,
    /// Replaces the rule's default severity when set.
    pub severity: Option<Severity>,
    /// Rule-specific options.
    pub options: O,
    /// Paths the rule skips, in addition to the global ignore.
    pub ignore: Option<RuleIgnore>,
    /// Whether the entry is fast-yaml's own default or was written by a config file.
    pub origin: EntryOrigin,
}

/// Gitignore-style patterns of one rule, anchored at the directory of the file that declared them.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use fast_yaml_linter::config::{CanonicalPath, RuleIgnore};
///
/// let ignore = RuleIgnore::new(Path::new("/repo"), vec!["vendor/".to_owned()]).unwrap();
/// let inside = CanonicalPath::assume_canonical("/repo/vendor/a.yaml".into());
/// let outside = CanonicalPath::assume_canonical("/repo/src/a.yaml".into());
/// assert!(ignore.matches(&inside));
/// assert!(!ignore.matches(&outside));
/// ```
#[derive(Debug, Clone)]
pub struct RuleIgnore {
    root: PathBuf,
    lines: Vec<String>,
    matcher: Arc<IgnorePatterns>,
}

impl RuleIgnore {
    /// Compiles `lines` as gitignore patterns anchored at `root`, which should be canonical.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidPathPattern`] when a line is not valid gitignore syntax or there are
    /// too many lines.
    pub fn new(root: &Path, lines: Vec<String>) -> Result<Self, InvalidPathPattern> {
        Ok(Self {
            root: root.to_owned(),
            matcher: Arc::new(IgnorePatterns::new(root, &lines)?),
            lines,
        })
    }

    /// Returns the directory the patterns are relative to.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the pattern lines, as written in the config file.
    #[must_use]
    pub fn patterns(&self) -> &[String] {
        &self.lines
    }

    /// Returns whether the rule skips `path`.
    #[must_use]
    pub fn matches(&self, path: &CanonicalPath) -> bool {
        self.matcher.matches(path.as_path(), false)
    }
}

impl PartialEq for RuleIgnore {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root && self.lines == other.lines
    }
}

impl Eq for RuleIgnore {}

/// Where a rule entry comes from, so `enable` can tell a configured entry from a default one.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::{EntryOrigin, NoOptions, RuleSettings};
///
/// assert_eq!(RuleSettings::<NoOptions>::default().origin, EntryOrigin::FyDefault);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EntryOrigin {
    /// Set by fast-yaml's built-in defaults or preset.
    #[default]
    FyDefault,
    /// Written by a config file.
    Configured,
}

impl<O: Default> Default for RuleSettings<O> {
    fn default() -> Self {
        Self {
            enabled: true,
            severity: None,
            options: O::default(),
            ignore: None,
            origin: EntryOrigin::default(),
        }
    }
}

impl<O> RuleSettings<O> {
    /// Returns the configured severity override, or `default` when none is set.
    #[must_use]
    pub fn severity_or(&self, default: Severity) -> Severity {
        self.severity.unwrap_or(default)
    }
}

/// Error returned when a string is not the code of a built-in rule.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnknownRuleError {
    /// The rejected rule name.
    pub name: String,
}

impl fmt::Display for UnknownRuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown rule '{}'", echo(&self.name, KEY_LIMIT))
    }
}

impl std::error::Error for UnknownRuleError {}

/// Errors produced while applying rule configuration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RuleConfigError {
    /// The rule name is not a built-in rule.
    #[error(transparent)]
    UnknownRule(#[from] UnknownRuleError),

    /// The rule entry has the wrong shape.
    #[error("rule '{rule}': {}", echo(.message, MESSAGE_LIMIT))]
    InvalidEntry {
        /// Rule whose entry is invalid.
        rule: RuleName,
        /// What is wrong with the entry.
        message: String,
    },

    /// An option has an unknown name or an invalid value.
    #[error(
        "rule '{rule}', option '{}': {}",
        echo(.key, KEY_LIMIT),
        echo(.message, MESSAGE_LIMIT)
    )]
    InvalidOption {
        /// Rule whose option is invalid.
        rule: RuleName,
        /// Path of the offending option.
        key: String,
        /// Why the option was rejected.
        message: String,
    },

    /// The severity is not a known severity name.
    #[error("rule '{rule}': {cause}")]
    InvalidSeverity {
        /// Rule whose severity is invalid.
        rule: RuleName,
        /// The parse failure.
        cause: ParseSeverityError,
    },

    /// The option is supported by yamllint but not implemented here.
    #[error(
        "rule '{rule}': option '{}' is supported by yamllint but not implemented by fast-yaml{}",
        echo(.key, KEY_LIMIT),
        .hint.map_or_else(String::new, |hint| format!(" ({hint})"))
    )]
    UnsupportedOption {
        /// Rule the option belongs to.
        rule: RuleName,
        /// The unsupported option.
        key: String,
        /// Extra guidance for the user.
        hint: Option<&'static str>,
    },

    /// A custom rule code is empty.
    #[error("custom rule code must not be empty")]
    EmptyRuleCode,

    /// A custom rule code collides with a built-in rule.
    #[error("custom rule code '{}' is reserved by a built-in rule", echo(.code, KEY_LIMIT))]
    ReservedRuleCode {
        /// The colliding code.
        code: String,
    },

    /// The configuration is not a mapping of rule names to entries.
    #[error("malformed rule configuration: {}", echo(.message, MESSAGE_LIMIT))]
    Malformed {
        /// What is wrong with the input.
        message: String,
    },
}

/// Code of a user-defined rule added with [`Linter::add_rule`](crate::Linter::add_rule).
///
/// Validated so that it can never shadow the settings of a built-in rule.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::CustomRuleCode;
///
/// assert!(CustomRuleCode::new("my-rule").is_ok());
/// assert!(CustomRuleCode::new("line-length").is_err());
/// assert!(CustomRuleCode::new("").is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CustomRuleCode(String);

impl CustomRuleCode {
    /// Validates a custom rule code.
    ///
    /// # Errors
    ///
    /// Returns an error when the code is empty, equals a built-in rule name or is one of the
    /// codes the linter reserves for its own diagnostics (`syntax`, `diagnostic-limit`).
    pub fn new(code: impl Into<String>) -> Result<Self, RuleConfigError> {
        let code = code.into();
        if code.is_empty() {
            return Err(RuleConfigError::EmptyRuleCode);
        }
        if RuleName::from_str(&code).is_ok()
            || [DiagnosticCode::SYNTAX, DiagnosticCode::DIAGNOSTIC_LIMIT].contains(&code.as_str())
        {
            return Err(RuleConfigError::ReservedRuleCode { code });
        }
        Ok(Self(code))
    }

    /// Returns the code as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for CustomRuleCode {
    fn borrow(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ApplyMode {
    /// A patch or the rules of a config file without a base: whatever an entry does not mention
    /// keeps its current value, except that an entry over fast-yaml's own defaults starts at
    /// yamllint's level.
    Patch,
    /// The rules of a config file over a preset or another config file; like `Patch`, and a
    /// severity or an options mapping also enables a disabled rule.
    FileOverBase,
}

impl ApplyMode {
    const fn enables_on_override(self) -> bool {
        matches!(self, Self::FileOverBase)
    }
}

/// Whether an entry that does not mention `enabled` still switches a preset-disabled rule on.
fn enables_rule(entry: &Value) -> bool {
    match entry {
        Value::Mapping(map) => !map.contains_key("enabled"),
        Value::String(text) => {
            !text.eq_ignore_ascii_case("enable") && !text.eq_ignore_ascii_case("disable")
        }
        _ => false,
    }
}

pub(super) const fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Sequence(_) => "a list",
        Value::Mapping(_) => "a mapping",
        Value::Tagged(_) => "a tagged value",
    }
}

/// Where relative paths of per-rule `ignore` and `ignore-from-file` start.
#[derive(Debug, Clone, Copy)]
pub enum IgnoreBase<'a> {
    /// There is no directory to anchor patterns at, so per-rule ignore is rejected.
    Unavailable,
    /// Patterns are anchored at, and ignore files are read relative to, this directory.
    Dir(&'a Path),
}

/// What entries are applied against: yamllint's own settings, for `enable`, and the ignore base.
struct ApplyCtx<'a> {
    yamllint: OnceCell<RulesConfig>,
    base: IgnoreBase<'a>,
}

impl<'a> ApplyCtx<'a> {
    const fn new(base: IgnoreBase<'a>) -> Self {
        Self {
            yamllint: OnceCell::new(),
            base,
        }
    }

    fn yamllint(&self) -> &RulesConfig {
        self.yamllint.get_or_init(|| Preset::Default.rules())
    }
}

/// Settings that `enable` gives a rule yamllint has no entry for: its option defaults at `error`.
fn yamllint_enabled<O: Clone>(rule: RuleName, defaults: &RuleSettings<O>) -> RuleSettings<O> {
    RuleSettings {
        enabled: true,
        // lint-directive is fast-yaml's own rule and keeps its own severity
        severity: if rule == RuleName::LintDirective {
            defaults.severity
        } else {
            Some(Severity::Error)
        },
        options: defaults.options.clone(),
        ignore: None,
        origin: EntryOrigin::Configured,
    }
}

fn apply_entry<O: RuleOptions>(
    rule: RuleName,
    settings: &mut RuleSettings<O>,
    ctx: &ApplyCtx<'_>,
    defaults: fn(&RulesConfig) -> &RuleSettings<O>,
    entry: Value,
) -> Result<(), RuleConfigError> {
    if !entry.is_null()
        && settings.origin == EntryOrigin::FyDefault
        && !matches!(&entry, Value::String(text) if text.eq_ignore_ascii_case("disable"))
    {
        // only the level: fast-yaml's own option keys (`indent-size`) must keep working
        settings.severity = yamllint_enabled(rule, defaults(ctx.yamllint())).severity;
    }
    match entry {
        Value::Null => return Ok(()),
        Value::String(text) => {
            if text.eq_ignore_ascii_case("enable") {
                // yamllint merges `enable` into an enabled base entry and replaces any other
                if settings.enabled && settings.origin == EntryOrigin::Configured {
                    settings.enabled = true;
                } else {
                    *settings = yamllint_enabled(rule, defaults(ctx.yamllint()));
                }
            } else if text.eq_ignore_ascii_case("disable") {
                settings.enabled = false;
            } else {
                settings.severity = Some(
                    text.parse()
                        .map_err(|cause| RuleConfigError::InvalidSeverity { rule, cause })?,
                );
            }
        }
        Value::Mapping(map) => apply_mapping(rule, settings, ctx.base, map)?,
        other => {
            return Err(RuleConfigError::InvalidEntry {
                rule,
                message: format!(
                    "expected a severity, 'enable', 'disable' or a mapping, got {}",
                    value_kind(&other)
                ),
            });
        }
    }
    settings.origin = EntryOrigin::Configured;
    Ok(())
}

/// The two spellings of a rule's severity key.
#[derive(Clone, Copy)]
enum SeveritySpelling {
    Severity,
    /// yamllint's name for `severity`.
    Level,
}

impl SeveritySpelling {
    const fn key(self) -> &'static str {
        match self {
            Self::Severity => "severity",
            Self::Level => "level",
        }
    }
}

fn apply_mapping<O: RuleOptions>(
    rule: RuleName,
    settings: &mut RuleSettings<O>,
    base: IgnoreBase<'_>,
    map: Mapping,
) -> Result<(), RuleConfigError> {
    let mut overlay = Mapping::new();
    let mut bool_word_keys = Vec::new();
    let mut severity_key: Option<SeveritySpelling> = None;
    let mut ignore_source: Option<IgnoreSource> = None;
    for (key, value) in map {
        let Value::String(key) = key else {
            return Err(RuleConfigError::InvalidEntry {
                rule,
                message: format!("option names must be strings, got {}", value_kind(&key)),
            });
        };
        match key.as_str() {
            "enabled" => match value {
                Value::Bool(enabled) => settings.enabled = enabled,
                other => {
                    return Err(RuleConfigError::InvalidOption {
                        rule,
                        key,
                        message: format!(
                            "expected a boolean, got {}{}",
                            value_kind(&other),
                            yaml_1_1_bool_hint(&other)
                        ),
                    });
                }
            },
            "severity" | "level" => match value {
                Value::String(text) => {
                    let spelling = if key == "level" {
                        SeveritySpelling::Level
                    } else {
                        SeveritySpelling::Severity
                    };
                    if let Some(first) = severity_key.replace(spelling) {
                        return Err(RuleConfigError::InvalidOption {
                            rule,
                            key,
                            message: format!(
                                "`{}` and `{}` are aliases and cannot both be set",
                                first.key(),
                                spelling.key()
                            ),
                        });
                    }
                    settings.severity = Some(
                        text.parse()
                            .map_err(|cause| RuleConfigError::InvalidSeverity { rule, cause })?,
                    );
                }
                other => {
                    return Err(RuleConfigError::InvalidOption {
                        rule,
                        key,
                        message: format!("expected a string, got {}", value_kind(&other)),
                    });
                }
            },
            "ignore" | "ignore-from-file" => {
                record_ignore(rule, base, &mut ignore_source, key, value)?;
            }
            _ => {
                if O::YAMLLINT_UNSUPPORTED.contains(&key.as_str()) {
                    return Err(RuleConfigError::UnsupportedOption {
                        rule,
                        key,
                        hint: None,
                    });
                }
                if value.is_null() && !O::NULLABLE.contains(&key.as_str()) {
                    return Err(RuleConfigError::InvalidOption {
                        rule,
                        key,
                        message: "null is not a valid value".to_owned(),
                    });
                }
                if is_yaml_1_1_bool_word(&value) {
                    bool_word_keys.push(key.clone());
                }
                overlay.insert(Value::String(key), value);
            }
        }
    }

    if let (Some(source), IgnoreBase::Dir(dir)) = (ignore_source, base) {
        settings.ignore = Some(source.compile(rule, dir)?);
    }
    overlay_options(rule, settings, overlay, &bool_word_keys)
}

fn record_ignore(
    rule: RuleName,
    base: IgnoreBase<'_>,
    seen: &mut Option<IgnoreSource>,
    key: String,
    value: Value,
) -> Result<(), RuleConfigError> {
    if matches!(base, IgnoreBase::Unavailable) {
        return Err(RuleConfigError::UnsupportedOption {
            rule,
            key,
            hint: Some(
                "per-rule ignore needs a config file; use the top-level 'ignore' key \
                 to skip files for every rule",
            ),
        });
    }
    seen.replace(IgnoreSource::parse(&key, value))
        .map_or(Ok(()), |first| {
            Err(RuleConfigError::InvalidOption {
                rule,
                key,
                message: format!("cannot be used together with '{}'", first.key()),
            })
        })
}

/// The value of a per-rule `ignore` or `ignore-from-file` key.
enum IgnoreSource {
    Lines(Result<Vec<String>, ListError>),
    Files(Result<Vec<String>, ListError>),
}

impl IgnoreSource {
    fn parse(key: &str, value: Value) -> Self {
        if key == "ignore" {
            Self::Lines(ignore_lines(value))
        } else {
            Self::Files(ignore_file_names(value))
        }
    }

    const fn key(&self) -> &'static str {
        match self {
            Self::Lines(_) => "ignore",
            Self::Files(_) => "ignore-from-file",
        }
    }

    fn compile(self, rule: RuleName, dir: &Path) -> Result<RuleIgnore, RuleConfigError> {
        let key = self.key();
        let invalid = |message: &dyn fmt::Display| RuleConfigError::InvalidOption {
            rule,
            key: key.to_owned(),
            message: message.to_string(),
        };
        let from_file = matches!(self, Self::Files(_));
        let lines = match self {
            Self::Lines(lines) => lines.map_err(|error| invalid(&error))?,
            Self::Files(names) => ignore_file_lines(dir, &names.map_err(|error| invalid(&error))?)
                .map_err(|error| invalid(&error))?,
        };
        let root = dir.canonicalize().map_err(|error| {
            invalid(&format_args!(
                "cannot resolve '{}': {error}",
                DisplayPath::new(dir)
            ))
        })?;
        RuleIgnore::new(&root, lines).map_err(|error| {
            // patterns read from a file are never echoed
            if from_file {
                invalid(&"the listed patterns cannot be combined")
            } else {
                invalid(&error)
            }
        })
    }
}

fn overlay_options<O: RuleOptions>(
    rule: RuleName,
    settings: &mut RuleSettings<O>,
    overlay: Mapping,
    bool_word_keys: &[String],
) -> Result<(), RuleConfigError> {
    if overlay.is_empty() {
        return Ok(());
    }

    let Value::Mapping(mut current) =
        serde_norway::to_value(&settings.options).map_err(|error| {
            RuleConfigError::InvalidEntry {
                rule,
                message: error.to_string(),
            }
        })?
    else {
        return Err(RuleConfigError::InvalidEntry {
            rule,
            message: "options do not serialize to a mapping".to_owned(),
        });
    };
    let patch_keys: Vec<String> = overlay
        .keys()
        .filter_map(|key| key.as_str().map(str::to_owned))
        .collect();
    for (key, value) in overlay {
        current.insert(key, value);
    }
    settings.options = serde_path_to_error::deserialize(Value::Mapping(current)).map_err(
        |error: serde_path_to_error::Error<serde_norway::Error>| {
            let key = error.path().to_string();
            let mut message = error.into_inner().to_string();
            if message.contains("unknown field") {
                message.push_str(" (also accepted: `enabled`, `severity`)");
            }
            if message.contains("expected a boolean") && bool_word_keys.contains(&key) {
                message.push_str("; use true or false");
            }
            RuleConfigError::InvalidOption { rule, key, message }
        },
    )?;
    match settings
        .options
        .conflicts()
        .into_iter()
        .find(|conflict| patch_keys.iter().any(|key| key == conflict.key))
    {
        Some(conflict) => Err(RuleConfigError::InvalidOption {
            rule,
            key: conflict.key.to_owned(),
            message: conflict.message,
        }),
        None => Ok(()),
    }
}

fn is_yaml_1_1_bool_word(value: &Value) -> bool {
    matches!(value, Value::String(text) if NON_STANDARD_BOOLS.contains(&text.as_str()))
}

fn yaml_1_1_bool_hint(value: &Value) -> &'static str {
    if is_yaml_1_1_bool_word(value) {
        "; use true or false"
    } else {
        ""
    }
}

#[cfg(test)]
fn entry_of<O: RuleOptions>(settings: &RuleSettings<O>) -> Value {
    let Ok(Value::Mapping(mut entry)) = serde_norway::to_value(&settings.options) else {
        return Value::Null;
    };
    entry.insert(
        Value::String("enabled".to_owned()),
        Value::Bool(settings.enabled),
    );
    if let Some(severity) = settings.severity {
        entry.insert(
            Value::String("severity".to_owned()),
            Value::String(severity.as_str().to_owned()),
        );
    }
    Value::Mapping(entry)
}

fn buffer<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Value, RuleConfigError> {
    Value::deserialize(deserializer).map_err(|error| RuleConfigError::Malformed {
        message: error.to_string(),
    })
}

macro_rules! builtin_rules {
    ($(
        ($variant:ident, $field:ident, $code:expr, $options:ty, $rule:ident, $doc:literal)
    ),+ $(,)?) => {
        /// Identifier of a built-in rule.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum RuleName {
            $(#[doc = $doc] $variant,)+
        }

        impl RuleName {
            /// Every built-in rule, in registry order.
            pub const ALL: [Self; [$(Self::$variant),+].len()] = [$(Self::$variant),+];

            /// Returns the kebab-case rule code used in diagnostics and configuration.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $code,)+
                }
            }
        }

        /// Typed settings of every built-in rule.
        ///
        /// # Examples
        ///
        /// ```
        /// use fast_yaml_linter::config::{Limit, RulesConfig};
        ///
        /// let mut rules = RulesConfig::default();
        /// let yaml = "line-length: {max: 120}\nkey-ordering: disable\ncomments: warning";
        /// rules.apply(serde_norway::Deserializer::from_str(yaml)).unwrap();
        /// assert_eq!(rules.line_length.options.max.map(|max| max.get()), Some(120));
        /// assert!(!rules.key_ordering.enabled);
        /// assert_eq!(rules.empty_lines.options.max, Limit::Max(2));
        /// ```
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct RulesConfig {
            $(#[doc = $doc] pub $field: RuleSettings<$options>,)+
        }

        impl RulesConfig {
            /// Returns whether the rule is enabled.
            #[must_use]
            pub const fn is_enabled(&self, name: RuleName) -> bool {
                match name {
                    $(RuleName::$variant => self.$field.enabled,)+
                }
            }

            /// Returns the configured severity override of a rule, if any.
            #[must_use]
            pub const fn severity(&self, name: RuleName) -> Option<Severity> {
                match name {
                    $(RuleName::$variant => self.$field.severity,)+
                }
            }

            /// Renders every rule as a configuration entry that `apply` accepts.
            #[cfg(test)]
            pub(crate) fn to_entries(&self) -> Mapping {
                let mut entries = Mapping::new();
                $(entries.insert(
                    Value::String($code.to_owned()),
                    entry_of(&self.$field),
                );)+
                entries
            }

            /// Enables or disables a rule.
            pub const fn set_enabled(&mut self, name: RuleName, enabled: bool) {
                match name {
                    $(RuleName::$variant => self.$field.enabled = enabled,)+
                }
            }

            fn apply_value(
                &mut self,
                name: RuleName,
                ctx: &ApplyCtx<'_>,
                entry: Value,
            ) -> Result<(), RuleConfigError> {
                match name {
                    $(RuleName::$variant => {
                        apply_entry(name, &mut self.$field, ctx, |rules| &rules.$field, entry)
                    })+
                }
            }

            /// Returns whether any rule has `ignore` patterns.
            #[must_use]
            pub const fn has_ignore(&self) -> bool {
                false $(|| self.$field.ignore.is_some())+
            }

            /// Returns whether the rule's own `ignore` patterns skip `path`.
            #[must_use]
            pub fn is_ignored(&self, name: RuleName, path: &CanonicalPath) -> bool {
                match name {
                    $(RuleName::$variant => {
                        self.$field.ignore.as_ref().is_some_and(|ignore| ignore.matches(path))
                    })+
                }
            }
        }

        /// Instantiates every built-in rule in registry order.
        pub fn default_rules() -> Vec<Rule> {
            vec![$(Rule::Source(Box::new($rule)),)+]
        }
    };
}

builtin_rules! {
    (DuplicateKey, duplicate_key, DiagnosticCode::DUPLICATE_KEY, DuplicateKeysOptions, DuplicateKeysRule, "Duplicate mapping keys."),
    (LineLength, line_length, DiagnosticCode::LINE_LENGTH, LineLengthOptions, LineLengthRule, "Maximum line length."),
    (TrailingWhitespace, trailing_whitespace, DiagnosticCode::TRAILING_WHITESPACE, NoOptions, TrailingWhitespaceRule, "Trailing whitespace at line ends."),
    (DocumentStart, document_start, DiagnosticCode::DOCUMENT_START, DocumentStartOptions, DocumentStartRule, "Document start marker `---`."),
    (DocumentEnd, document_end, DiagnosticCode::DOCUMENT_END, DocumentEndOptions, DocumentEndRule, "Document end marker `...`."),
    (EmptyValues, empty_values, DiagnosticCode::EMPTY_VALUES, EmptyValuesOptions, EmptyValuesRule, "Empty mapping and sequence values."),
    (NewLineAtEndOfFile, new_line_at_end_of_file, DiagnosticCode::NEW_LINE_AT_END_OF_FILE, NoOptions, NewLineAtEndOfFileRule, "Newline at the end of the file."),
    (Braces, braces, DiagnosticCode::BRACES, FlowCollectionOptions, BracesRule, "Flow mapping braces `{}`."),
    (Brackets, brackets, DiagnosticCode::BRACKETS, FlowCollectionOptions, BracketsRule, "Flow sequence brackets `[]`."),
    (Colons, colons, DiagnosticCode::COLONS, ColonsOptions, ColonsRule, "Spacing around colons."),
    (Commas, commas, DiagnosticCode::COMMAS, CommasOptions, CommasRule, "Spacing around commas."),
    (Hyphens, hyphens, DiagnosticCode::HYPHENS, HyphensOptions, HyphensRule, "Spacing after sequence hyphens."),
    (Comments, comments, DiagnosticCode::COMMENTS, CommentsOptions, CommentsRule, "Comment formatting."),
    (CommentsIndentation, comments_indentation, DiagnosticCode::COMMENTS_INDENTATION, NoOptions, CommentsIndentationRule, "Comment indentation."),
    (EmptyLines, empty_lines, DiagnosticCode::EMPTY_LINES, EmptyLinesOptions, EmptyLinesRule, "Consecutive empty lines."),
    (NewLines, new_lines, DiagnosticCode::NEW_LINES, NewLinesOptions, NewLinesRule, "Line ending style."),
    (OctalValues, octal_values, DiagnosticCode::OCTAL_VALUES, OctalValuesOptions, OctalValuesRule, "Octal number forms."),
    (Truthy, truthy, DiagnosticCode::TRUTHY, TruthyOptions, TruthyRule, "Truthy value spellings."),
    (QuotedStrings, quoted_strings, DiagnosticCode::QUOTED_STRINGS, QuotedStringsOptions, QuotedStringsRule, "Quoted string style."),
    (KeyOrdering, key_ordering, DiagnosticCode::KEY_ORDERING, KeyOrderingOptions, KeyOrderingRule, "Alphabetical key ordering."),
    (FloatValues, float_values, DiagnosticCode::FLOAT_VALUES, FloatValuesOptions, FloatValuesRule, "Float number forms."),
    (InvalidAnchor, invalid_anchor, DiagnosticCode::INVALID_ANCHOR, InvalidAnchorsOptions, InvalidAnchorsRule, "Invalid anchor names."),
    (Indentation, indentation, DiagnosticCode::INDENTATION, IndentationOptions, IndentationRule, "Indentation size."),
    (SetValues, set_values, DiagnosticCode::SET_VALUES, NoOptions, SetValuesRule, "`!!set` members without values."),
    (LintDirective, lint_directive, DiagnosticCode::LINT_DIRECTIVE, NoOptions, LintDirectiveRule, "Invalid inline lint directives."),
}

impl fmt::Display for RuleName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RuleName {
    type Err = UnknownRuleError;

    /// Parses a rule code such as `line-length`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::config::RuleName;
    ///
    /// assert_eq!("braces".parse::<RuleName>(), Ok(RuleName::Braces));
    /// assert!("nope".parse::<RuleName>().is_err());
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|name| name.as_str() == s)
            .ok_or_else(|| UnknownRuleError { name: s.to_owned() })
    }
}

impl RuleName {
    /// Rules whose yamllint name differs from the fast-yaml name.
    const YAMLLINT_NAMES: [(&'static str, Self); 3] = [
        ("key-duplicates", Self::DuplicateKey),
        ("trailing-spaces", Self::TrailingWhitespace),
        ("anchors", Self::InvalidAnchor),
    ];

    /// Parses a rule name as written in a configuration: a fast-yaml code or the name yamllint
    /// uses for the same rule (`key-duplicates`, `trailing-spaces`, `anchors`).
    ///
    /// Diagnostics always carry the fast-yaml code, which is what [`FromStr`] accepts.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownRuleError`] when the name is neither.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::config::RuleName;
    ///
    /// assert_eq!(RuleName::from_config_key("trailing-spaces"), Ok(RuleName::TrailingWhitespace));
    /// assert_eq!(RuleName::from_config_key("braces"), Ok(RuleName::Braces));
    /// assert!("trailing-spaces".parse::<RuleName>().is_err());
    /// ```
    pub fn from_config_key(name: &str) -> Result<Self, UnknownRuleError> {
        Self::YAMLLINT_NAMES
            .into_iter()
            .find_map(|(yamllint, rule)| (yamllint == name).then_some(rule))
            .map_or_else(|| name.parse(), Ok)
    }
}

impl RulesConfig {
    /// Applies a mapping of rule names to entries on top of the current settings.
    ///
    /// Each entry is `null` (no change), a severity name, `enable` or `disable`, or a
    /// mapping with `enabled`, `severity` and rule options. Anything an entry does not
    /// mention keeps its current value. The input is buffered into a YAML value first so
    /// that every source (files, Python, JSON) is validated identically. On error `self`
    /// is left unchanged.
    ///
    /// # Errors
    ///
    /// Returns the first problem found: an unknown rule, an unknown or mistyped option,
    /// an invalid severity, or an option yamllint has but fast-yaml does not implement.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::config::RulesConfig;
    ///
    /// let mut rules = RulesConfig::default();
    /// let yaml = "quoted-strings: {quote-type: singel}";
    /// let err = rules.apply(serde_norway::Deserializer::from_str(yaml)).unwrap_err();
    /// assert!(err.to_string().contains("quoted-strings"));
    /// assert!(err.to_string().contains("quote-type"));
    /// ```
    pub fn apply<'de, D: Deserializer<'de>>(
        &mut self,
        deserializer: D,
    ) -> Result<(), RuleConfigError> {
        self.apply_entries(
            buffer(deserializer)?,
            ApplyMode::Patch,
            IgnoreBase::Unavailable,
        )
    }

    /// Like [`RulesConfig::apply_at`], anchored at the working directory, or like
    /// [`RulesConfig::apply`] when the working directory cannot be read.
    ///
    /// # Errors
    ///
    /// As [`RulesConfig::apply_at`].
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::config::RulesConfig;
    ///
    /// let mut rules = RulesConfig::default();
    /// let yaml = "line-length: {ignore: 'vendor/'}";
    /// rules.apply_in_cwd(serde_norway::Deserializer::from_str(yaml)).unwrap();
    /// assert!(rules.has_ignore());
    /// ```
    pub fn apply_in_cwd<'de, D: Deserializer<'de>>(
        &mut self,
        deserializer: D,
    ) -> Result<(), RuleConfigError> {
        match std::env::current_dir() {
            Ok(dir) => self.apply_at(deserializer, &dir),
            Err(_) => self.apply(deserializer),
        }
    }

    /// Like [`RulesConfig::apply`], but accepts per-rule `ignore` and `ignore-from-file`.
    ///
    /// Patterns are anchored at `base_dir`, and ignore files are read relative to it. A rule
    /// skips the files its patterns match when the source is linted with
    /// [`Linter::lint_file`](crate::Linter::lint_file).
    ///
    /// # Errors
    ///
    /// As [`RulesConfig::apply`], and when `base_dir` cannot be resolved, an ignore file cannot
    /// be read, or a pattern is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use fast_yaml_linter::config::RulesConfig;
    ///
    /// let mut rules = RulesConfig::default();
    /// let yaml = "line-length: {ignore: 'vendor/'}";
    /// let base = std::env::temp_dir();
    /// rules.apply_at(serde_norway::Deserializer::from_str(yaml), &base).unwrap();
    /// assert!(rules.line_length.ignore.is_some());
    /// ```
    pub fn apply_at<'de, D: Deserializer<'de>>(
        &mut self,
        deserializer: D,
        base_dir: &Path,
    ) -> Result<(), RuleConfigError> {
        self.apply_entries(
            buffer(deserializer)?,
            ApplyMode::Patch,
            IgnoreBase::Dir(base_dir),
        )
    }

    /// Applies the `rules:` section of a config that extends a preset.
    ///
    /// Like [`RulesConfig::apply_at`], except that a severity name or a mapping without an
    /// `enabled` key also turns a rule on, as in yamllint where such an entry replaces the
    /// preset's disabled rule.
    pub(super) fn apply_over_preset(
        &mut self,
        value: Value,
        base: IgnoreBase<'_>,
    ) -> Result<(), RuleConfigError> {
        self.apply_entries(value, ApplyMode::FileOverBase, base)
    }

    pub(super) fn apply_entries(
        &mut self,
        value: Value,
        mode: ApplyMode,
        base: IgnoreBase<'_>,
    ) -> Result<(), RuleConfigError> {
        let entries = match value {
            Value::Null => return Ok(()),
            Value::Mapping(entries) => entries,
            other => {
                return Err(RuleConfigError::Malformed {
                    message: format!(
                        "expected a mapping of rule names, got {}",
                        value_kind(&other)
                    ),
                });
            }
        };

        let ctx = ApplyCtx::new(base);
        let mut next = self.clone();
        let mut seen = Vec::new();
        for (key, entry) in entries {
            let Value::String(name) = key else {
                return Err(RuleConfigError::Malformed {
                    message: format!("rule names must be strings, got {}", value_kind(&key)),
                });
            };
            let rule = RuleName::from_config_key(&name).map_err(RuleConfigError::UnknownRule)?;
            if seen.contains(&rule) {
                return Err(RuleConfigError::InvalidEntry {
                    rule,
                    message: format!(
                        "'{}' names the same rule as an earlier entry",
                        echo(&name, KEY_LIMIT)
                    ),
                });
            }
            seen.push(rule);
            let enables = mode.enables_on_override() && enables_rule(&entry);
            next.apply_value(rule, &ctx, entry)?;
            if enables {
                next.set_enabled(rule, true);
            }
        }
        *self = next;
        Ok(())
    }

    /// Applies one rule's entry on top of the current settings.
    ///
    /// Accepts the same entry forms as [`RulesConfig::apply`]. On error `self` is left
    /// unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error when the entry is invalid for the rule.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Severity;
    /// use fast_yaml_linter::config::{RuleName, RulesConfig};
    ///
    /// let mut rules = RulesConfig::default();
    /// rules
    ///     .apply_rule(RuleName::Colons, serde_norway::Deserializer::from_str("error"))
    ///     .unwrap();
    /// assert_eq!(rules.colons.severity, Some(Severity::Error));
    /// ```
    pub fn apply_rule<'de, D: Deserializer<'de>>(
        &mut self,
        name: RuleName,
        deserializer: D,
    ) -> Result<(), RuleConfigError> {
        self.apply_rule_over(name, deserializer, IgnoreBase::Unavailable)
    }

    /// Like [`RulesConfig::apply_rule`], but accepts `ignore` and `ignore-from-file` in the
    /// entry, anchored at `base_dir` as in [`RulesConfig::apply_at`].
    ///
    /// # Errors
    ///
    /// As [`RulesConfig::apply_rule`], and when `base_dir` cannot be resolved, an ignore file
    /// cannot be read, or a pattern is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::config::{RuleName, RulesConfig};
    ///
    /// let mut rules = RulesConfig::default();
    /// let yaml = "{ignore: 'vendor/'}";
    /// let base = std::env::temp_dir();
    /// rules
    ///     .apply_rule_at(RuleName::Colons, serde_norway::Deserializer::from_str(yaml), &base)
    ///     .unwrap();
    /// assert!(rules.has_ignore());
    /// ```
    pub fn apply_rule_at<'de, D: Deserializer<'de>>(
        &mut self,
        name: RuleName,
        deserializer: D,
        base_dir: &Path,
    ) -> Result<(), RuleConfigError> {
        self.apply_rule_over(name, deserializer, IgnoreBase::Dir(base_dir))
    }

    /// Like [`RulesConfig::apply_rule_at`], anchored at the working directory, or like
    /// [`RulesConfig::apply_rule`] when the working directory cannot be read.
    ///
    /// # Errors
    ///
    /// As [`RulesConfig::apply_rule_at`].
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::config::{RuleName, RulesConfig};
    ///
    /// let mut rules = RulesConfig::default();
    /// let yaml = "{ignore: 'vendor/'}";
    /// rules
    ///     .apply_rule_in_cwd(RuleName::Colons, serde_norway::Deserializer::from_str(yaml))
    ///     .unwrap();
    /// assert!(rules.has_ignore());
    /// ```
    pub fn apply_rule_in_cwd<'de, D: Deserializer<'de>>(
        &mut self,
        name: RuleName,
        deserializer: D,
    ) -> Result<(), RuleConfigError> {
        match std::env::current_dir() {
            Ok(dir) => self.apply_rule_at(name, deserializer, &dir),
            Err(_) => self.apply_rule(name, deserializer),
        }
    }

    fn apply_rule_over<'de, D: Deserializer<'de>>(
        &mut self,
        name: RuleName,
        deserializer: D,
        base: IgnoreBase<'_>,
    ) -> Result<(), RuleConfigError> {
        let entry = buffer(deserializer)?;
        let mut next = self.clone();
        next.apply_value(name, &ApplyCtx::new(base), entry)?;
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleRegistry;

    fn apply(rules: &mut RulesConfig, yaml: &str) -> Result<(), RuleConfigError> {
        rules.apply(serde_norway::Deserializer::from_str(yaml))
    }

    fn applied(yaml: &str) -> RulesConfig {
        let mut rules = RulesConfig::default();
        apply(&mut rules, yaml).unwrap();
        rules
    }

    fn error_of(yaml: &str) -> String {
        apply(&mut RulesConfig::default(), yaml)
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn registry_and_rule_names_are_a_bijection() {
        let registry = RuleRegistry::with_default_rules();
        let codes: Vec<&str> = registry
            .rules()
            .iter()
            .map(|rule| rule.info().id().as_str())
            .collect();
        let names: Vec<&str> = RuleName::ALL.iter().map(|name| name.as_str()).collect();
        assert_eq!(codes, names);
        assert_eq!(RuleName::ALL.len(), 25);
    }

    #[test]
    fn rule_name_round_trips_through_str() {
        for name in RuleName::ALL {
            assert_eq!(name.as_str().parse::<RuleName>(), Ok(name));
            assert_eq!(name.to_string(), name.as_str());
        }
        assert!("undefined-alias".parse::<RuleName>().is_err());
    }

    #[test]
    fn unknown_rule_is_an_error() {
        assert_eq!(
            error_of("no-such-rule: error"),
            "unknown rule 'no-such-rule'"
        );
    }

    #[test]
    fn enable_and_disable_shorthands_are_case_insensitive() {
        let rules = applied("braces: disable\nbrackets: DISABLE\ncolons: Enable");
        assert!(!rules.braces.enabled);
        assert!(!rules.brackets.enabled);
        assert!(rules.colons.enabled);
        assert_eq!(rules.braces.severity, None);
    }

    #[test]
    fn severity_string_sets_severity_only() {
        let mut rules = applied("braces: disable");
        apply(&mut rules, "braces: error").unwrap();
        assert_eq!(rules.braces.severity, Some(Severity::Error));
        assert!(!rules.braces.enabled);
    }

    #[test]
    fn invalid_severity_names_rule() {
        let message = error_of("braces: loud");
        assert!(message.contains("braces"), "{message}");
        assert!(message.contains("loud"), "{message}");
        let message = error_of("braces: {severity: loud}");
        assert!(message.contains("loud"), "{message}");
    }

    #[test]
    fn null_entry_is_a_no_op_and_non_entry_types_fail() {
        let rules = applied("braces: ~");
        assert_eq!(rules, RulesConfig::default());
        assert!(error_of("braces: 5").contains("got a number"));
        assert!(error_of("braces: [a]").contains("got a list"));
    }

    #[test]
    fn enabled_must_be_boolean() {
        let message = error_of("braces: {enabled: 'false'}");
        assert!(message.contains("enabled"), "{message}");
        assert!(message.contains("boolean"), "{message}");
        assert!(error_of("braces: {enabled: ~}").contains("enabled"));
    }

    #[test]
    fn list_index_path_is_reported() {
        let message = error_of("truthy: {allowed-values: ['true', maybe]}");
        assert!(message.contains("allowed-values[1]"), "{message}");
    }

    #[test]
    fn patch_keeps_untouched_options() {
        let mut rules = applied("line-length: {max: 120}");
        apply(&mut rules, "line-length: error").unwrap();
        assert_eq!(
            rules
                .line_length
                .options
                .max
                .map(std::num::NonZeroUsize::get),
            Some(120)
        );
        apply(&mut rules, "line-length: {severity: info}").unwrap();
        assert_eq!(
            rules
                .line_length
                .options
                .max
                .map(std::num::NonZeroUsize::get),
            Some(120)
        );
    }

    #[test]
    fn disabled_rule_stays_disabled_after_severity_patch() {
        let mut rules = applied("braces: disable");
        apply(&mut rules, "braces: {severity: error}").unwrap();
        assert!(!rules.braces.enabled);
    }

    #[test]
    fn options_patch_merges_per_key() {
        let mut rules = applied("empty-lines: {max: 5}");
        apply(&mut rules, "empty-lines: {max-start: 1}").unwrap();
        assert_eq!(rules.empty_lines.options.max, crate::config::Limit::Max(5));
        assert_eq!(
            rules.empty_lines.options.max_start,
            crate::config::Limit::Max(1)
        );
    }

    #[test]
    fn failed_apply_leaves_config_unchanged() {
        let mut rules = RulesConfig::default();
        let result = apply(&mut rules, "braces: disable\nline-length: {max: lots}");
        assert!(result.is_err());
        assert_eq!(rules, RulesConfig::default());

        let result = rules.apply_rule(
            RuleName::Braces,
            serde_norway::Deserializer::from_str("{enabled: false, nope: 1}"),
        );
        assert!(result.is_err());
        assert!(rules.braces.enabled);
    }

    #[test]
    fn apply_rule_targets_one_rule() {
        let mut rules = RulesConfig::default();
        rules
            .apply_rule(
                RuleName::LineLength,
                serde_norway::Deserializer::from_str("{max: 30, severity: error}"),
            )
            .unwrap();
        assert_eq!(
            rules
                .line_length
                .options
                .max
                .map(std::num::NonZeroUsize::get),
            Some(30)
        );
        assert_eq!(rules.line_length.severity, Some(Severity::Error));
    }

    #[test]
    fn non_mapping_root_is_malformed() {
        let message = error_of("[a, b]");
        assert!(message.contains("malformed"), "{message}");
        assert!(apply(&mut RulesConfig::default(), "").is_ok());
    }

    #[test]
    fn check_multi_line_strings_is_a_supported_option() {
        let rules = applied("indentation: {check-multi-line-strings: true}");
        assert!(rules.indentation.options.check_multi_line_strings);
        assert!(error_of("indentation: {check-multi-line-string: true}").contains("unknown field"));
    }

    #[test]
    fn indentation_spaces_and_sequences_are_typed() {
        use crate::config::{IndentSequences, IndentSize, IndentSpaces};

        let mut rules = RulesConfig::default();
        assert_eq!(rules.indentation.options.spaces, None);
        apply(
            &mut rules,
            "indentation: {spaces: 4, indent-sequences: whatever}",
        )
        .unwrap();
        assert_eq!(
            rules.indentation.options.spaces,
            Some(IndentSpaces::Fixed(IndentSize::try_from(4u64).unwrap()))
        );
        assert_eq!(
            rules.indentation.options.indent_sequences,
            IndentSequences::Whatever
        );

        apply(&mut rules, "indentation: {spaces: consistent}").unwrap();
        assert_eq!(
            rules.indentation.options.spaces,
            Some(IndentSpaces::Consistent)
        );
        assert!(error_of("indentation: {spaces: 0}").contains("spaces"));
        assert!(error_of("indentation: {indent-sequences: maybe}").contains("indent-sequences"));
    }

    #[test]
    fn implemented_yamllint_options_are_accepted() {
        let mut rules = RulesConfig::default();
        apply(
            &mut rules,
            "line-length: {allow-non-breakable-words: false, allow-non-breakable-inline-mappings: true}\n\
             quoted-strings: {allow-quoted-quotes: true, check-keys: true}\n\
             duplicate-key: {forbid-duplicated-merge-keys: false}\n\
             key-ordering: {ignored-keys: ['^name$']}\n\
             invalid-anchor: {forbid-undeclared-aliases: true, forbid-duplicated-anchors: false, forbid-unused-anchors: true}",
        )
        .unwrap();
        assert!(rules.key_ordering.options.ignored_keys.is_match("name"));
        assert!(!rules.invalid_anchor.options.forbid_duplicated_anchors);
        assert!(rules.invalid_anchor.options.forbid_unused_anchors);
        assert!(!rules.line_length.options.allow_non_breakable_words);
        assert!(
            rules
                .line_length
                .options
                .allow_non_breakable_inline_mappings
        );
        assert!(rules.quoted_strings.options.allow_quoted_quotes);
        assert!(rules.quoted_strings.options.check_keys);
        assert!(!rules.duplicate_key.options.forbid_duplicated_merge_keys);
    }

    #[test]
    fn whole_rules_config_round_trips_through_entries() {
        let mut original = RulesConfig::default();
        apply(
            &mut original,
            "braces: {forbid: non-empty, min-spaces-inside-empty: 1, severity: hint}\n\
             colons: {max-spaces-before: -1, enabled: false}\n\
             quoted-strings: {quote-type: double, required: never, severity: error}\n\
             truthy: {allowed-values: [yes, 'Off'], check-keys: true}\n\
             line-length: {max: ~, severity: warning}\n\
             document-end: {present: true}\n\
             indentation: {indent-size: 8, enabled: false}",
        )
        .unwrap();
        for name in RuleName::ALL {
            let mut entry = Mapping::new();
            entry.insert(
                Value::String("severity".to_owned()),
                Value::String("info".to_owned()),
            );
            entry.insert(
                Value::String("enabled".to_owned()),
                Value::Bool(name.as_str().len() % 2 == 0),
            );
            if original.severity(name).is_none() {
                original
                    .apply_value(
                        name,
                        &ApplyCtx::new(IgnoreBase::Unavailable),
                        Value::Mapping(entry),
                    )
                    .unwrap();
            }
        }
        let mut restored = RulesConfig::default();
        restored
            .apply(Value::Mapping(original.to_entries()))
            .unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn level_is_an_alias_of_severity() {
        let mut rules = RulesConfig::default();
        apply(&mut rules, "line-length: {level: warning}").unwrap();
        assert_eq!(rules.line_length.severity, Some(Severity::Warning));
        apply(&mut rules, "braces: {level: error}").unwrap();
        assert_eq!(rules.braces.severity, Some(Severity::Error));
    }

    #[test]
    fn level_and_severity_together_are_rejected() {
        for yaml in [
            "line-length: {level: warning, severity: error}",
            "line-length: {severity: error, level: warning}",
        ] {
            let message = error_of(yaml);
            assert!(
                message.contains("aliases") && message.contains("level"),
                "{message}"
            );
        }
    }

    #[test]
    fn invalid_level_value_names_the_severity_error() {
        assert!(error_of("braces: {level: loud}").contains("loud"));
        assert!(error_of("braces: {level: 1}").contains("expected a string"));
    }

    #[test]
    fn unknown_field_hint_lists_enabled_and_severity() {
        let message = error_of("colons: {nope: 1}");
        assert!(
            message.contains("`enabled`") && message.contains("`severity`"),
            "{message}"
        );
    }

    #[test]
    fn yaml_1_1_bool_words_get_a_hint() {
        for yaml in [
            "comments: {require-starting-space: no}",
            "comments: {ignore-shebangs: Yes}",
            "colons: {enabled: off}",
        ] {
            let message = error_of(yaml);
            assert!(message.contains("use true or false"), "{yaml}: {message}");
        }
        assert!(!error_of("comments: {require-starting-space: maybe}").contains("use true"));
    }

    #[test]
    fn forbid_no_is_an_alias_of_false() {
        let rules = applied("braces: {forbid: 'no'}\nbrackets: {forbid: no}");
        assert_eq!(rules.braces.options.forbid, crate::rules::Forbid::No);
        assert_eq!(rules.brackets.options.forbid, crate::rules::Forbid::No);
    }

    #[test]
    fn hostile_names_are_bounded_and_escaped() {
        let long = "x".repeat(100_000);
        assert!(error_of(&format!("{long}: error")).len() < 1_000);
        assert!(error_of(&format!("braces: {{{long}: 1}}")).len() < 1_000);
        assert!(error_of(&format!("braces: {long}")).len() < 1_000);
        let esc = error_of("\"a\\e]0;pwned\\ab\": 1");
        assert!(!esc.contains('\u{1b}') && !esc.contains('\u{7}'), "{esc:?}");
        let esc = error_of("braces: {\"a\\e[2J\": 1}");
        assert!(!esc.contains('\u{1b}'), "{esc:?}");
        let esc = error_of("braces: \"a\\e[2J\"");
        assert!(!esc.contains('\u{1b}'), "{esc:?}");
    }

    #[test]
    fn severity_for_resolves_builtin_rules() {
        use crate::LintConfig;
        use crate::rules::RuleId;
        let mut config = LintConfig::default();
        apply(&mut config.rules, "braces: error").unwrap();
        assert_eq!(
            config.severity_for(RuleId::BuiltIn(RuleName::Braces), Severity::Hint),
            Severity::Error
        );
        assert_eq!(
            config.severity_for(RuleId::BuiltIn(RuleName::Colons), Severity::Hint),
            Severity::Hint
        );
    }

    #[test]
    fn custom_rule_code_validation() {
        assert_eq!(
            CustomRuleCode::new("").unwrap_err(),
            RuleConfigError::EmptyRuleCode
        );
        assert!(matches!(
            CustomRuleCode::new("braces"),
            Err(RuleConfigError::ReservedRuleCode { .. })
        ));
        assert_eq!(CustomRuleCode::new("mine").unwrap().as_str(), "mine");
        for reserved in ["syntax", "diagnostic-limit"] {
            assert!(matches!(
                CustomRuleCode::new(reserved),
                Err(RuleConfigError::ReservedRuleCode { .. })
            ));
        }
    }

    #[test]
    fn unknown_rule_names_no_yamllint_alias() {
        assert_eq!(
            error_of("key-duplicate: enable"),
            "unknown rule 'key-duplicate'"
        );
    }

    #[test]
    fn per_rule_ignore_is_explicitly_unsupported() {
        for key in ["ignore", "ignore-from-file"] {
            let message = error_of(&format!("braces: {{{key}: vendor/}}"));
            assert!(
                message.contains("braces") && message.contains(key),
                "{message}"
            );
            assert!(message.contains("supported by yamllint"), "{message}");
            assert!(message.contains("top-level 'ignore'"), "{message}");
        }
    }

    #[test]
    fn apply_rule_at_accepts_ignore_and_apply_rule_does_not() {
        let entry = || serde_norway::Deserializer::from_str("{ignore: 'gen/'}");
        let mut rules = RulesConfig::default();
        assert!(rules.apply_rule(RuleName::Colons, entry()).is_err());
        assert!(!rules.has_ignore());

        let base = std::env::temp_dir().canonicalize().unwrap();
        rules
            .apply_rule_at(RuleName::Colons, entry(), &base)
            .unwrap();
        assert!(rules.has_ignore());
        let skipped = CanonicalPath::assume_canonical(base.join("gen/a.yaml"));
        let kept = CanonicalPath::assume_canonical(base.join("src/a.yaml"));
        assert!(rules.is_ignored(RuleName::Colons, &skipped));
        assert!(!rules.is_ignored(RuleName::Colons, &kept));
    }

    fn over_preset(rules: &mut RulesConfig, yaml: &str) {
        let value = serde_norway::from_str(yaml).unwrap();
        rules
            .apply_over_preset(value, IgnoreBase::Unavailable)
            .unwrap();
    }

    #[test]
    fn preset_override_enables_on_mapping_and_severity_but_not_on_disable() {
        let mut rules = RulesConfig::default();
        for name in RuleName::ALL {
            rules.set_enabled(name, false);
        }
        over_preset(
            &mut rules,
            "braces: {max-spaces-inside: 1}\nbrackets: warning\ncolons: {}\n\
             commas: enable\nhyphens: disable\nindentation: {enabled: false, indent-size: 4}\n\
             line-length: ~",
        );
        assert!(rules.braces.enabled);
        assert!(rules.brackets.enabled);
        assert_eq!(rules.brackets.severity, Some(Severity::Warning));
        assert!(rules.colons.enabled);
        assert!(rules.commas.enabled);
        assert!(!rules.hyphens.enabled);
        assert!(!rules.indentation.enabled);
        assert_eq!(rules.indentation.options.indent_size().get(), 4);
        assert!(!rules.line_length.enabled);
    }

    #[test]
    fn plain_apply_keeps_a_disabled_rule_disabled() {
        let mut rules = applied("braces: disable");
        apply(&mut rules, "braces: {max-spaces-inside: 1}").unwrap();
        assert!(!rules.braces.enabled);
    }

    #[test]
    fn failed_preset_override_leaves_config_unchanged() {
        let mut rules = RulesConfig::default();
        rules.set_enabled(RuleName::Braces, false);
        let before = rules.clone();
        let value = serde_norway::from_str("braces: {max-spaces-inside: 1}\ncolons: loud").unwrap();
        assert!(
            rules
                .apply_over_preset(value, IgnoreBase::Unavailable)
                .is_err()
        );
        assert_eq!(rules, before);
    }

    #[test]
    fn default_rules_follow_rule_name_order() {
        assert_eq!(default_rules().len(), RuleName::ALL.len());
    }
}
