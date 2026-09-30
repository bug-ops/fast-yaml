//! Typed configuration for every built-in rule, generated from one rule table.

use std::fmt;
use std::str::FromStr;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_norway::{Mapping, Value};

use crate::DiagnosticCode;
use crate::ParseSeverityError;
use crate::Severity;
use crate::echo::{KEY_LIMIT, MESSAGE_LIMIT, echo};
use crate::rules::NON_STANDARD_BOOLS;
use crate::rules::{
    BracesRule, BracketsRule, ColonsRule, CommasRule, CommentsIndentationRule, CommentsRule,
    DocumentEndRule, DocumentStartRule, DuplicateKeysRule, EmptyLinesRule, EmptyValuesRule,
    FloatValuesRule, HyphensRule, IndentationRule, InvalidAnchorsRule, KeyOrderingRule,
    LineLengthRule, LintDirectiveRule, LintRule, NewLineAtEndOfFileRule, NewLinesRule,
    OctalValuesRule, QuotedStringsRule, TrailingWhitespaceRule, TruthyRule,
};
use crate::rules::{
    ColonsOptions, CommasOptions, CommentsOptions, DocumentEndOptions, DocumentStartOptions,
    DuplicateKeysOptions, EmptyLinesOptions, EmptyValuesOptions, FloatValuesOptions,
    FlowCollectionOptions, HyphensOptions, IndentationOptions, InvalidAnchorsOptions,
    KeyOrderingOptions, LineLengthOptions, NewLinesOptions, OctalValuesOptions,
    QuotedStringsOptions, TruthyOptions,
};

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

    /// Explains why a value of an otherwise known option is not implemented.
    ///
    /// Returns a hint for the user when yamllint accepts `value` but fast-yaml cannot honor it.
    fn unsupported_value(_key: &str, _value: &Value) -> Option<&'static str> {
        None
    }

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
}

