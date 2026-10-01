//! Lint rules and rule registry.

use std::fmt;

use crate::config::{CustomRuleCode, RuleName};
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
mod lint_directive;
mod new_line_at_end_of_file;
mod new_lines;
pub(crate) mod node_roles;
mod octal_values;
mod quoted_strings;
mod set_values;
mod token_stream;
mod trailing_whitespace;
mod truthy;

pub use crate::config::MarkerPresence;
pub use braces::BracesRule;
pub use brackets::BracketsRule;
pub use colons::{ColonsOptions, ColonsRule};
pub use commas::{CommasOptions, CommasRule};
pub use comments::{CommentsOptions, CommentsRule};
pub use comments_indentation::CommentsIndentationRule;
pub use document_end::{DocumentEndOptions, DocumentEndRule};
pub use document_start::{DocumentStartOptions, DocumentStartRule};
pub use duplicate_keys::{DuplicateKeysOptions, DuplicateKeysRule};
pub use empty_lines::{EmptyLinesOptions, EmptyLinesRule};
pub use empty_values::{EmptyValuesOptions, EmptyValuesRule};
pub use float_values::{FloatValuesOptions, FloatValuesRule};
pub use flow_common::{FlowCollectionOptions, Forbid};
pub use hyphens::{HyphensOptions, HyphensRule};
pub use indentation::{IndentationOptions, IndentationRule};
pub use invalid_anchors::{InvalidAnchorsOptions, InvalidAnchorsRule};
pub use key_ordering::{KeyOrderingOptions, KeyOrderingRule};
pub use line_length::{LineLengthOptions, LineLengthRule};
pub use lint_directive::LintDirectiveRule;
pub use new_line_at_end_of_file::NewLineAtEndOfFileRule;
pub use new_lines::{LineEndingType, NewLinesOptions, NewLinesRule};
pub use octal_values::{OctalValuesOptions, OctalValuesRule};
pub use quoted_strings::{QuoteRequirement, QuoteType, QuotedStringsOptions, QuotedStringsRule};
pub use set_values::SetValuesRule;
pub use trailing_whitespace::TrailingWhitespaceRule;
pub(crate) use truthy::NON_STANDARD_BOOLS;
pub use truthy::{TruthyOptions, TruthyRule, TruthySpelling, UnknownTruthySpelling};

/// Identity of a lint rule: a built-in rule or a custom rule with a validated code.
///
/// Typed so that settings lookups never parse a string, and a built-in rule cannot be mistaken
/// for a custom one.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::{CustomRuleCode, RuleName};
/// use fast_yaml_linter::rules::RuleId;
///
/// let custom = CustomRuleCode::new("my-rule").unwrap();
/// assert_eq!(RuleId::BuiltIn(RuleName::Braces).as_str(), "braces");
/// assert_eq!(RuleId::Custom(&custom).as_str(), "my-rule");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleId<'a> {
    /// One of the built-in rules.
    BuiltIn(RuleName),
    /// A rule added with [`Linter::add_rule`](crate::Linter::add_rule).
    Custom(&'a CustomRuleCode),
}

impl<'a> RuleId<'a> {
    /// The kebab-case code that diagnostics and configuration use for the rule.
    #[must_use]
    pub fn as_str(self) -> &'a str {
        match self {
            Self::BuiltIn(name) => name.as_str(),
            Self::Custom(code) => code.as_str(),
        }
    }
}

impl RuleId<'_> {
    /// Copies the id so it can outlive the rule it came from.
    #[must_use]
    pub fn to_owned_id(self) -> OwnedRuleId {
        match self {
            Self::BuiltIn(name) => OwnedRuleId::BuiltIn(name),
            Self::Custom(code) => OwnedRuleId::Custom(code.clone()),
        }
    }
}

