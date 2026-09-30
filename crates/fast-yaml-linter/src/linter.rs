//! Main linter engine and configuration.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::str::FromStr;

use crate::config::{CustomRuleCode, IndentSize, NoOptions, RuleName, RuleSettings, RulesConfig};
use crate::context::lines_of;
use crate::directives::Directives;
use crate::rules::{DocumentEndPresence, DocumentStartPresence};
use crate::{Diagnostic, LintContext, Severity, rules::RuleRegistry};
use fast_yaml_core::limits::ParseLimits;
use fast_yaml_core::{Parser, ScalarOwned, Value};

/// Configuration for the linter.
///
/// Holds the typed settings of every built-in rule plus the enablement and severity of
/// custom rules added with [`Linter::add_rule`].
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::LintConfig;
///
/// let config = LintConfig::default();
/// assert_eq!(config.rules.line_length.options.max.map(|max| max.get()), Some(80));
/// assert_eq!(config.rules.indentation.options.indent_size.get(), 2);
/// ```
#[derive(Debug, Clone, Default)]
pub struct LintConfig {
    /// Settings of the built-in rules.
    pub rules: RulesConfig,
    /// Settings of custom rules by code.
    pub custom_rules: HashMap<CustomRuleCode, RuleSettings<NoOptions>>,
    /// Resource limits applied when parsing the source.
    pub parse_limits: ParseLimits,
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
    /// assert!(config.rules.line_length.enabled);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum line length (`None` removes the limit).
    ///
    /// Does not change whether the rule is enabled.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::num::NonZeroUsize;
    /// use fast_yaml_linter::LintConfig;
    ///
    /// let config = LintConfig::new().with_max_line_length(NonZeroUsize::new(120));
    /// assert_eq!(config.rules.line_length.options.max, NonZeroUsize::new(120));
    /// ```
    #[must_use]
    pub const fn with_max_line_length(mut self, max: Option<NonZeroUsize>) -> Self {
        self.rules.line_length.options.max = max;
        self
    }

    /// Sets the expected indentation size.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintConfig;
    /// use fast_yaml_linter::config::IndentSize;
    ///
    /// let config = LintConfig::new().with_indent_size(IndentSize::try_from(4u64).unwrap());
    /// assert_eq!(config.rules.indentation.options.indent_size.get(), 4);
    /// ```
    #[must_use]
    pub const fn with_indent_size(mut self, size: IndentSize) -> Self {
        self.rules.indentation.options.indent_size = size;
        self
    }

    /// Sets whether the document start marker (`---`) is required, forbidden or allowed.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintConfig;
    /// use fast_yaml_linter::rules::DocumentStartPresence;
    ///
    /// let config = LintConfig::new().with_document_start(DocumentStartPresence::Required);
    /// assert_eq!(
    ///     config.rules.document_start.options.present,
    ///     DocumentStartPresence::Required
    /// );
    /// ```
    #[must_use]
    pub const fn with_document_start(mut self, presence: DocumentStartPresence) -> Self {
        self.rules.document_start.options.present = presence;
        self
    }

    /// Sets the parser resource limits.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::limits::{MaxDepth, ParseLimits};
    /// use fast_yaml_linter::{LintConfig, Linter};
    ///
    /// let limits = ParseLimits { max_depth: MaxDepth::new(2).unwrap(), ..ParseLimits::default() };
    /// let linter = Linter::with_config(LintConfig::new().with_parse_limits(limits));
    /// assert!(linter.lint("[[[1]]]").is_err());
    /// ```
    #[must_use]
    pub const fn with_parse_limits(mut self, limits: ParseLimits) -> Self {
        self.parse_limits = limits;
        self
    }

    /// Sets whether the document end marker (`...`) is required or merely allowed.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintConfig;
    /// use fast_yaml_linter::rules::DocumentEndPresence;
    ///
    /// let config = LintConfig::new().with_document_end(DocumentEndPresence::Required);
    /// assert_eq!(config.rules.document_end.options.present, DocumentEndPresence::Required);
    /// ```
    #[must_use]
    pub const fn with_document_end(mut self, presence: DocumentEndPresence) -> Self {
        self.rules.document_end.options.present = presence;
        self
    }

