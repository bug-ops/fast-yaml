//! Main linter engine and configuration.

use crate::{Diagnostic, LintContext, Severity, config::RuleConfig, rules::RuleRegistry};
use fast_yaml_core::{Parser, ScalarOwned, Value};
use std::collections::{HashMap, HashSet};

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
    /// Per-rule configurations.
    pub rule_configs: HashMap<String, RuleConfig>,
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
            rule_configs: HashMap::new(),
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
            || self.rule_configs.get(code).is_some_and(|rc| !rc.enabled)
    }

    /// Gets the configuration for a specific rule.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{LintConfig, config::RuleConfig};
    ///
    /// let mut config = LintConfig::new();
    /// let rule_config = RuleConfig::new().with_option("max", 120usize);
    /// config = config.with_rule_config("line-length", rule_config);
    ///
    /// assert!(config.get_rule_config("line-length").is_some());
    /// ```
    #[must_use]
    pub fn get_rule_config(&self, rule_code: &str) -> Option<&RuleConfig> {
        self.rule_configs.get(rule_code)
    }

    /// Checks if a rule is enabled (not disabled via config or rule-specific config).
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{LintConfig, config::RuleConfig};
    ///
    /// let config = LintConfig::new()
    ///     .with_rule_config("line-length", RuleConfig::disabled());
    ///
    /// assert!(!config.is_rule_enabled("line-length"));
    /// assert!(config.is_rule_enabled("duplicate-key"));
    /// ```
    #[must_use]
    pub fn is_rule_enabled(&self, rule_code: &str) -> bool {
        !self.is_rule_disabled(rule_code)
            && self.get_rule_config(rule_code).is_none_or(|rc| rc.enabled)
    }

    /// Gets the effective severity for a rule (with per-rule override).
    ///
    /// Returns the per-rule severity override if set, otherwise the rule's default severity.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{LintConfig, Severity, config::RuleConfig};
    ///
    /// let config = LintConfig::new()
    ///     .with_rule_config(
    ///         "line-length",
    ///         RuleConfig::new().with_severity(Severity::Error),
    ///     );
    ///
    /// assert_eq!(
    ///     config.get_effective_severity("line-length", Severity::Warning),
    ///     Severity::Error
    /// );
    /// ```
    #[must_use]
    pub fn get_effective_severity(&self, rule_code: &str, default: Severity) -> Severity {
        self.get_rule_config(rule_code)
            .and_then(|rc| rc.severity)
            .unwrap_or(default)
    }

    /// Allows or disallows duplicate keys.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintConfig;
    ///
    /// let config = LintConfig::new().with_allow_duplicate_keys(true);
    /// assert!(config.allow_duplicate_keys);
    /// ```
    #[must_use]
    pub const fn with_allow_duplicate_keys(mut self, allow: bool) -> Self {
        self.allow_duplicate_keys = allow;
        self
    }

    /// Adds a rule-specific configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{LintConfig, config::RuleConfig};
    ///
    /// let config = LintConfig::new()
    ///     .with_rule_config(
    ///         "line-length",
    ///         RuleConfig::new().with_option("max", 120usize),
    ///     );
    ///
    /// assert!(config.get_rule_config("line-length").is_some());
    /// ```
    #[must_use]
    pub fn with_rule_config(mut self, rule_code: impl Into<String>, config: RuleConfig) -> Self {
        self.rule_configs.insert(rule_code.into(), config);
        self
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

    /// Creates a linter with all default rules and custom configuration.
    ///
    /// This is equivalent to [`Linter::with_all_rules_and_config`] and loads
    /// all default rules with the provided configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{Linter, LintConfig};
    ///
    /// let config = LintConfig::new().with_indent_size(4);
    /// let linter = Linter::with_config(config);
    /// assert!(!linter.registry().rules().is_empty());
    /// ```
    #[must_use]
    pub fn with_config(config: LintConfig) -> Self {
        Self {
            config,
            registry: RuleRegistry::with_default_rules(),
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

    /// Creates a linter with all default rules and custom configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{Linter, LintConfig};
    ///
    /// let config = LintConfig::new().with_max_line_length(Some(120));
    /// let linter = Linter::with_all_rules_and_config(config);
    /// ```
    #[must_use]
    pub fn with_all_rules_and_config(config: LintConfig) -> Self {
        Self {
            config,
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
        let docs = Parser::parse_all(source)?;
        let doc_start_lines = compute_doc_start_lines(source, docs.len());
        let mut context = LintContext::new(source);
        let mut diagnostics = Vec::new();

        for rule in self.registry.rules() {
            if self.config.is_rule_disabled(rule.code()) {
                continue;
            }

            if rule.needs_value() {
                for (idx, doc) in docs.iter().enumerate() {
                    let start_line = doc_start_lines.get(idx).copied().unwrap_or(1);
                    context.set_doc_start_line(start_line);
                    diagnostics.extend(rule.check(&context, doc, &self.config));
                }
                context.set_doc_start_line(1);
            } else {
                let dummy = Value::Value(ScalarOwned::Null);
                diagnostics.extend(rule.check(&context, &dummy, &self.config));
            }
        }

        diagnostics.sort_by(|a, b| a.span.start.cmp(&b.span.start));
        Ok(diagnostics)
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
        let context = LintContext::new(source);
        let mut diagnostics = Vec::new();

        for rule in self.registry.rules() {
            if self.config.is_rule_disabled(rule.code()) {
                continue;
            }

            let mut rule_diagnostics = rule.check(&context, value, &self.config);
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

/// Returns a `Vec` where `result[i]` is the 1-based line number at which
/// document `i` begins in `source`.
///
/// Document boundaries are detected by scanning for lines that consist solely
/// of `---` (the YAML directive-end marker). The first document always starts
/// at line 1. Each `---` line introduces the *next* document, which begins on
/// the following line.
fn compute_doc_start_lines(source: &str, doc_count: usize) -> Vec<usize> {
    let mut starts = Vec::with_capacity(doc_count);
    starts.push(1usize);

    for (idx, line) in source.lines().enumerate() {
        if line.trim_end_matches('\r') == "---" {
            starts.push(idx + 2); // line after `---` (1-based)
            if starts.len() == doc_count {
                break;
            }
        }
    }

    starts
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
        assert_eq!(linter.registry().rules().len(), 23);
    }

    #[test]
    fn test_linter_with_config() {
        let config = LintConfig::new().with_indent_size(4);
        let linter = Linter::with_config(config);
        assert_eq!(linter.config().indent_size, 4);
        assert!(!linter.registry().rules().is_empty());
    }

    #[test]
    fn test_linter_with_config_detects_duplicate_keys() {
        let yaml = "key: 1\nkey: 2\n";
        let config = LintConfig::new().with_allow_duplicate_keys(false);
        let linter = Linter::with_config(config);
        let diagnostics = linter.lint(yaml).unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_str() == "duplicate-key"),
            "Linter::with_config should detect duplicate keys"
        );
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

    #[test]
    fn test_multidoc_key_ordering_all_documents() {
        // Regression test for #142: key-ordering must fire in ALL documents, not just the first.
        let yaml = "---\nb: 1\na: 2\n---\nd: 1\nc: 2\n";
        let config = LintConfig::new().with_rule_config(
            crate::DiagnosticCode::KEY_ORDERING,
            crate::config::RuleConfig::new(),
        );
        let linter = Linter::with_all_rules_and_config(config);
        let diagnostics = linter.lint(yaml).unwrap();

        let ordering_diags: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code.as_str() == crate::DiagnosticCode::KEY_ORDERING)
            .collect();

        assert!(
            ordering_diags.len() >= 2,
            "key-ordering should fire in both documents, got {} diagnostics: {:?}",
            ordering_diags.len(),
            ordering_diags
        );
    }

    #[test]
    fn test_multidoc_empty_values_all_documents() {
        // Regression test for #142: empty-values must fire in ALL documents, not just the first.
        let yaml = "---\nfoo:\n---\nbar:\n";
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        let empty_diags: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code.as_str() == crate::DiagnosticCode::EMPTY_VALUES)
            .collect();

        assert!(
            empty_diags.len() >= 2,
            "empty-values should fire in both documents, got {} diagnostics: {:?}",
            empty_diags.len(),
            empty_diags
        );
    }

    #[test]
    fn test_single_doc_key_ordering_no_regression() {
        // Verify single-doc YAML with correct key order produces no key-ordering diagnostic.
        let yaml = "a: 1\nb: 2\nc: 3\n";
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code.as_str() == crate::DiagnosticCode::KEY_ORDERING),
            "correctly ordered single-doc should produce no key-ordering diagnostics"
        );
    }

    #[test]
    fn test_single_doc_empty_values_no_regression() {
        // Verify single-doc YAML without empty values produces no empty-values diagnostic.
        let yaml = "foo: bar\nbaz: qux\n";
        let linter = Linter::with_all_rules();
        let diagnostics = linter.lint(yaml).unwrap();

        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code.as_str() == crate::DiagnosticCode::EMPTY_VALUES),
            "single-doc without empty values should produce no empty-values diagnostics"
        );
    }

    #[test]
    fn test_linter_rule_config_disabled_suppresses_rule() {
        // Regression test for #133: RuleConfig::disabled() via with_rule_config should
        // suppress the rule, not just store it in rule_configs without effect.
        let yaml = "very_long_line: this line is definitely longer than eighty characters and should trigger a warning";
        let config = LintConfig::new()
            .with_rule_config("line-length", crate::config::RuleConfig::disabled());
        let linter = Linter::with_config(config);

        let mut linter = linter;
        linter.add_rule(Box::new(crate::rules::LineLengthRule));

        let diagnostics = linter.lint(yaml).unwrap();

        assert!(
            !diagnostics.iter().any(|d| d.code.as_str() == "line-length"),
            "rule disabled via RuleConfig::disabled() should not produce diagnostics"
        );
    }
}