/// A [`RuleId`] that owns its custom code.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::RuleName;
/// use fast_yaml_linter::rules::{OwnedRuleId, RuleId};
///
/// let id = RuleId::BuiltIn(RuleName::Braces).to_owned_id();
/// assert_eq!(id, OwnedRuleId::BuiltIn(RuleName::Braces));
/// assert_eq!(id.as_id(), RuleId::BuiltIn(RuleName::Braces));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnedRuleId {
    /// One of the built-in rules.
    BuiltIn(RuleName),
    /// A rule added with [`Linter::add_rule`](crate::Linter::add_rule).
    Custom(CustomRuleCode),
}

impl OwnedRuleId {
    /// Borrows the id.
    #[must_use]
    pub const fn as_id(&self) -> RuleId<'_> {
        match self {
            Self::BuiltIn(name) => RuleId::BuiltIn(*name),
            Self::Custom(code) => RuleId::Custom(code),
        }
    }
}

impl fmt::Display for OwnedRuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_id().fmt(f)
    }
}

impl fmt::Display for RuleId<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str((*self).as_str())
    }
}

/// Metadata every lint rule exposes: its identity, name, description and default severity.
///
/// A rule also implements [`SourceRule`], which reads the source text and what the loader pass
/// collected, or [`DocumentRule`], which walks the value tree of each document, and is registered
/// as a [`Rule`].
///
/// # Contract
///
/// Implementors must return the same [`RuleId`] on every call. A [`RuleId::BuiltIn`] id makes the
/// rule share that built-in rule's settings and scan products, so a custom rule returns
/// [`RuleId::Custom`] with a [`CustomRuleCode`]. Callers may assume the metadata is cheap.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{Diagnostic, LintConfig, LintContext, Severity};
/// use fast_yaml_linter::config::CustomRuleCode;
/// use fast_yaml_linter::rules::{LintRule, Rule, RuleId, SourceRule};
///
/// struct ExampleRule(CustomRuleCode);
///
/// impl LintRule for ExampleRule {
///     fn id(&self) -> RuleId<'_> {
///         RuleId::Custom(&self.0)
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
/// }
///
/// impl SourceRule for ExampleRule {
///     fn check(&self, context: &LintContext, config: &LintConfig) -> Vec<Diagnostic> {
///         Vec::new()
///     }
/// }
///
/// let code = CustomRuleCode::new("example-rule").unwrap();
/// let rule = Rule::Source(Box::new(ExampleRule(code)));
/// assert_eq!(rule.info().id().as_str(), "example-rule");
/// ```
pub trait LintRule: Send + Sync {
    /// Identity of this rule.
    ///
    /// A custom rule returns [`RuleId::Custom`] with a [`CustomRuleCode`], which cannot name a
    /// built-in rule. Returning a [`RuleId::BuiltIn`] makes the rule share that rule's settings;
    /// the trait allows it on purpose, so that an embedder can register a replacement for a
    /// built-in rule in an empty [`Linter`](crate::Linter), and the registry rejects the id when
    /// the built-in is already registered. Splitting the trait to forbid it would double every
    /// registry type for one corner case.
    fn id(&self) -> RuleId<'_>;

    /// Human-readable name.
    ///
    /// Should be title case, e.g., "Duplicate Keys", "Line Length".
    fn name(&self) -> &str;

    /// Detailed description of what this rule checks.
    fn description(&self) -> &str;

    /// Default severity level.
    fn default_severity(&self) -> Severity;
}

/// A rule that reads the source text and what the loader pass collected, never the value tree.
///
/// Runs once for the whole input. Every built-in rule is a source rule, so linting builds no
/// value tree unless a [`DocumentRule`] is registered and enabled.
///
/// # Contract
///
/// Built-in rules read what `context` collected from the loader pass of the source. A context
/// that [`Linter`](crate::Linter) did not scan loads the source on first use under the default
/// [`ParseLimits`](fast_yaml_core::limits::ParseLimits), so no rule can drive an unguarded
/// parser (#563).
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{LintConfig, LintContext};
/// use fast_yaml_linter::rules::{DuplicateKeysRule, SourceRule};
///
/// let diagnostics = DuplicateKeysRule.check(&LintContext::new("a: 1\na: 2\n"), &LintConfig::default());
/// assert_eq!(diagnostics.len(), 1);
/// ```
pub trait SourceRule: LintRule {
    /// Checks the source and returns the diagnostics found, empty if there are none.
    ///
    /// `context` gives access to the source, comments and scan products; `config` holds the
    /// linter settings.
    fn check(&self, context: &LintContext, config: &LintConfig) -> Vec<Diagnostic>;
}

