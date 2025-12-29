//! Lint rules and rule registry.

use crate::{Diagnostic, LintConfig, LintContext, Severity};
use fast_yaml_core::Value;

mod braces;
mod brackets;
mod colons;
mod commas;
mod comments;
mod comments_indentation;
mod document_end;
mod document_start;
mod duplicate_keys;
mod empty_lines;
mod empty_values;
mod float_values;
pub mod flow_common;
mod hyphens;
mod indentation;
mod invalid_anchors;
mod key_ordering;
mod line_length;
mod new_line_at_end_of_file;
mod new_lines;
mod octal_values;
mod quoted_strings;
mod trailing_whitespace;
mod truthy;

pub use braces::BracesRule;
pub use brackets::BracketsRule;
pub use colons::ColonsRule;
pub use commas::CommasRule;
pub use comments::CommentsRule;
pub use comments_indentation::CommentsIndentationRule;
pub use document_end::DocumentEndRule;
pub use document_start::DocumentStartRule;
pub use duplicate_keys::DuplicateKeysRule;
pub use empty_lines::EmptyLinesRule;
pub use empty_values::EmptyValuesRule;
pub use float_values::FloatValuesRule;
pub use hyphens::HyphensRule;
pub use indentation::IndentationRule;
pub use invalid_anchors::InvalidAnchorsRule;
pub use key_ordering::KeyOrderingRule;
pub use line_length::LineLengthRule;
pub use new_line_at_end_of_file::NewLineAtEndOfFileRule;
pub use new_lines::NewLinesRule;
pub use octal_values::OctalValuesRule;
pub use quoted_strings::QuotedStringsRule;
pub use trailing_whitespace::TrailingWhitespaceRule;
pub use truthy::TruthyRule;

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
///     fn check(&self, context: &LintContext, value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
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
    /// - `context`: The lint context providing access to source, comments, and metadata
    /// - `value`: The parsed YAML value tree
    /// - `config`: Linter configuration
    ///
    /// # Returns
    ///
    /// A vector of diagnostics found by this rule. Empty if no issues.
    fn check(&self, context: &LintContext, value: &Value, config: &LintConfig) -> Vec<Diagnostic>;
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
    /// - Line Too Long (INFO)
    /// - Trailing Whitespace (HINT)
    /// - Document Start (WARNING)
    /// - Document End (WARNING)
    /// - Empty Values (WARNING)
    /// - New Line at End of File (INFO)
    /// - Braces (WARNING)
    /// - Brackets (WARNING)
    /// - Colons (WARNING)
    /// - Commas (WARNING)
    /// - Hyphens (WARNING)
    /// - Comments (INFO)
    /// - Comments Indentation (INFO)
    /// - Empty Lines (INFO)
    /// - New Lines (WARNING)
    /// - Octal Values (WARNING)
    /// - Truthy (WARNING)
    /// - Quoted Strings (WARNING)
    /// - Key Ordering (INFO)
    /// - Float Values (WARNING)
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::rules::RuleRegistry;
    ///
    /// let registry = RuleRegistry::with_default_rules();
    /// assert_eq!(registry.rules().len(), 21);
    /// ```
    #[must_use]
    pub fn with_default_rules() -> Self {
        let mut registry = Self::new();

        // Phase 1 rules (7)
        registry.add(Box::new(DuplicateKeysRule));
        registry.add(Box::new(LineLengthRule));
        registry.add(Box::new(TrailingWhitespaceRule));
        registry.add(Box::new(DocumentStartRule));
        registry.add(Box::new(DocumentEndRule));
        registry.add(Box::new(EmptyValuesRule));
        registry.add(Box::new(NewLineAtEndOfFileRule));

        // Phase 2 rules (5)
        registry.add(Box::new(BracesRule));
        registry.add(Box::new(BracketsRule));
        registry.add(Box::new(ColonsRule));
        registry.add(Box::new(CommasRule));
        registry.add(Box::new(HyphensRule));

        // Phase 3 rules (5)
        registry.add(Box::new(CommentsRule));
        registry.add(Box::new(CommentsIndentationRule));
        registry.add(Box::new(EmptyLinesRule));
        registry.add(Box::new(NewLinesRule));
        registry.add(Box::new(OctalValuesRule));

        // Phase 4 rules (4)
        registry.add(Box::new(TruthyRule));
        registry.add(Box::new(QuotedStringsRule));
        registry.add(Box::new(KeyOrderingRule));
        registry.add(Box::new(FloatValuesRule));

        // Not yet implemented - planned for future phases
        // registry.add(Box::new(IndentationRule));
        // registry.add(Box::new(InvalidAnchorsRule));

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
        assert_eq!(registry.rules().len(), 21);
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
        assert_eq!(registry.rules().len(), 21);
    }
}
