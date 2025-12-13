//! Main linter engine and configuration.

use crate::{Diagnostic, rules::RuleRegistry};
use fast_yaml_core::{Parser, Value};
use std::collections::HashSet;

/// Configuration for the linter.
///
/// Controls linting behavior including rule enablement,
/// formatting preferences, and validation strictness.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::LintConfig;
///
/// let config = LintConfig::default();
/// assert_eq!(config.max_line_length, Some(80));
/// assert_eq!(config.indent_size, 2);
/// ```
#[derive(Debug, Clone)]
pub struct LintConfig {
    /// Maximum line length (None = unlimited).
    pub max_line_length: Option<usize>,
    /// Expected indentation size in spaces.
    pub indent_size: usize,
    /// Require document start marker (---).
    pub require_document_start: bool,
    /// Require document end marker (...).
    pub require_document_end: bool,
    /// Allow duplicate keys (non-compliant behavior).
    pub allow_duplicate_keys: bool,
    /// Disabled rule codes.
    pub disabled_rules: HashSet<String>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            max_line_length: Some(80),
            indent_size: 2,
            require_document_start: false,
            require_document_end: false,
            allow_duplicate_keys: false,
            disabled_rules: HashSet::new(),
        }
    }
}

impl LintConfig {
    /// Creates a new configuration with default values.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintConfig;
    ///
    /// let config = LintConfig::new();
    /// assert_eq!(config.indent_size, 2);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum line length.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintConfig;
    ///
    /// let config = LintConfig::new().with_max_line_length(Some(120));
    /// assert_eq!(config.max_line_length, Some(120));
    /// ```
    #[must_use]
    pub const fn with_max_line_length(mut self, max: Option<usize>) -> Self {
        self.max_line_length = max;
        self
    }

    /// Sets the indentation size.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintConfig;
    ///
    /// let config = LintConfig::new().with_indent_size(4);
    /// assert_eq!(config.indent_size, 4);
    /// ```
    #[must_use]
    pub const fn with_indent_size(mut self, size: usize) -> Self {
        self.indent_size = size;
        self
    }

    /// Disables a rule by code.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{LintConfig, DiagnosticCode};
    ///
    /// let config = LintConfig::new()
    ///     .with_disabled_rule(DiagnosticCode::LINE_LENGTH);
    ///
    /// assert!(config.is_rule_disabled(DiagnosticCode::LINE_LENGTH));
    /// ```
    #[must_use]
    pub fn with_disabled_rule(mut self, code: impl Into<String>) -> Self {
        self.disabled_rules.insert(code.into());
        self
    }

    /// Checks if a rule is disabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{LintConfig, DiagnosticCode};
    ///
    /// let config = LintConfig::new()
    ///     .with_disabled_rule(DiagnosticCode::LINE_LENGTH);
    ///
    /// assert!(config.is_rule_disabled(DiagnosticCode::LINE_LENGTH));
    /// assert!(!config.is_rule_disabled(DiagnosticCode::DUPLICATE_KEY));
    /// ```
    #[must_use]
    pub fn is_rule_disabled(&self, code: &str) -> bool {
        self.disabled_rules.contains(code)
    }
}

/// The main linter.
///
/// Orchestrates the linting process by parsing YAML source,
/// running enabled rules, and collecting diagnostics.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::Linter;
///
/// let yaml = "name: value\nage: 30";
/// let linter = Linter::with_all_rules();
/// let diagnostics = linter.lint(yaml).unwrap();
/// ```
pub struct Linter {
    config: LintConfig,
    registry: RuleRegistry,
}

