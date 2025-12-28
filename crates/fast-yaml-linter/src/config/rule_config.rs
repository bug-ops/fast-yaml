//! Per-rule configuration types.

use std::collections::HashMap;

#[cfg(feature = "json-output")]
use serde::{Deserialize, Serialize};

use crate::Severity;

/// Configuration for a single linting rule.
///
/// Controls whether the rule is enabled, its severity level, and rule-specific options.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::RuleConfig;
/// use fast_yaml_linter::Severity;
///
/// let config = RuleConfig::new()
///     .with_severity(Severity::Error)
///     .with_option("max", 120usize);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
pub struct RuleConfig {
    /// Whether this rule is enabled
    pub enabled: bool,
    /// Override the rule's default severity
    pub severity: Option<Severity>,
    /// Rule-specific configuration options
    pub options: RuleOptions,
}

impl RuleConfig {
    /// Creates a new rule configuration with defaults (enabled, no severity override, no options).
    pub fn new() -> Self {
        Self {
            enabled: true,
            severity: None,
            options: RuleOptions::new(),
        }
    }

    /// Creates a disabled rule configuration.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            severity: None,
            options: RuleOptions::new(),
        }
    }

    /// Sets a severity override for this rule.
    #[must_use]
    pub const fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = Some(severity);
        self
    }

    /// Adds a configuration option for this rule.
    #[must_use]
    pub fn with_option(mut self, key: impl Into<String>, value: impl Into<RuleOption>) -> Self {
        self.options.set(key, value);
        self
    }
}

impl Default for RuleConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Container for rule-specific configuration options.
///
/// Provides type-safe access to bool, int, string, and string list values.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
pub struct RuleOptions(HashMap<String, RuleOption>);

impl RuleOptions {
    /// Creates an empty options container.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Sets an option value.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<RuleOption>) {
        self.0.insert(key.into(), value.into());
    }

    /// Gets an option value by key.
    pub fn get(&self, key: &str) -> Option<&RuleOption> {
        self.0.get(key)
    }

    /// Gets a boolean option value.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match self.get(key) {
            Some(RuleOption::Bool(v)) => Some(*v),
            _ => None,
        }
    }

    /// Gets an integer option value.
    pub fn get_int(&self, key: &str) -> Option<i64> {
        match self.get(key) {
            Some(RuleOption::Int(v)) => Some(*v),
            _ => None,
        }
    }

    /// Gets an integer option value as usize.
    pub fn get_usize(&self, key: &str) -> Option<usize> {
        self.get_int(key).and_then(|i| usize::try_from(i).ok())
    }

    /// Gets a string option value.
    pub fn get_string(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(RuleOption::String(v)) => Some(v.as_str()),
            _ => None,
        }
    }

    /// Gets a string list option value.
    pub fn get_string_list(&self, key: &str) -> Option<&[String]> {
        match self.get(key) {
            Some(RuleOption::StringList(v)) => Some(v.as_slice()),
            _ => None,
        }
    }
}

/// A configuration option value for a linting rule.
///
/// Supports boolean, integer, string, and string list values.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "json-output", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "json-output", serde(untagged))]
pub enum RuleOption {
    /// Boolean value
    Bool(bool),
    /// Integer value
    Int(i64),
    /// String value
    String(String),
    /// List of strings
    StringList(Vec<String>),
}

impl From<bool> for RuleOption {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<i64> for RuleOption {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}

impl From<usize> for RuleOption {
    fn from(v: usize) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        Self::Int(v as i64)
    }
}

impl From<String> for RuleOption {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

impl From<&str> for RuleOption {
    fn from(v: &str) -> Self {
        Self::String(v.to_string())
    }
}

impl From<Vec<String>> for RuleOption {
    fn from(v: Vec<String>) -> Self {
        Self::StringList(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_config_new() {
        let config = RuleConfig::new();
        assert!(config.enabled);
        assert!(config.severity.is_none());
    }

    #[test]
    fn test_rule_config_disabled() {
        let config = RuleConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_rule_config_with_severity() {
        let config = RuleConfig::new().with_severity(Severity::Error);
        assert_eq!(config.severity, Some(Severity::Error));
    }

    #[test]
    fn test_rule_config_with_option() {
        let config = RuleConfig::new().with_option("max", 120usize);
        assert_eq!(config.options.get_usize("max"), Some(120));
    }

    #[test]
    fn test_rule_options_get_bool() {
        let mut options = RuleOptions::new();
        options.set("enabled", true);
        assert_eq!(options.get_bool("enabled"), Some(true));
    }

    #[test]
    fn test_rule_options_get_int() {
        let mut options = RuleOptions::new();
        options.set("max", 100i64);
        assert_eq!(options.get_int("max"), Some(100));
    }

    #[test]
    fn test_rule_options_get_string() {
        let mut options = RuleOptions::new();
        options.set("mode", "strict");
        assert_eq!(options.get_string("mode"), Some("strict"));
    }

    #[test]
    fn test_rule_options_get_string_list() {
        let mut options = RuleOptions::new();
        options.set("allowed", vec!["true".to_string(), "false".to_string()]);
        assert_eq!(
            options.get_string_list("allowed"),
            Some(&["true".to_string(), "false".to_string()][..])
        );
    }

    #[test]
    fn test_rule_option_from_conversions() {
        assert!(matches!(RuleOption::from(true), RuleOption::Bool(true)));
        assert!(matches!(RuleOption::from(42i64), RuleOption::Int(42)));
        assert!(matches!(
            RuleOption::from("test"),
            RuleOption::String(s) if s == "test"
        ));
        assert!(matches!(
            RuleOption::from(vec!["a".to_string()]),
            RuleOption::StringList(v) if v == vec!["a"]
        ));
    }
}
