//! Lint rules and rule registry.

use crate::{Diagnostic, LintConfig, Severity};
use fast_yaml_core::Value;

mod duplicate_keys;
mod indentation;
mod invalid_anchors;
mod line_length;
mod trailing_whitespace;

pub use duplicate_keys::DuplicateKeysRule;
pub use indentation::IndentationRule;
pub use invalid_anchors::InvalidAnchorsRule;
pub use line_length::LineLengthRule;
pub use trailing_whitespace::TrailingWhitespaceRule;

/// Trait for implementing lint rules.
///
/// All lint rules must implement this trait to be used with the linter.
/// Rules check YAML source and values, returning diagnostics for any issues found.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{Diagnostic, LintConfig, Severity, DiagnosticCode};
/// use fast_yaml_linter::rules::LintRule;
/// use fast_yaml_core::Value;
///
/// struct ExampleRule;
///
/// impl LintRule for ExampleRule {
///     fn code(&self) -> &str {
///         "example-rule"
///     }
///
///     fn name(&self) -> &str {
///         "Example Rule"
///     }
///
///     fn description(&self) -> &str {
///         "An example lint rule"
///     }
///
///     fn default_severity(&self) -> Severity {
///         Severity::Warning
///     }
///
///     fn check(&self, source: &str, value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
///         Vec::new()
///     }
/// }
/// ```
pub trait LintRule: Send + Sync {
    /// Unique code for this rule.
    ///
    /// Should be kebab-case, e.g., "duplicate-key", "line-length".
    fn code(&self) -> &str;

    /// Human-readable name.
    ///
    /// Should be title case, e.g., "Duplicate Keys", "Line Length".
    fn name(&self) -> &str;

    /// Detailed description of what this rule checks.
    fn description(&self) -> &str;

    /// Default severity level.
    fn default_severity(&self) -> Severity;

    /// Checks the source and returns diagnostics.
    ///
    /// # Parameters
    ///
    /// - `source`: The original YAML source text
    /// - `value`: The parsed YAML value tree
    /// - `config`: Linter configuration
    ///
    /// # Returns
    ///
    /// A vector of diagnostics found by this rule. Empty if no issues.
    fn check(&self, source: &str, value: &Value, config: &LintConfig) -> Vec<Diagnostic>;
}

/// Registry of all available lint rules.
///
/// Manages a collection of lint rules that can be applied to YAML sources.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::rules::RuleRegistry;
///
/// let registry = RuleRegistry::with_default_rules();
/// assert!(!registry.rules().is_empty());
/// ```
pub struct RuleRegistry {
    rules: Vec<Box<dyn LintRule>>,
}

impl RuleRegistry {
    /// Creates a new empty registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::rules::RuleRegistry;
    ///
    /// let registry = RuleRegistry::new();
    /// assert!(registry.rules().is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Registers all default rules.
    ///
    /// Includes:
    /// - Duplicate Keys (ERROR)
    /// - Invalid Anchors (ERROR)
    /// - Inconsistent Indentation (WARNING)
    /// - Line Too Long (WARNING)
    /// - Trailing Whitespace (INFO)
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::rules::RuleRegistry;
    ///
    /// let registry = RuleRegistry::with_default_rules();
    /// assert_eq!(registry.rules().len(), 5);
    /// ```
    #[must_use]
    pub fn with_default_rules() -> Self {
        let mut registry = Self::new();
        registry.add(Box::new(DuplicateKeysRule));
        registry.add(Box::new(InvalidAnchorsRule));
        registry.add(Box::new(IndentationRule));
        registry.add(Box::new(LineLengthRule));
        registry.add(Box::new(TrailingWhitespaceRule));
        registry
    }

    /// Adds a rule to the registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::rules::{RuleRegistry, DuplicateKeysRule};
    ///
    /// let mut registry = RuleRegistry::new();
    /// registry.add(Box::new(DuplicateKeysRule));
    /// assert_eq!(registry.rules().len(), 1);
    /// ```
    pub fn add(&mut self, rule: Box<dyn LintRule>) -> &mut Self {
        self.rules.push(rule);
        self
    }

    /// Gets all registered rules.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::rules::RuleRegistry;
    ///
    /// let registry = RuleRegistry::with_default_rules();
    /// assert!(!registry.rules().is_empty());
    /// ```
    #[must_use]
    pub fn rules(&self) -> &[Box<dyn LintRule>] {
        &self.rules
    }

    /// Gets a rule by code.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::rules::RuleRegistry;
    /// use fast_yaml_linter::DiagnosticCode;
    ///
    /// let registry = RuleRegistry::with_default_rules();
    /// let rule = registry.get(DiagnosticCode::DUPLICATE_KEY);
    /// assert!(rule.is_some());
    /// ```
    #[must_use]
    pub fn get(&self, code: &str) -> Option<&dyn LintRule> {
        self.rules.iter().find(|r| r.code() == code).map(|b| &**b)
    }
}

impl Default for RuleRegistry {
    fn default() -> Self {
        Self::with_default_rules()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let registry = RuleRegistry::new();
        assert!(registry.rules().is_empty());
    }

    #[test]
    fn test_registry_with_default_rules() {
        let registry = RuleRegistry::with_default_rules();
        assert_eq!(registry.rules().len(), 5);
    }

    #[test]
    fn test_registry_add() {
        let mut registry = RuleRegistry::new();
        registry.add(Box::new(DuplicateKeysRule));
        assert_eq!(registry.rules().len(), 1);
    }

    #[test]
    fn test_registry_get() {
        let registry = RuleRegistry::with_default_rules();
        let rule = registry.get("duplicate-key");
        assert!(rule.is_some());
        assert_eq!(rule.unwrap().code(), "duplicate-key");
    }

    #[test]
    fn test_registry_get_missing() {
        let registry = RuleRegistry::with_default_rules();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_default() {
        let registry = RuleRegistry::default();
        assert_eq!(registry.rules().len(), 5);
    }
}