/// One parsed document handed to a [`DocumentRule`].
///
/// # Examples
///
/// ```
/// use fast_yaml_core::Value;
/// use fast_yaml_linter::rules::LintDocument;
///
/// let value = Value::Null;
/// let document = LintDocument { value: &value, first_line: 1 };
/// assert_eq!(document.first_line, 1);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct LintDocument<'a> {
    /// The loaded value of the document.
    pub value: &'a Value,
    /// 1-based line where the document's content begins, after its `---` marker.
    pub first_line: usize,
}

/// A rule that walks the value tree of each document.
///
/// Runs once per document of the stream. Registering and enabling one makes the linter build the
/// documents, which costs memory proportional to the input, so prefer a [`SourceRule`] when the
/// scan products are enough.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::Value;
/// use fast_yaml_linter::{Diagnostic, DiagnosticBuilder, LintConfig, LintContext, Linter, Location, Severity, Span};
/// use fast_yaml_linter::config::CustomRuleCode;
/// use fast_yaml_linter::rules::{DocumentRule, LintDocument, LintRule, Rule, RuleId};
///
/// struct NoNull(CustomRuleCode);
///
/// impl LintRule for NoNull {
///     fn id(&self) -> RuleId<'_> {
///         RuleId::Custom(&self.0)
///     }
///     fn name(&self) -> &str {
///         "No Null"
///     }
///     fn description(&self) -> &str {
///         "Rejects a null document"
///     }
///     fn default_severity(&self) -> Severity {
///         Severity::Warning
///     }
/// }
///
/// impl DocumentRule for NoNull {
///     fn check(
///         &self,
///         context: &LintContext,
///         document: LintDocument<'_>,
///         _config: &LintConfig,
///     ) -> Vec<Diagnostic> {
///         if *document.value != Value::Null {
///             return Vec::new();
///         }
///         let line = document.first_line;
///         let offset = context.source_context().get_line_offset(line);
///         let span = Span::new(Location::new(line, 1, offset), Location::new(line, 2, offset + 1));
///         vec![DiagnosticBuilder::new(self.0.as_str(), Severity::Warning, "null document", span).build()]
///     }
/// }
///
/// let mut linter = Linter::new();
/// let code = CustomRuleCode::new("no-null").unwrap();
/// linter.add_rule(Rule::Document(Box::new(NoNull(code)))).unwrap();
/// let found = linter.lint("a: 1\n---\n~\n").unwrap();
/// assert_eq!(found.len(), 1);
/// assert_eq!(found[0].span.start.line, 3);
/// ```
pub trait DocumentRule: LintRule {
    /// Checks one document and returns the diagnostics found, empty if there are none.
    fn check(
        &self,
        context: &LintContext,
        document: LintDocument<'_>,
        config: &LintConfig,
    ) -> Vec<Diagnostic>;
}

/// A registered rule, by the input it reads.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::rules::{DuplicateKeysRule, Rule};
///
/// let rule = Rule::Source(Box::new(DuplicateKeysRule));
/// assert_eq!(rule.info().id().as_str(), "duplicate-key");
/// ```
pub enum Rule {
    /// A rule that reads the source text only.
    Source(Box<dyn SourceRule>),
    /// A rule that walks the value tree of each document.
    Document(Box<dyn DocumentRule>),
}

impl Rule {
    /// The metadata of the rule.
    #[must_use]
    pub fn info(&self) -> &dyn LintRule {
        match self {
            Self::Source(rule) => &**rule,
            Self::Document(rule) => &**rule,
        }
    }
}