    /// Disables a built-in rule.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintConfig;
    /// use fast_yaml_linter::config::RuleName;
    ///
    /// let config = LintConfig::new().with_disabled_rule(RuleName::LineLength);
    /// assert!(!config.is_rule_enabled("line-length"));
    /// assert!(config.is_rule_enabled("duplicate-key"));
    /// ```
    #[must_use]
    pub const fn with_disabled_rule(mut self, rule: RuleName) -> Self {
        self.rules.set_enabled(rule, false);
        self
    }

    /// Sets the enablement and severity of a custom rule.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{LintConfig, Severity};
    /// use fast_yaml_linter::config::{CustomRuleCode, NoOptions, RuleSettings};
    ///
    /// let settings = RuleSettings::<NoOptions> {
    ///     enabled: false,
    ///     severity: Some(Severity::Error),
    ///     options: NoOptions::default(),
    /// };
    /// let config = LintConfig::new()
    ///     .with_custom_rule(CustomRuleCode::new("my-rule").unwrap(), settings);
    /// assert!(!config.is_rule_enabled("my-rule"));
    /// assert_eq!(config.severity_for("my-rule", Severity::Hint), Severity::Error);
    /// ```
    #[must_use]
    pub fn with_custom_rule(
        mut self,
        code: CustomRuleCode,
        settings: RuleSettings<NoOptions>,
    ) -> Self {
        self.custom_rules.insert(code, settings);
        self
    }

    /// Returns whether the rule with this code is enabled.
    ///
    /// Built-in rules are looked up by [`RuleName`], then custom rules; unknown codes are
    /// enabled.
    #[must_use]
    pub fn is_rule_enabled(&self, code: &str) -> bool {
        RuleName::from_str(code).map_or_else(
            |_| {
                self.custom_settings(code)
                    .is_none_or(|settings| settings.enabled)
            },
            |name| self.rules.is_enabled(name),
        )
    }

    /// Returns the configured severity of a rule, or `default` when none is set.
    ///
    /// Built-in rules are resolved through [`RulesConfig`], custom rules through
    /// [`LintConfig::custom_rules`]. Custom [`LintRule`](crate::rules::LintRule) implementations call this with their own
    /// code; built-in rules read their typed settings directly.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{LintConfig, Severity};
    ///
    /// let config = LintConfig::new();
    /// assert_eq!(config.severity_for("my-rule", Severity::Info), Severity::Info);
    /// ```
    #[must_use]
    pub fn severity_for(&self, code: &str, default: Severity) -> Severity {
        RuleName::from_str(code).map_or_else(
            |_| {
                self.custom_settings(code)
                    .map_or(default, |settings| settings.severity_or(default))
            },
            |name| self.rules.severity(name).unwrap_or(default),
        )
    }

