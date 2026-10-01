//! Main linter engine and configuration.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::str::FromStr;

use crate::config::{
    CanonicalPath, CustomRuleCode, IndentSize, NoOptions, RuleName, RuleSettings, RulesConfig,
};
use crate::directives::Directives;
use crate::rules::{LintRule, MarkerPresence};
use crate::scan::{ScanCollector, ScanNeeds, SourceScan, lint_load_options};
use crate::{Diagnostic, DiagnosticCode, LintContext, LintSource, Severity, rules::RuleRegistry};
use fast_yaml_core::limits::{InputTooLarge, MaxInputBytes, ParseLimits, StreamBudget};
use fast_yaml_core::{NormalizedInput, Parser, Value};

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
/// assert_eq!(config.rules.indentation.options.indent_size().get(), 2);
/// ```
#[derive(Debug, Clone, Default)]
pub struct LintConfig {
    /// Settings of the built-in rules.
    pub rules: RulesConfig,
    /// Settings of custom rules by code.
    pub custom_rules: HashMap<CustomRuleCode, RuleSettings<NoOptions>>,
    /// Resource limits applied when parsing the source.
    pub parse_limits: ParseLimits,
    /// Largest source accepted for linting; bounds work on oversized input, not memory.
    pub max_input_bytes: MaxInputBytes,
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
    /// assert_eq!(config.rules.indentation.options.indent_size().get(), 4);
    /// ```
    #[must_use]
    pub const fn with_indent_size(mut self, size: IndentSize) -> Self {
        self.rules.indentation.options.indent_size = Some(size);
        self
    }