/// A rule with this id is already registered.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::rules::{DuplicateKeysRule, Rule, RuleRegistry};
///
/// let mut registry = RuleRegistry::new();
/// registry.add(Rule::Source(Box::new(DuplicateKeysRule))).unwrap();
/// let error = registry.add(Rule::Source(Box::new(DuplicateKeysRule))).err().unwrap();
/// assert_eq!(error.id.to_string(), "duplicate-key");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a rule with the id '{id}' is already registered")]
pub struct DuplicateRule {
    /// The id of the rule that is registered twice.
    pub id: OwnedRuleId,
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
    rules: Vec<Rule>,
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
    pub const fn new() -> Self {
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
    /// - Indentation (WARNING)
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::rules::RuleRegistry;
    ///
    /// let registry = RuleRegistry::with_default_rules();
    /// assert_eq!(registry.rules().len(), 25);
    /// ```
    #[must_use]
    pub fn with_default_rules() -> Self {
        Self {
            rules: crate::config::default_rules(),
        }
    }

    /// Adds a rule to the registry.
    ///
    /// # Errors
    ///
    /// Returns [`DuplicateRule`] when a rule with the same id is registered, since both would
    /// report the same findings under one set of settings.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::rules::{DuplicateKeysRule, Rule, RuleRegistry};
    ///
    /// let mut registry = RuleRegistry::new();
    /// registry.add(Rule::Source(Box::new(DuplicateKeysRule))).unwrap();
    /// assert_eq!(registry.rules().len(), 1);
    /// ```
    pub fn add(&mut self, rule: Rule) -> Result<&mut Self, DuplicateRule> {
        let id = rule.info().id();
        if self.rules.iter().any(|known| known.info().id() == id) {
            return Err(DuplicateRule {
                id: id.to_owned_id(),
            });
        }
        self.rules.push(rule);
        Ok(self)
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
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Gets a rule by id.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::config::RuleName;
    /// use fast_yaml_linter::rules::{RuleId, RuleRegistry};
    ///
    /// let registry = RuleRegistry::with_default_rules();
    /// assert!(registry.get(RuleId::BuiltIn(RuleName::DuplicateKey)).is_some());
    /// ```
    #[must_use]
    pub fn get(&self, id: RuleId<'_>) -> Option<&Rule> {
        self.rules.iter().find(|r| r.info().id() == id)
    }

    /// Gets a rule by the code written in untrusted text, such as an inline directive.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::rules::RuleRegistry;
    ///
    /// let registry = RuleRegistry::with_default_rules();
    /// assert!(registry.find_by_code("duplicate-key").is_some());
    /// assert!(registry.find_by_code("nonexistent").is_none());
    /// ```
    #[must_use]
    pub fn find_by_code(&self, code: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.info().id().as_str() == code)
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
        assert_eq!(registry.rules().len(), 25);
    }

    #[test]
    fn test_registry_add() {
        let mut registry = RuleRegistry::new();
        registry
            .add(Rule::Source(Box::new(DuplicateKeysRule)))
            .unwrap();
        assert_eq!(registry.rules().len(), 1);
    }

    #[test]
    fn test_registry_rejects_a_duplicate_id() {
        let mut registry = RuleRegistry::with_default_rules();
        let error = registry
            .add(Rule::Source(Box::new(DuplicateKeysRule)))
            .err()
            .unwrap();
        assert_eq!(error.id, OwnedRuleId::BuiltIn(RuleName::DuplicateKey));
        assert_eq!(registry.rules().len(), 25);
    }

    #[test]
    fn test_registry_get() {
        let registry = RuleRegistry::with_default_rules();
        let rule = registry.get(RuleId::BuiltIn(RuleName::DuplicateKey));
        assert!(rule.is_some());
        assert_eq!(rule.unwrap().info().id().as_str(), "duplicate-key");
    }

    #[test]
    fn test_registry_get_missing() {
        let registry = RuleRegistry::with_default_rules();
        assert!(registry.find_by_code("nonexistent").is_none());
    }

    #[test]
    fn test_registry_default() {
        let registry = RuleRegistry::default();
        assert_eq!(registry.rules().len(), 25);
    }
}