impl<O: Default> Default for RuleSettings<O> {
    fn default() -> Self {
        Self {
            enabled: true,
            severity: None,
            options: O::default(),
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
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
#[error("unknown rule '{}'", echo(.name, KEY_LIMIT))]
pub struct UnknownRuleError {
    /// The rejected rule name.
    pub name: String,
}

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
    /// Returns an error when the code is empty or equals a built-in rule name.
    pub fn new(code: impl Into<String>) -> Result<Self, RuleConfigError> {
        let code = code.into();
        if code.is_empty() {
            return Err(RuleConfigError::EmptyRuleCode);
        }
        if RuleName::from_str(&code).is_ok() {
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

const fn value_kind(value: &Value) -> &'static str {
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

fn apply_entry<O: RuleOptions>(
    rule: RuleName,
    settings: &mut RuleSettings<O>,
    entry: Value,
) -> Result<(), RuleConfigError> {
    match entry {
        Value::Null => Ok(()),
        Value::String(text) => {
            if text.eq_ignore_ascii_case("enable") {
                settings.enabled = true;
            } else if text.eq_ignore_ascii_case("disable") {
                settings.enabled = false;
            } else {
                settings.severity = Some(
                    text.parse()
                        .map_err(|cause| RuleConfigError::InvalidSeverity { rule, cause })?,
                );
            }
            Ok(())
        }
        Value::Mapping(map) => apply_mapping(rule, settings, map),
        other => Err(RuleConfigError::InvalidEntry {
            rule,
            message: format!(
                "expected a severity, 'enable', 'disable' or a mapping, got {}",
                value_kind(&other)
            ),
        }),
    }
}

fn apply_mapping<O: RuleOptions>(
    rule: RuleName,
    settings: &mut RuleSettings<O>,
    map: Mapping,
) -> Result<(), RuleConfigError> {
    let mut overlay = Mapping::new();
    let mut bool_word_keys = Vec::new();
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
            "severity" => match value {
                Value::String(text) => {
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
            "level" => {
                return Err(RuleConfigError::InvalidOption {
                    rule,
                    key,
                    message: "unknown option; the yamllint `level` is called `severity` here"
                        .to_owned(),
                });
            }
            _ => {
                if O::YAMLLINT_UNSUPPORTED.contains(&key.as_str()) {
                    return Err(RuleConfigError::UnsupportedOption {
                        rule,
                        key,
                        hint: None,
                    });
                }
                if let Some(hint) = O::unsupported_value(&key, &value) {
                    return Err(RuleConfigError::UnsupportedOption {
                        rule,
                        key,
                        hint: Some(hint),
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

    overlay_options(rule, settings, overlay, &bool_word_keys)
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

            fn apply_value(&mut self, name: RuleName, entry: Value) -> Result<(), RuleConfigError> {
                match name {
                    $(RuleName::$variant => apply_entry(name, &mut self.$field, entry),)+
                }
            }
        }

        /// Instantiates every built-in rule in registry order.
        pub fn default_rules() -> Vec<Box<dyn LintRule>> {
            vec![$(Box::new($rule) as Box<dyn LintRule>,)+]
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
        let entries = match buffer(deserializer)? {
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

        let mut next = self.clone();
        for (key, entry) in entries {
            let Value::String(name) = key else {
                return Err(RuleConfigError::Malformed {
                    message: format!("rule names must be strings, got {}", value_kind(&key)),
                });
            };
            let rule = RuleName::from_str(&name).map_err(RuleConfigError::UnknownRule)?;
            next.apply_value(rule, entry)?;
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
        let entry = buffer(deserializer)?;
        let mut next = self.clone();
        next.apply_value(name, entry)?;
        *self = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{QuoteRequirement, RuleRegistry};

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
        let codes: Vec<&str> = registry.rules().iter().map(|rule| rule.code()).collect();
        let names: Vec<&str> = RuleName::ALL.iter().map(|name| name.as_str()).collect();
        assert_eq!(codes, names);
        assert_eq!(RuleName::ALL.len(), 24);
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
    fn yamllint_only_options_are_refused() {
        let cases = [
            ("line-length", "allow-non-breakable-words"),
            ("line-length", "allow-non-breakable-inline-mappings"),
            ("indentation", "spaces"),
            ("indentation", "indent-sequences"),
            ("indentation", "check-multi-line-strings"),
            ("key-ordering", "ignored-keys"),
            ("quoted-strings", "allow-quoted-quotes"),
            ("quoted-strings", "check-keys"),
            ("duplicate-key", "forbid-duplicated-merge-keys"),
            ("invalid-anchor", "forbid-undeclared-aliases"),
            ("invalid-anchor", "forbid-duplicated-anchors"),
            ("invalid-anchor", "forbid-unused-anchors"),
        ];
        for (rule, key) in cases {
            let message = error_of(&format!("{rule}: {{{key}: true}}"));
            assert!(message.contains("supported by yamllint"), "{message}");
            assert!(message.contains(rule) && message.contains(key), "{message}");
        }
    }

    #[test]
    fn document_end_present_false_is_refused_with_hint() {
        let message = error_of("document-end: {present: false}");
        assert!(message.contains("document-end"), "{message}");
        assert!(message.contains("disable the rule"), "{message}");
        let rules = applied("document-end: {present: true}");
        assert_eq!(
            rules.document_end.options.present,
            crate::rules::DocumentEndPresence::Required
        );
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
                original.apply_value(name, Value::Mapping(entry)).unwrap();
            }
        }
        let mut restored = RulesConfig::default();
        restored
            .apply(Value::Mapping(original.to_entries()))
            .unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn level_key_points_to_severity() {
        let message = error_of("line-length: {level: warning}");
        assert!(
            message.contains("level") && message.contains("severity"),
            "{message}"
        );
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
    fn inert_extra_patterns_are_rejected() {
        for yaml in [
            "quoted-strings: {required: false, extra-required: [a]}",
            "quoted-strings: {required: always, extra-required: [a]}",
            "quoted-strings: {required: never, extra-allowed: [a]}",
            "quoted-strings: {extra-allowed: [a]}",
        ] {
            let message = error_of(yaml);
            assert!(message.contains("has no effect"), "{yaml}: {message}");
        }
        let rules = applied("quoted-strings: {extra-required: [a]}");
        assert_eq!(rules.quoted_strings.options.extra_required, ["a"]);
        let rules = applied("quoted-strings: {required: true, extra-allowed: [a]}");
        assert_eq!(rules.quoted_strings.options.extra_allowed, ["a"]);
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
        let mut config = LintConfig::default();
        apply(&mut config.rules, "braces: error").unwrap();
        assert_eq!(
            config.severity_for("braces", Severity::Hint),
            Severity::Error
        );
        assert_eq!(
            config.severity_for("colons", Severity::Hint),
            Severity::Hint
        );
    }

    #[test]
    fn stale_extra_patterns_do_not_block_changing_required() {
        let mut rules = applied("quoted-strings: {extra-required: [a]}");
        apply(&mut rules, "quoted-strings: {required: always}").unwrap();
        assert_eq!(
            rules.quoted_strings.options.required,
            QuoteRequirement::Always
        );
        let mut rules = applied("quoted-strings: {required: always, extra-allowed: [a]}");
        apply(&mut rules, "quoted-strings: {required: false}").unwrap();
        assert_eq!(
            rules.quoted_strings.options.required,
            QuoteRequirement::NotRequired
        );
    }

    #[test]
    fn conflict_is_reported_for_the_option_the_patch_sets() {
        let mut rules = applied("quoted-strings: {required: always}");
        let message = apply(&mut rules, "quoted-strings: {extra-required: [a]}")
            .unwrap_err()
            .to_string();
        assert!(message.contains("extra-required"), "{message}");
        let message = error_of("quoted-strings: {required: false, extra-required: [a]}");
        assert!(message.contains("extra-required"), "{message}");
        let mut rules = applied("quoted-strings: {extra-required: [a]}");
        apply(
            &mut rules,
            "quoted-strings: {required: always, extra-required: []}",
        )
        .unwrap();
        assert!(rules.quoted_strings.options.extra_required.is_empty());
    }

    #[test]
    fn regex_patterns_are_rejected() {
        for pattern in [
            "'^http'", "'.*:.*'", "'a|b'", "'(x)'", "'a$'", "'[ab]'", "'a+'", "'a?'", "'a{2}'",
            "'a\\\\b'",
        ] {
            let yaml = format!("quoted-strings: {{extra-required: [{pattern}]}}");
            let message = error_of(&yaml);
            assert!(message.contains("plain substrings"), "{pattern}: {message}");
            assert!(message.contains("extra-required"), "{pattern}: {message}");
        }
        let message = error_of("quoted-strings: {required: always, extra-allowed: ['^ftp']}");
        assert!(message.contains("extra-allowed"), "{message}");
        let rules = applied("quoted-strings: {extra-required: ['http', 'a.b']}");
        assert_eq!(rules.quoted_strings.options.extra_required, ["http", "a.b"]);
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
    }

    #[test]
    fn default_rules_follow_rule_name_order() {
        assert_eq!(default_rules().len(), RuleName::ALL.len());
    }
}