    fn custom_settings(&self, code: &str) -> Option<&RuleSettings<NoOptions>> {
        self.custom_rules.get(code)
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
    /// use fast_yaml_linter::config::IndentSize;
    ///
    /// let config = LintConfig::new().with_indent_size(IndentSize::try_from(4u64).unwrap());
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
    /// use std::num::NonZeroUsize;
    /// use fast_yaml_linter::{Linter, LintConfig};
    ///
    /// let config = LintConfig::new().with_max_line_length(NonZeroUsize::new(120));
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
    /// Parses the source and runs all enabled rules, then applies inline suppression
    /// directives (`# fy: disable`, `# fy: enable`, `# fy: disable-line`, `# fy: disable-file`,
    /// also spelled `# yamllint ...`). Problems in directives are reported as `lint-directive`
    /// diagnostics.
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
        let (source, bom_len) = split_bom(source);
        let docs = Parser::parse_all_with_limits(source, &self.config.parse_limits)?;
        let doc_start_lines = compute_doc_start_lines(source, docs.len());
        let directives = Directives::from_source(source, &self.config, &self.registry);
        let mut context = LintContext::new(source);
        let mut diagnostics = Vec::new();

        for rule in self.registry.rules() {
            if directives.disables_file() {
                break;
            }
            if !self.config.is_rule_enabled(rule.code()) {
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

        Ok(finish(diagnostics, directives, bom_len))
    }

    /// Lints a pre-parsed Value (avoids double parsing).
    ///
    /// Use this when you already have a parsed YAML value.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Linter;
    /// use fast_yaml_core::Parser;
    ///
    /// let yaml = "name: John";
    /// let value = Parser::parse_str(yaml).unwrap().unwrap();
    ///
    /// let linter = Linter::with_all_rules();
    /// let diagnostics = linter.lint_value(yaml, &value).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `LintError::ParseError` if `source` contains a NUL character, which the
    /// tokenizer would otherwise treat as end of input.
    pub fn lint_value(&self, source: &str, value: &Value) -> Result<Vec<Diagnostic>, LintError> {
        let (source, bom_len) = split_bom(source);
        fast_yaml_core::reject_nul(source)?;
        let directives = Directives::from_source(source, &self.config, &self.registry);
        let context = LintContext::new(source);
        let mut diagnostics = Vec::new();

        for rule in self.registry.rules() {
            if directives.disables_file() {
                break;
            }
            if !self.config.is_rule_enabled(rule.code()) {
                continue;
            }

            let mut rule_diagnostics = rule.check(&context, value, &self.config);
            diagnostics.append(&mut rule_diagnostics);
        }

        Ok(finish(diagnostics, directives, bom_len))
    }

    /// Gets the current configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{Linter, LintConfig};
    /// use fast_yaml_linter::config::RuleName;
    ///
    /// let config = LintConfig::new().with_disabled_rule(RuleName::Colons);
    /// let linter = Linter::with_config(config);
    ///
    /// assert!(!linter.config().rules.colons.enabled);
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
#[non_exhaustive]
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

    for (idx, line) in lines_of(source).enumerate() {
        if line == "---" {
            starts.push(idx + 2); // line after `---` (1-based)
            if starts.len() == doc_count {
                break;
            }
        }
    }

    starts
}

/// Applies inline directives, rebases spans onto the original file and sorts.
fn finish(
    mut diagnostics: Vec<Diagnostic>,
    directives: Directives,
    bom_len: usize,
) -> Vec<Diagnostic> {
    directives.apply(&mut diagnostics);
    shift_offsets(&mut diagnostics, bom_len);
    diagnostics.sort_by_key(|d| d.span.start);
    diagnostics
}

/// Strips a leading BOM and returns the stripped text with the BOM's byte length.
fn split_bom(source: &str) -> (&str, usize) {
    let stripped = fast_yaml_core::strip_bom(source);
    (stripped, source.len() - stripped.len())
}

/// Rebases span offsets onto the original file, which still contains the BOM.
fn shift_offsets(diagnostics: &mut [Diagnostic], bom_len: usize) {
    if bom_len == 0 {
        return;
    }
    let spans = diagnostics.iter_mut().flat_map(|d| {
        std::iter::once(&mut d.span).chain(d.suggestions.iter_mut().map(|s| &mut s.span))
    });
    for span in spans {
        span.start.offset += bom_len;
        span.end.offset += bom_len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_support::config_with_rule;
    use crate::rules::LintRule;
    use std::fmt::Write as _;

    fn indent(size: u64) -> IndentSize {
        IndentSize::try_from(size).unwrap()
    }

    struct AlwaysFlags;

    impl LintRule for AlwaysFlags {
        fn code(&self) -> &'static str {
            "always-flags"
        }

        fn name(&self) -> &'static str {
            "Always Flags"
        }

        fn description(&self) -> &'static str {
            "Flags every document"
        }

        fn default_severity(&self) -> Severity {
            Severity::Hint
        }

        fn check(
            &self,
            context: &LintContext,
            _value: &Value,
            config: &LintConfig,
        ) -> Vec<Diagnostic> {
            let span = context
                .source_context()
                .span_at(context.source_context().line_start(1), 1);
            vec![
                crate::DiagnosticBuilder::new(
                    self.code(),
                    config.severity_for(self.code(), self.default_severity()),
                    "flagged",
                    span,
                )
                .build_with_context(context.source_context()),
            ]
        }
    }