impl Linter {
    /// Creates a new linter with default configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Linter;
    ///
    /// let linter = Linter::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: LintConfig::default(),
            registry: RuleRegistry::new(),
        }
    }

    /// Creates a linter with custom configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{Linter, LintConfig};
    ///
    /// let config = LintConfig::new().with_indent_size(4);
    /// let linter = Linter::with_config(config);
    /// ```
    #[must_use]
    pub fn with_config(config: LintConfig) -> Self {
        Self {
            config,
            registry: RuleRegistry::new(),
        }
    }

    /// Creates a linter with all default rules enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Linter;
    ///
    /// let linter = Linter::with_all_rules();
    /// ```
    #[must_use]
    pub fn with_all_rules() -> Self {
        Self {
            config: LintConfig::default(),
            registry: RuleRegistry::with_default_rules(),
        }
    }

    /// Adds a custom rule.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{Linter, rules::DuplicateKeysRule};
    ///
    /// let mut linter = Linter::new();
    /// linter.add_rule(Box::new(DuplicateKeysRule));
    /// ```
    pub fn add_rule(&mut self, rule: Box<dyn crate::rules::LintRule>) -> &mut Self {
        self.registry.add(rule);
        self
    }

    /// Lints YAML source code.
    ///
    /// Parses the source and runs all enabled rules.
    ///
    /// # Errors
    ///
    /// Returns `LintError::ParseError` if the YAML cannot be parsed.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Linter;
    ///
    /// let yaml = "name: John\nage: 30";
    /// let linter = Linter::with_all_rules();
    /// let diagnostics = linter.lint(yaml).unwrap();
    /// ```
    pub fn lint(&self, source: &str) -> Result<Vec<Diagnostic>, LintError> {
        let value_opt = Parser::parse_str(source)?;

        value_opt.map_or_else(
            || Ok(Vec::new()),
            |value| Ok(self.lint_value(source, &value)),
        )
    }

    /// Lints a pre-parsed Value (avoids double parsing).
    ///
    /// Use this when you already have a parsed YAML value.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Linter;
    /// use fast_yaml_core::parse_str;
    ///
    /// let yaml = "name: John";
    /// let value = parse_str(yaml).unwrap();
    ///
    /// let linter = Linter::with_all_rules();
    /// let diagnostics = linter.lint_value(yaml, &value);
    /// ```
    #[must_use]
    pub fn lint_value(&self, source: &str, value: &Value) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for rule in self.registry.rules() {
            if self.config.is_rule_disabled(rule.code()) {
                continue;
            }

            let mut rule_diagnostics = rule.check(source, value, &self.config);
            diagnostics.append(&mut rule_diagnostics);
        }

        diagnostics.sort_by(|a, b| a.span.start.cmp(&b.span.start));

        diagnostics
    }

    /// Gets the current configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{Linter, LintConfig};
    ///
    /// let config = LintConfig::new().with_indent_size(4);
    /// let linter = Linter::with_config(config);
    ///
    /// assert_eq!(linter.config().indent_size, 4);
    /// ```
    #[must_use]
    pub const fn config(&self) -> &LintConfig {
        &self.config
    }

    /// Gets the rule registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Linter;
    ///
    /// let linter = Linter::with_all_rules();
    /// assert!(!linter.registry().rules().is_empty());
    /// ```
    #[must_use]
    pub const fn registry(&self) -> &RuleRegistry {
        &self.registry
    }
}

impl Default for Linter {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during linting.
#[derive(Debug, thiserror::Error)]
pub enum LintError {
    /// Failed to parse YAML.
    #[error("failed to parse YAML: {0}")]
    ParseError(#[from] fast_yaml_core::ParseError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = LintConfig::default();
        assert_eq!(config.max_line_length, Some(80));
        assert_eq!(config.indent_size, 2);
        assert!(!config.require_document_start);
        assert!(!config.allow_duplicate_keys);
    }

    #[test]
    fn test_config_builder() {
        let config = LintConfig::new()
            .with_max_line_length(Some(120))
            .with_indent_size(4);

        assert_eq!(config.max_line_length, Some(120));
        assert_eq!(config.indent_size, 4);
    }

    #[test]
    fn test_config_disabled_rules() {
        let config = LintConfig::new().with_disabled_rule("line-length");

        assert!(config.is_rule_disabled("line-length"));
        assert!(!config.is_rule_disabled("duplicate-key"));
    }

    #[test]
    fn test_linter_new() {
        let linter = Linter::new();
        assert!(linter.registry().rules().is_empty());
    }

    #[test]
    fn test_linter_with_all_rules() {
        let linter = Linter::with_all_rules();
        assert_eq!(linter.registry().rules().len(), 5);
    }

    #[test]
    fn test_linter_with_config() {
        let config = LintConfig::new().with_indent_size(4);
        let linter = Linter::with_config(config);
        assert_eq!(linter.config().indent_size, 4);
    }

    #[test]
    fn test_linter_lint_valid() {
        let yaml = "name: John\nage: 30";
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        assert!(
            !diagnostics
                .iter()
                .any(|d| d.severity == crate::Severity::Error)
        );
    }

    #[test]
    fn test_linter_lint_invalid_yaml() {
        let yaml = "invalid: [unclosed";
        let linter = Linter::with_all_rules();
        let result = linter.lint(yaml);

        assert!(result.is_err());
    }

    #[test]
    fn test_linter_lint_value() {
        let yaml = "name: John";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint_value(yaml, &value);

        assert!(
            diagnostics
                .iter()
                .all(|d| d.severity != crate::Severity::Error)
        );
    }

    #[test]
    fn test_linter_disabled_rule() {
        let yaml = "very_long_line: this line is definitely longer than eighty characters and should trigger a warning";
        let config = LintConfig::new().with_disabled_rule("line-length");
        let linter = Linter::with_config(config);

        let mut linter = linter;
        linter.add_rule(Box::new(crate::rules::LineLengthRule));

        let diagnostics = linter.lint(yaml).unwrap();

        assert!(!diagnostics.iter().any(|d| d.code.as_str() == "line-length"));
    }
}