    /// Sets whether the document start marker (`---`) is required, forbidden or allowed.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintConfig;
    /// use fast_yaml_linter::rules::MarkerPresence;
    ///
    /// let config = LintConfig::new().with_document_start(MarkerPresence::Required);
    /// assert_eq!(
    ///     config.rules.document_start.options.present,
    ///     MarkerPresence::Required
    /// );
    /// ```
    #[must_use]
    pub const fn with_document_start(mut self, presence: MarkerPresence) -> Self {
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

    /// Sets the largest source, in bytes, accepted for linting.
    ///
    /// Bounds the work done on oversized input. The source is already in memory when the
    /// check runs, so this is not a memory bound.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::limits::MaxInputBytes;
    /// use fast_yaml_linter::{LintConfig, Linter};
    ///
    /// let max = MaxInputBytes::new(8).unwrap();
    /// let linter = Linter::with_config(LintConfig::new().with_max_input_bytes(max));
    /// assert!(linter.lint("a: 1\n").is_ok());
    /// assert!(linter.lint("a: 1\nb: 2\n").is_err());
    /// ```
    #[must_use]
    pub const fn with_max_input_bytes(mut self, max: MaxInputBytes) -> Self {
        self.max_input_bytes = max;
        self
    }

    /// Sets whether the document end marker (`...`) is required or merely allowed.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::LintConfig;
    /// use fast_yaml_linter::rules::MarkerPresence;
    ///
    /// let config = LintConfig::new().with_document_end(MarkerPresence::Required);
    /// assert_eq!(config.rules.document_end.options.present, MarkerPresence::Required);
    /// ```
    #[must_use]
    pub const fn with_document_end(mut self, presence: MarkerPresence) -> Self {
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
    ///     ..RuleSettings::default()
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

    /// Returns whether the built-in rule runs for the file at `path`, or for a source without
    /// a path.
    ///
    /// A rule runs when it is enabled and its own `ignore` patterns do not match `path`. With
    /// no path nothing is ignored, as in yamllint for standard input.
    #[must_use]
    pub fn is_active(&self, name: RuleName, path: Option<&CanonicalPath>) -> bool {
        self.rules.is_enabled(name) && path.is_none_or(|path| !self.rules.is_ignored(name, path))
    }

    /// Like [`LintConfig::is_active`] for a registry code, which may name a custom rule.
    fn is_code_active(&self, code: &str, path: Option<&CanonicalPath>) -> bool {
        RuleName::from_str(code).map_or_else(
            |_| self.is_rule_enabled(code),
            |name| self.is_active(name, path),
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
    /// Every location (line, column, byte offset, suggestion spans) refers to `source` with
    /// document-prefix BOMs removed, the same text the parser reports syntax errors against; use
    /// [`NormalizedInput::original_offset`] to map an offset back to `source`.
    ///
    /// # Errors
    ///
    /// Returns `LintError::InputTooLarge` if `source` exceeds [`LintConfig::max_input_bytes`],
    /// and `LintError::ParseError` if the YAML cannot be parsed.
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
        self.lint_source(&self.source(source)?)
    }

    /// Lints the source of the file at `path`, skipping the rules whose own `ignore` patterns
    /// match it.
    ///
    /// Otherwise like [`Linter::lint`]. Per-rule `ignore` and `ignore-from-file` come from the
    /// config file, or from [`RulesConfig::apply_at`].
    ///
    /// # Errors
    ///
    /// Same as [`Linter::lint`].
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::config::{CanonicalPath, RulesConfig};
    /// use fast_yaml_linter::{LintConfig, Linter};
    ///
    /// let dir = std::env::temp_dir().canonicalize().unwrap();
    /// let mut rules = RulesConfig::default();
    /// let yaml = "trailing-whitespace: {ignore: 'generated/'}";
    /// rules.apply_at(serde_norway::Deserializer::from_str(yaml), &dir).unwrap();
    /// let linter = Linter::with_config(LintConfig { rules, ..LintConfig::default() });
    ///
    /// let source = "a: 1 \n";
    /// let kept = CanonicalPath::assume_canonical(dir.join("src/a.yaml"));
    /// let skipped = CanonicalPath::assume_canonical(dir.join("generated/a.yaml"));
    /// assert!(linter.lint_file(source, &kept).unwrap().iter().any(|d| d.code.as_str() == "trailing-whitespace"));
    /// assert!(linter.lint_file(source, &skipped).unwrap().iter().all(|d| d.code.as_str() != "trailing-whitespace"));
    /// ```
    pub fn lint_file(
        &self,
        source: &str,
        path: &CanonicalPath,
    ) -> Result<Vec<Diagnostic>, LintError> {
        self.lint_source_file(&self.source(source)?, path)
    }

    /// Validates `raw` for linting: checks [`LintConfig::max_input_bytes`] first, then strips
    /// prefix byte order marks.
    ///
    /// # Errors
    ///
    /// Returns `LintError::InputTooLarge` if `raw` exceeds the configured limit, and
    /// `LintError::ParseError` if it contains a character YAML does not allow.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::Linter;
    ///
    /// let linter = Linter::with_all_rules();
    /// let source = linter.source("a: 1\n").unwrap();
    /// assert!(linter.lint_source(&source).is_ok());
    /// ```
    pub fn source<'a>(&self, raw: &'a str) -> Result<LintSource<'a>, LintError> {
        self.config.max_input_bytes.check(raw.len())?;
        LintSource::new(raw)
    }

    /// Lints source text that is already validated.
    ///
    /// Use this with [`LintSource`] when the caller also prints excerpts of the text, so the
    /// input is validated and stripped of byte order marks once.
    ///
    /// # Errors
    ///
    /// Same as [`Linter::lint`].
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::{LintSource, Linter};
    ///
    /// let source = LintSource::new("name: John\n").unwrap();
    /// let diagnostics = Linter::with_all_rules().lint_source(&source).unwrap();
    /// ```
    pub fn lint_source(&self, input: &LintSource<'_>) -> Result<Vec<Diagnostic>, LintError> {
        self.run(input, None)
    }

    /// Lints source text that is already validated, as the file at `path`.
    ///
    /// The [`LintSource`] counterpart of [`Linter::lint_file`].
    ///
    /// # Errors
    ///
    /// Same as [`Linter::lint`].
    pub fn lint_source_file(
        &self,
        input: &LintSource<'_>,
        path: &CanonicalPath,
    ) -> Result<Vec<Diagnostic>, LintError> {
        self.run(input, Some(path))
    }

    fn run(
        &self,
        input: &LintSource<'_>,
        path: Option<&CanonicalPath>,
    ) -> Result<Vec<Diagnostic>, LintError> {
        self.config.max_input_bytes.check(input.original_len())?;
        let normalized = input.normalized();
        let source = normalized.as_str();
        let context = LintContext::new(source).with_parse_limits(self.config.parse_limits);
        let mut collector = ScanCollector::new(
            normalized,
            source,
            context.source_context(),
            self.scan_needs(path),
        );
        let docs = Parser::parse_normalized_observed(
            normalized,
            &StreamBudget::new(self.config.parse_limits),
            lint_load_options(),
            |item| collector.observe(item),
        )?;
        let scan = collector.finish();
        let mut context = context.with_scan(scan);
        let directives = Directives::from_context(&context, &self.config, &self.registry);
        let rules: Vec<&dyn LintRule> = if directives.disables_file() {
            Vec::new()
        } else {
            self.registry
                .rules()
                .iter()
                .map(AsRef::as_ref)
                .filter(|rule| self.config.is_code_active(rule.code(), path))
                .collect()
        };

        // The rules that read the documents run first, so the documents (the bulk of the heap)
        // are freed before the other rules allocate; results are merged in registry order.
        let mut by_value: Vec<Option<Vec<Diagnostic>>> = rules.iter().map(|_| None).collect();
        for (slot, rule) in by_value.iter_mut().zip(&rules) {
            if !rule.needs_value() {
                continue;
            }
            let mut found = Vec::new();
            for (idx, doc) in docs.iter().enumerate() {
                let start_line = context.documents().get(idx).map_or(1, |d| d.first_line);
                context.set_doc_start_line(start_line);
                found.extend(rule.check(&context, doc, &self.config));
            }
            *slot = Some(found);
        }
        context.set_doc_start_line(1);
        drop(docs);

        let mut diagnostics = Vec::new();
        for (slot, rule) in by_value.iter_mut().zip(&rules) {
            let mut found = slot
                .take()
                .unwrap_or_else(|| rule.check(&context, &Value::Null, &self.config));
            if diagnostics.is_empty() {
                diagnostics = found;
            } else {
                diagnostics.append(&mut found);
            }
        }

        let mut diagnostics = finish(diagnostics, directives);
        if path.is_some_and(|path| self.config.rules.is_ignored(RuleName::LintDirective, path)) {
            diagnostics.retain(|d| d.code.as_str() != DiagnosticCode::LINT_DIRECTIVE);
        }
        Ok(diagnostics)
    }

    /// The scan products the active rules read.
    fn scan_needs(&self, path: Option<&CanonicalPath>) -> ScanNeeds {
        ScanNeeds::of_rules(
            self.registry
                .rules()
                .iter()
                .map(|rule| rule.code())
                .filter(|code| self.config.is_code_active(code, path)),
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
    /// use fast_yaml_core::Parser;
    ///
    /// let yaml = "name: John";
    /// let value = Parser::parse_str(yaml).unwrap().unwrap();
    ///
    /// let linter = Linter::with_all_rules();
    /// let diagnostics = linter.lint_value(yaml, &value).unwrap();
    /// ```
    ///
    /// Locations use the same BOM-free coordinates as [`Linter::lint`].
    ///
    /// Comments and document markers are read from `source` itself under
    /// [`LintConfig::parse_limits`], so `source` must load.
    ///
    /// # Errors
    ///
    /// Returns `LintError::InputTooLarge` if `source` exceeds [`LintConfig::max_input_bytes`],
    /// and `LintError::ParseError` if `source` does not load (it exceeds the parse limits or is
    /// not valid YAML) or contains a NUL character, which the
    /// tokenizer would otherwise treat as end of input, or if the scanner reads more than
    /// [`ParseLimits::max_scan_ahead`] past a node.
    pub fn lint_value(&self, source: &str, value: &Value) -> Result<Vec<Diagnostic>, LintError> {
        self.config.max_input_bytes.check(source.len())?;
        let normalized = NormalizedInput::new(source)?;
        let source = normalized.as_str();
        let context = LintContext::new(source).with_parse_limits(self.config.parse_limits);
        // Comments and markers come from the source itself, under the configured limits; a source
        // that does not load is an error here, as it is in `lint`
        let (scan, failure) = SourceScan::scan(
            source,
            context.source_context(),
            self.config.parse_limits,
            self.scan_needs(None),
        );
        if let Some(error) = failure {
            return Err(error.into());
        }
        let context = context.with_scan(scan);
        let directives = Directives::from_context(&context, &self.config, &self.registry);
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

        Ok(finish(diagnostics, directives))
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
    #[error(transparent)]
    ParseError(#[from] fast_yaml_core::ParseError),
    /// Source exceeds the configured input size limit.
    #[error(transparent)]
    InputTooLarge(#[from] InputTooLarge),
}

/// Applies inline directives and sorts.
fn finish(mut diagnostics: Vec<Diagnostic>, directives: Directives) -> Vec<Diagnostic> {
    directives.apply(&mut diagnostics);
    diagnostics.sort_by_key(|d| d.span.start);
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::test_support::config_with_rule;
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
                .build(),
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
    fn test_lint_honors_max_input_bytes() {
        let max = MaxInputBytes::new(5).unwrap();
        let linter = Linter::with_config(LintConfig::new().with_max_input_bytes(max));
        assert!(linter.lint("a: 1\n").is_ok());
        assert!(matches!(
            linter.lint("a: 1\n#"),
            Err(LintError::InputTooLarge(InputTooLarge { size: 6, .. }))
        ));
    }

    #[test]
    fn test_lint_and_lint_value_honor_max_scan_ahead() {
        let limits = ParseLimits {
            max_scan_ahead: fast_yaml_core::limits::MaxScanAhead::new(8).unwrap(),
            ..ParseLimits::default()
        };
        let linter = Linter::with_config(LintConfig::new().with_parse_limits(limits));
        let source = "[1, 2, 3, 4, 5, 6, 7, 8, 9]";
        let scan_ahead = |result: Result<_, LintError>| {
            matches!(
                result,
                Err(LintError::ParseError(
                    fast_yaml_core::ParseError::LimitExceeded {
                        kind: fast_yaml_core::LimitKind::ScanAhead(_),
                        ..
                    }
                ))
            )
        };
        assert!(scan_ahead(linter.lint(source)));
        assert!(scan_ahead(linter.lint_value(source, &Value::Null)));
        assert!(linter.lint_value("a: 1\n", &Value::Null).is_ok());
    }

    #[test]
    fn test_lint_value_honors_max_input_bytes() {
        let max = MaxInputBytes::new(8).unwrap();
        let linter = Linter::with_config(LintConfig::new().with_max_input_bytes(max));
        let value = Value::Null;
        assert!(linter.lint_value("a: 1\n", &value).is_ok());
        assert!(matches!(
            linter.lint_value("a: 1\nb: 2\n", &value),
            Err(LintError::InputTooLarge(_))
        ));
    }

    #[test]
    fn test_max_input_bytes_counts_bom() {
        let max = MaxInputBytes::new(6).unwrap();
        let linter = Linter::with_config(LintConfig::new().with_max_input_bytes(max));
        assert!(linter.lint("a: 1\n").is_ok());
        assert!(matches!(
            linter.lint("\u{feff}a: 1\n"),
            Err(LintError::InputTooLarge(InputTooLarge { size: 8, .. }))
        ));
    }

    #[test]
    fn test_max_input_bytes_defaults() {
        assert_eq!(LintConfig::new().max_input_bytes, MaxInputBytes::DEFAULT);
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
        assert_eq!(config.rules.indentation.options.indent_size().get(), 2);
        assert_eq!(
            config.rules.document_start.options.present,
            MarkerPresence::Allowed
        );
        assert!(config.rules.duplicate_key.enabled);
    }

    #[test]
    fn test_config_builder() {
        let config = LintConfig::new()
            .with_max_line_length(NonZeroUsize::new(120))
            .with_indent_size(indent(4));

        assert_eq!(config.rules.line_length.options.max, NonZeroUsize::new(120));
        assert_eq!(config.rules.indentation.options.indent_size().get(), 4);
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
        assert_eq!(linter.registry().rules().len(), 25);
    }

    #[test]
    fn test_linter_with_config() {
        let config = LintConfig::new().with_indent_size(indent(4));
        let linter = Linter::with_config(config);
        assert_eq!(
            linter
                .config()
                .rules
                .indentation
                .options
                .indent_size()
                .get(),
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

        assert_eq!(empty_diags.len(), 2, "{empty_diags:?}");
    }

    fn empty_value_lines(yaml: &str) -> Vec<usize> {
        Linter::with_all_rules()
            .lint(yaml)
            .unwrap()
            .iter()
            .filter(|d| d.code.as_str() == crate::DiagnosticCode::EMPTY_VALUES)
            .map(|d| d.span.start.line)
            .collect()
    }

    #[test]
    fn test_multidoc_empty_values_reported_once_each() {
        assert_eq!(empty_value_lines("a:\n---\nb:\n...\n---\nc: yes\n"), [1, 3]);
        assert_eq!(empty_value_lines("a:\n---\nb:\n---\nc:\n"), [1, 3, 5]);
    }

    #[test]
    fn test_many_documents_empty_values_are_linear() {
        let yaml = "---\na:\n".repeat(3000);
        let start = std::time::Instant::now();
        assert_eq!(empty_value_lines(&yaml).len(), 3000);
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn test_multidoc_truthy_positions_after_document_start() {
        let diags = Linter::with_all_rules()
            .lint("a: yes\n---\nb: no\n")
            .unwrap();
        let lines: Vec<usize> = diags
            .iter()
            .filter(|d| d.code.as_str() == crate::DiagnosticCode::TRUTHY)
            .map(|d| d.span.start.line)
            .collect();
        assert_eq!(lines, [1, 3]);
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
        let config = LintConfig::new().with_document_start(MarkerPresence::Required);
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
        let config = LintConfig::new().with_document_start(MarkerPresence::Required);
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
        let config = LintConfig::new().with_document_end(MarkerPresence::Required);
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
        let config = LintConfig::new().with_document_end(MarkerPresence::Required);
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

    fn assert_same_coordinates(plain: &[Diagnostic], bom: &[Diagnostic], normalized: &str) {
        assert_ne!(plain, []);
        assert_eq!(plain.len(), bom.len());
        for (p, b) in plain.iter().zip(bom) {
            assert_eq!(p.code, b.code);
            assert_eq!(p.span, b.span, "{}", p.code.as_str());
            assert_eq!(
                p.suggestions.iter().map(|s| s.span).collect::<Vec<_>>(),
                b.suggestions.iter().map(|s| s.span).collect::<Vec<_>>(),
                "{}",
                p.code.as_str()
            );
            assert!(
                normalized
                    .get(b.span.start.offset..b.span.end.offset)
                    .is_some()
            );
        }
    }

    #[test]
    fn test_lint_bom_input_uses_normalized_coordinates() {
        let linter = Linter::with_all_rules();
        let plain = linter.lint("# c\na: 1").unwrap();
        let bom = linter.lint("\u{FEFF}# c\na: 1").unwrap();
        assert_same_coordinates(&plain, &bom, "# c\na: 1");
    }

    #[test]
    fn test_lint_prefix_bom_lines_use_normalized_coordinates() {
        let linter = Linter::with_all_rules();
        let bom = '\u{FEFF}';
        for (plain_src, bom_src) in [
            (
                "a: 1\n...\nb: 2   \nc:  3\n".to_owned(),
                format!("a: 1\n...\n{bom}b: 2   \nc:  3\n"),
            ),
            (
                "a: 1\n...\n# c\n---\nb: 2   \n".to_owned(),
                format!("{bom}a: 1\n...\n{bom}# c\n{bom}---\nb: 2   \n"),
            ),
        ] {
            let plain = linter.lint(&plain_src).unwrap();
            let shifted = linter.lint(&bom_src).unwrap();
            assert_same_coordinates(&plain, &shifted, &plain_src);
        }
    }

    #[test]
    fn test_lint_rejects_non_printable_characters() {
        let linter = Linter::with_all_rules();
        for source in ["a: \u{FFFE}", "a: 1\u{7F}", "a: \"x\u{86}\""] {
            assert!(linter.lint(source).is_err(), "{source:?}");
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
    fn test_lint_value_bom_uses_normalized_coordinates() {
        let linter = Linter::with_all_rules();
        let src = "a: 1";
        let value = fast_yaml_core::Parser::parse_str(src).unwrap().unwrap();
        let plain = linter.lint_value(src, &value).unwrap();
        let bom = linter.lint_value("\u{FEFF}a: 1", &value).unwrap();
        assert_same_coordinates(&plain, &bom, src);
    }

    #[test]
    fn test_lint_bom_before_mapping_no_error() {
        let diagnostics = Linter::with_all_rules().lint("\u{FEFF}a: 1\n").unwrap();
        assert!(diagnostics.is_empty(), "unexpected: {diagnostics:?}");
    }

    #[test]
    fn lint_reports_merge_error_hidden_by_duplicate_key() {
        let err = Linter::with_all_rules()
            .lint("x: {<<: 1}\nx: 2\n")
            .unwrap_err();
        assert!(err.to_string().contains("line 1, column 5"), "{err}");
    }

    #[test]
    fn test_comment_only_source_lints_without_documents() {
        let diagnostics = Linter::with_all_rules().lint("# only a comment\n").unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn test_lint_scans_comments_in_the_loader_pass() {
        let diagnostics = Linter::with_all_rules()
            .lint("a: 1\n---\nb: 2 #x\n")
            .unwrap();
        let lines: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code.as_str() == "comments")
            .map(|d| d.span.start.line)
            .collect();
        assert_eq!(lines, [3, 3]);
    }

    #[test]
    fn test_lint_value_applies_directives_under_the_configured_limits() {
        use fast_yaml_core::limits::{MaxDepth, ParseLimits};
        use std::fmt::Write as _;

        let mut source = String::from("# fy: disable-file\n");
        for depth in 0..300 {
            writeln!(source, "{}k:", " ".repeat(depth)).unwrap();
        }
        writeln!(source, "{}v: 1 ", " ".repeat(300)).unwrap();
        let value = Value::Null;

        let limits = |depth| ParseLimits {
            max_depth: MaxDepth::new(depth).unwrap(),
            ..ParseLimits::default()
        };
        let raised = Linter::with_config(LintConfig::new().with_parse_limits(limits(512)));
        assert_eq!(raised.lint_value(&source, &value).unwrap(), []);

        let lowered = Linter::with_config(LintConfig::new().with_parse_limits(limits(8)));
        assert!(matches!(
            lowered.lint_value(&source, &value),
            Err(LintError::ParseError(_))
        ));
    }
}