    #[test]
    fn test_custom_rule_runs_with_default_severity() {
        let mut linter = Linter::with_config(LintConfig::new());
        linter.add_rule(Box::new(AlwaysFlags));
        let diagnostics = linter.lint("a: 1\n").unwrap();
        let flagged: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code.as_str() == "always-flags")
            .collect();
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].severity, Severity::Hint);
    }

    #[test]
    fn test_custom_rule_severity_override_and_disable() {
        let code = CustomRuleCode::new("always-flags").unwrap();
        let overridden = RuleSettings::<NoOptions> {
            severity: Some(Severity::Error),
            ..RuleSettings::default()
        };
        let mut linter =
            Linter::with_config(LintConfig::new().with_custom_rule(code.clone(), overridden));
        linter.add_rule(Box::new(AlwaysFlags));
        let diagnostics = linter.lint("a: 1\n").unwrap();
        let flagged = diagnostics
            .iter()
            .find(|d| d.code.as_str() == "always-flags")
            .unwrap();
        assert_eq!(flagged.severity, Severity::Error);

        let disabled = RuleSettings::<NoOptions> {
            enabled: false,
            ..RuleSettings::default()
        };
        let mut linter = Linter::with_config(LintConfig::new().with_custom_rule(code, disabled));
        linter.add_rule(Box::new(AlwaysFlags));
        let diagnostics = linter.lint("a: 1\n").unwrap();
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code.as_str() == "always-flags")
        );
    }

    #[test]
    fn test_builders_do_not_change_enablement() {
        let config = LintConfig::new()
            .with_disabled_rule(RuleName::LineLength)
            .with_max_line_length(NonZeroUsize::new(10));
        assert!(!config.rules.line_length.enabled);
        assert_eq!(config.rules.line_length.options.max, NonZeroUsize::new(10));
    }

    #[test]
    fn test_allow_duplicate_keys_can_be_reenabled_by_rules() {
        let mut config = LintConfig::new().with_disabled_rule(RuleName::DuplicateKey);
        config
            .rules
            .apply(serde_norway::Deserializer::from_str(
                "duplicate-key: {enabled: true}",
            ))
            .unwrap();
        let diagnostics = Linter::with_config(config).lint("k: 1\nk: 2\n").unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_str() == "duplicate-key")
        );
    }

    #[test]
    fn test_issue_324_document_start_present_true_is_enforced() {
        let config = config_with_rule(RuleName::DocumentStart, "{present: true}");
        let diagnostics = Linter::with_config(config).lint("a: 1\n").unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_str() == "document-start")
        );
    }

    #[test]
    fn test_lint_rejects_deeply_nested_input() {
        let input = format!("{}x", "- ".repeat(20_000));
        let err = Linter::with_all_rules().lint(&input).unwrap_err();
        assert!(matches!(
            err,
            LintError::ParseError(fast_yaml_core::ParseError::LimitExceeded { .. })
        ));
    }

    fn run_on_2mib_stack(f: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(f)
            .unwrap()
            .join()
            .unwrap();
    }

    fn lint_at_max_depth(input: &str) {
        let limits = ParseLimits {
            max_depth: fast_yaml_core::limits::MaxDepth::MAX,
            ..ParseLimits::default()
        };
        let linter = Linter::with_config(LintConfig::new().with_parse_limits(limits));
        assert!(linter.lint(input).is_ok());
    }

    #[test]
    fn test_lint_nested_maps_at_max_depth_on_2mib_stack() {
        run_on_2mib_stack(|| {
            let depth = fast_yaml_core::limits::MaxDepth::MAX.get();
            let mut input = String::new();
            for i in 0..depth {
                writeln!(input, "{}k:", " ".repeat(i)).unwrap();
            }
            input.push_str(&" ".repeat(depth));
            input.push('v');
            lint_at_max_depth(&input);
        });
    }

    #[test]
    fn test_lint_tagged_seq_at_max_depth_on_2mib_stack() {
        run_on_2mib_stack(|| {
            let depth = fast_yaml_core::limits::MaxDepth::MAX.get();
            let mut input = String::new();
            for i in 0..depth {
                writeln!(input, "{}- !t", "  ".repeat(i)).unwrap();
            }
            input.push_str(&"  ".repeat(depth));
            input.push('x');
            lint_at_max_depth(&input);
        });
    }

    #[test]
    fn test_lint_honors_parse_limits() {
        let limits = ParseLimits {
            max_depth: fast_yaml_core::limits::MaxDepth::new(2).unwrap(),
            ..ParseLimits::default()
        };
        let linter = Linter::with_config(LintConfig::new().with_parse_limits(limits));
        assert!(linter.lint("[[1]]").is_ok());
        assert!(matches!(
            linter.lint("[[[1]]]"),
            Err(LintError::ParseError(
                fast_yaml_core::ParseError::LimitExceeded { .. }
            ))
        ));
    }

    #[test]
    fn test_lint_rejects_tag_prefix_amplification() {
        let mut input = format!("%TAG !e! tag:e.com,{}\n---\n", "a".repeat(100_000));
        input.extend((0..1_000).map(|i| format!("k{i}: !e!x v\n")));
        let err = Linter::with_all_rules().lint(&input).unwrap_err();
        assert!(matches!(
            err,
            LintError::ParseError(fast_yaml_core::ParseError::LimitExceeded {
                kind: fast_yaml_core::LimitKind::TagBytes(_),
                ..
            })
        ));
    }

    #[test]
    fn test_config_default() {
        let config = LintConfig::default();
        assert_eq!(config.rules.line_length.options.max, NonZeroUsize::new(80));
        assert_eq!(config.rules.indentation.options.indent_size.get(), 2);
        assert_eq!(
            config.rules.document_start.options.present,
            DocumentStartPresence::Allowed
        );
        assert!(config.rules.duplicate_key.enabled);
    }

    #[test]
    fn test_config_builder() {
        let config = LintConfig::new()
            .with_max_line_length(NonZeroUsize::new(120))
            .with_indent_size(indent(4));

        assert_eq!(config.rules.line_length.options.max, NonZeroUsize::new(120));
        assert_eq!(config.rules.indentation.options.indent_size.get(), 4);
    }

    #[test]
    fn test_config_disabled_rules() {
        let config = LintConfig::new().with_disabled_rule(RuleName::LineLength);

        assert!(!config.is_rule_enabled("line-length"));
        assert!(config.is_rule_enabled("duplicate-key"));
    }

    #[test]
    fn test_linter_new() {
        let linter = Linter::new();
        assert!(linter.registry().rules().is_empty());
    }

    #[test]
    fn test_linter_with_all_rules() {
        let linter = Linter::with_all_rules();
        assert_eq!(linter.registry().rules().len(), 24);
    }

    #[test]
    fn test_linter_with_config() {
        let config = LintConfig::new().with_indent_size(indent(4));
        let linter = Linter::with_config(config);
        assert_eq!(
            linter.config().rules.indentation.options.indent_size.get(),
            4
        );
        assert!(!linter.registry().rules().is_empty());
    }

    #[test]
    fn test_linter_with_config_detects_duplicate_keys() {
        let yaml = "key: 1\nkey: 2\n";
        let linter = Linter::with_config(LintConfig::new());
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
        let diagnostics = linter.lint_value(yaml, &value).unwrap();

        assert!(
            diagnostics
                .iter()
                .all(|d| d.severity != crate::Severity::Error)
        );
    }

    #[test]
    fn test_linter_disabled_rule() {
        let yaml = "very_long_line: this line is definitely longer than eighty characters and should trigger a warning";
        let config = LintConfig::new().with_disabled_rule(RuleName::LineLength);
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
        let linter = Linter::with_all_rules_and_config(LintConfig::new());
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
    fn test_require_document_start_missing() {
        let yaml = "key: value\n";
        let config = LintConfig::new().with_document_start(DocumentStartPresence::Required);
        let linter = Linter::with_all_rules_and_config(config);
        let diagnostics = linter.lint(yaml).unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_str() == crate::DiagnosticCode::DOCUMENT_START),
            "require_document_start=true must produce a diagnostic when '---' is absent"
        );
    }

    #[test]
    fn test_require_document_start_present() {
        let yaml = "---\nkey: value\n";
        let config = LintConfig::new().with_document_start(DocumentStartPresence::Required);
        let linter = Linter::with_all_rules_and_config(config);
        let diagnostics = linter.lint(yaml).unwrap();
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code.as_str() == crate::DiagnosticCode::DOCUMENT_START),
            "require_document_start=true must not produce a diagnostic when '---' is present"
        );
    }

    #[test]
    fn test_require_document_end_missing() {
        let yaml = "key: value\n";
        let config = LintConfig::new().with_document_end(DocumentEndPresence::Required);
        let linter = Linter::with_all_rules_and_config(config);
        let diagnostics = linter.lint(yaml).unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_str() == crate::DiagnosticCode::DOCUMENT_END),
            "require_document_end=true must produce a diagnostic when '...' is absent"
        );
    }

    #[test]
    fn test_require_document_end_present() {
        let yaml = "key: value\n...\n";
        let config = LintConfig::new().with_document_end(DocumentEndPresence::Required);
        let linter = Linter::with_all_rules_and_config(config);
        let diagnostics = linter.lint(yaml).unwrap();
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code.as_str() == crate::DiagnosticCode::DOCUMENT_END),
            "require_document_end=true must not produce a diagnostic when '...' is present"
        );
    }

    #[test]
    fn test_document_start_forbidden_flags_existing_marker() {
        let yaml = "---\nkey: value\n";
        let config = config_with_rule(RuleName::DocumentStart, "{present: forbidden}");
        let linter = Linter::with_all_rules_and_config(config);
        let diagnostics = linter.lint(yaml).unwrap();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_str() == crate::DiagnosticCode::DOCUMENT_START),
            "present=forbidden must flag an existing '---'"
        );
    }

    #[test]
    fn test_linter_rule_config_disabled_suppresses_rule() {
        // Regression test for #133: a rule disabled through configuration must not run.
        let yaml = "very_long_line: this line is definitely longer than eighty characters and should trigger a warning";
        let config = config_with_rule(RuleName::LineLength, "disable");
        let linter = Linter::with_config(config);

        let mut linter = linter;
        linter.add_rule(Box::new(crate::rules::LineLengthRule));

        let diagnostics = linter.lint(yaml).unwrap();

        assert!(
            !diagnostics.iter().any(|d| d.code.as_str() == "line-length"),
            "a disabled rule should not produce diagnostics"
        );
    }

    #[test]
    fn test_lint_bom_input_keeps_line_column_and_shifts_offset() {
        let linter = Linter::with_all_rules();
        let plain = linter.lint("# c\na: 1").unwrap();
        let bom = linter.lint("\u{FEFF}# c\na: 1").unwrap();
        assert!(!plain.is_empty());
        assert_eq!(plain.len(), bom.len());
        for (p, b) in plain.iter().zip(&bom) {
            assert_eq!(p.code, b.code);
            assert_eq!(
                (p.span.start.line, p.span.start.column),
                (b.span.start.line, b.span.start.column)
            );
            assert_eq!(b.span.start.offset, p.span.start.offset + 3);
            assert_eq!(b.span.end.offset, p.span.end.offset + 3);
        }
    }

    #[test]
    fn test_lint_value_rejects_nul() {
        let value = Parser::parse_str("a: 1").unwrap().unwrap();
        assert!(
            Linter::with_all_rules()
                .lint_value("a: 1\0\nb: 2", &value)
                .is_err()
        );
    }

    #[test]
    fn test_lint_value_bom_shifts_offset() {
        let linter = Linter::with_all_rules();
        let src = "a: 1";
        let value = fast_yaml_core::Parser::parse_str(src).unwrap().unwrap();
        let plain = linter.lint_value(src, &value).unwrap();
        let bom = linter.lint_value("\u{FEFF}a: 1", &value).unwrap();
        assert!(!plain.is_empty());
        assert_eq!(bom[0].span.start.offset, plain[0].span.start.offset + 3);
    }

    #[test]
    fn test_lint_bom_before_mapping_no_error() {
        let diagnostics = Linter::with_all_rules().lint("\u{FEFF}a: 1\n").unwrap();
        assert!(diagnostics.is_empty(), "unexpected: {diagnostics:?}");
    }
}
