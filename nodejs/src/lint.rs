//! NAPI-RS bindings for fast-yaml-linter.
//!
//! Exposes the YAML linter API to Node.js with comprehensive diagnostics,
//! rich error reporting, and configurable linting rules.

use fast_yaml_linter::{
    ContextLine as RustContextLine, Diagnostic as RustDiagnostic,
    DiagnosticContext as RustDiagnosticContext, LintConfig as RustLintConfig, Linter as RustLinter,
    Location as RustLocation, Severity as RustSeverity, Span as RustSpan,
    Suggestion as RustSuggestion, config::RuleConfig as RustRuleConfig,
};
use napi_derive::napi;
use serde_json::Value as JsonValue;
use std::collections::HashSet;

/// Diagnostic severity levels.
#[napi(string_enum)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Critical error that prevents YAML parsing or violates spec.
    Error,
    /// Potential issue that should be addressed.
    Warning,
    /// Informational message about style or best practices.
    Info,
    /// Suggestion for improvement.
    Hint,
}

impl From<RustSeverity> for Severity {
    fn from(s: RustSeverity) -> Self {
        match s {
            RustSeverity::Error => Self::Error,
            RustSeverity::Warning => Self::Warning,
            RustSeverity::Info => Self::Info,
            RustSeverity::Hint => Self::Hint,
        }
    }
}

/// A position in the source file.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct Location {
    /// Line number (1-indexed).
    pub line: u32,
    /// Column number (1-indexed).
    pub column: u32,
    /// Byte offset from start of file (0-indexed).
    pub offset: u32,
}

impl From<RustLocation> for Location {
    fn from(loc: RustLocation) -> Self {
        Self {
            line: u32::try_from(loc.line).unwrap_or(u32::MAX),
            column: u32::try_from(loc.column).unwrap_or(u32::MAX),
            offset: u32::try_from(loc.offset).unwrap_or(u32::MAX),
        }
    }
}

/// A span of text in the source file.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct Span {
    /// Start position (inclusive).
    pub start: Location,
    /// End position (exclusive).
    pub end: Location,
}

impl From<RustSpan> for Span {
    fn from(span: RustSpan) -> Self {
        Self {
            start: span.start.into(),
            end: span.end.into(),
        }
    }
}

/// A single line of source context.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ContextLine {
    /// Line number (1-indexed).
    pub line_number: u32,
    /// Source text content.
    pub content: String,
    /// Highlight ranges as [[start, end], ...] (column positions).
    pub highlights: Vec<Vec<u32>>,
}

impl From<RustContextLine> for ContextLine {
    fn from(line: RustContextLine) -> Self {
        Self {
            line_number: u32::try_from(line.line_number).unwrap_or(u32::MAX),
            content: line.content,
            highlights: line
                .highlights
                .into_iter()
                .map(|(start, end)| {
                    vec![
                        u32::try_from(start).unwrap_or(u32::MAX),
                        u32::try_from(end).unwrap_or(u32::MAX),
                    ]
                })
                .collect(),
        }
    }
}

/// Source code context for diagnostics.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DiagnosticContext {
    /// Source lines surrounding the diagnostic.
    pub lines: Vec<ContextLine>,
}

impl From<RustDiagnosticContext> for DiagnosticContext {
    fn from(ctx: RustDiagnosticContext) -> Self {
        Self {
            lines: ctx.lines.into_iter().map(Into::into).collect(),
        }
    }
}

/// A suggested fix for a diagnostic.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// Description of the fix.
    pub message: String,
    /// Span to replace.
    pub span: Span,
    /// Replacement text (None = deletion).
    pub replacement: Option<String>,
}

impl From<RustSuggestion> for Suggestion {
    fn from(s: RustSuggestion) -> Self {
        Self {
            message: s.message,
            span: s.span.into(),
            replacement: s.replacement,
        }
    }
}

/// A diagnostic message with location and context.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Diagnostic code (e.g., "duplicate-key").
    pub code: String,
    /// Severity level.
    pub severity: Severity,
    /// Primary error message.
    pub message: String,
    /// Location span where the error occurred.
    pub span: Span,
    /// Additional context for display.
    pub context: Option<DiagnosticContext>,
    /// Suggested fixes.
    pub suggestions: Vec<Suggestion>,
}

impl From<RustDiagnostic> for Diagnostic {
    fn from(d: RustDiagnostic) -> Self {
        Self {
            code: d.code.as_str().to_string(),
            severity: d.severity.into(),
            message: d.message,
            span: d.span.into(),
            context: d.context.map(Into::into),
            suggestions: d.suggestions.into_iter().map(Into::into).collect(),
        }
    }
}

/// Parses a severity string into a `RustSeverity`.
///
/// # Errors
///
/// Returns an error if the string is not a valid severity value.
fn parse_severity_str(s: &str) -> napi::Result<RustSeverity> {
    match s.to_lowercase().as_str() {
        "error" => Ok(RustSeverity::Error),
        "warning" => Ok(RustSeverity::Warning),
        "info" => Ok(RustSeverity::Info),
        "hint" => Ok(RustSeverity::Hint),
        _ => Err(napi::Error::from_reason(format!(
            "Invalid severity '{s}', expected one of: error, warning, info, hint"
        ))),
    }
}

/// Parses a JSON value (string shorthand or object) into a `RustRuleConfig`.
///
/// Accepts either a string like `"error"` or an object `{ severity?, enabled? }`.
///
/// # Errors
///
/// Returns an error if the value has an invalid type or invalid severity string.
fn parse_rule_config_json(value: &JsonValue) -> napi::Result<RustRuleConfig> {
    match value {
        JsonValue::String(s) => {
            let severity = parse_severity_str(s)?;
            Ok(RustRuleConfig::new().with_severity(severity))
        }
        JsonValue::Object(obj) => {
            let mut rc = RustRuleConfig::new();

            // Parse optional `enabled` field
            if obj.get("enabled").and_then(JsonValue::as_bool) == Some(false) {
                rc = RustRuleConfig::disabled();
            }

            // Parse optional `severity` field (applied after enabled to preserve it)
            if let Some(sev_str) = obj.get("severity").and_then(JsonValue::as_str) {
                rc = rc.with_severity(parse_severity_str(sev_str)?);
            }

            Ok(rc)
        }
        JsonValue::Null => Ok(RustRuleConfig::new()),
        _ => Err(napi::Error::from_reason(
            "Rule config value must be a string (severity shorthand) or an object { severity?, enabled? }",
        )),
    }
}

/// Configuration for the linter.
///
/// All fields are optional; defaults are applied during conversion.
#[napi(object)]
#[derive(Default)]
pub struct LintConfig {
    /// Maximum line length (None = unlimited).
    pub max_line_length: Option<u32>,
    /// Expected indentation size in spaces.
    pub indent_size: Option<u32>,
    /// Require document start marker (---).
    pub require_document_start: Option<bool>,
    /// Require document end marker (...).
    pub require_document_end: Option<bool>,
    /// Allow duplicate keys (non-compliant).
    pub allow_duplicate_keys: Option<bool>,
    /// Disabled rule codes.
    pub disabled_rules: Option<Vec<String>>,
    /// Per-rule configuration overrides.
    ///
    /// Each key is a rule code; the value is either a severity string shorthand
    /// (`"error"` | `"warning"` | `"info"` | `"hint"`) or an object with optional
    /// `severity` and `enabled` fields.
    ///
    /// Unknown rule codes are silently accepted (they have no effect at lint time).
    /// `disabled_rules` takes precedence over a `rules` entry with `enabled: false`.
    ///
    /// Note: `options` field of `RuleConfig` is intentionally not exposed here (no
    /// current rule uses custom options; deferred to a future release).
    /// Note: pass as a JS object; values may be a severity string shorthand or
    /// `{ severity?, enabled? }` object. Internally deserialized via `serde_json`.
    pub rules: Option<JsonValue>,
}

fn to_rust_lint_config(config: LintConfig) -> napi::Result<RustLintConfig> {
    let mut rust = RustLintConfig::default();
    if let Some(max) = config.max_line_length {
        rust.max_line_length = Some(max as usize);
    }
    if let Some(indent) = config.indent_size {
        rust.indent_size = indent as usize;
    }
    if let Some(v) = config.require_document_start {
        rust.require_document_start = v;
    }
    if let Some(v) = config.require_document_end {
        rust.require_document_end = v;
    }
    if let Some(v) = config.allow_duplicate_keys {
        rust.allow_duplicate_keys = v;
    }
    if let Some(disabled) = config.disabled_rules {
        rust.disabled_rules = HashSet::from_iter(disabled);
    }
    if let Some(JsonValue::Object(rules_map)) = config.rules {
        for (code, value) in rules_map {
            let rc = parse_rule_config_json(&value)?;
            rust = rust.with_rule_config(code, rc);
        }
    }
    Ok(rust)
}

fn convert_diagnostics(diagnostics: Vec<RustDiagnostic>) -> Vec<Diagnostic> {
    diagnostics.into_iter().map(Into::into).collect()
}

/// YAML linter with configurable rules.
///
/// # Example
///
/// ```javascript
/// const { Linter } = require('@fast-yaml/core');
/// const linter = Linter.withAllRules();
/// const diagnostics = linter.lint('name: value\nname: duplicate');
/// ```
#[napi]
pub struct Linter {
    inner: RustLinter,
}

#[napi]
impl Linter {
    /// Creates a new linter with optional configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    #[napi(constructor)]
    pub fn new(config: Option<LintConfig>) -> napi::Result<Self> {
        let inner = match config {
            Some(cfg) => RustLinter::with_config(to_rust_lint_config(cfg)?),
            None => RustLinter::with_config(RustLintConfig::default()),
        };
        Ok(Self { inner })
    }

    /// Creates a linter with all default rules enabled.
    #[napi(factory)]
    pub fn with_all_rules() -> Self {
        Self {
            inner: RustLinter::with_all_rules(),
        }
    }

    /// Lints YAML source code and returns diagnostics.
    ///
    /// # Errors
    ///
    /// Returns an error if the YAML cannot be parsed.
    #[napi]
    #[allow(clippy::needless_pass_by_value)]
    pub fn lint(&self, source: String) -> napi::Result<Vec<Diagnostic>> {
        self.inner
            .lint(&source)
            .map(convert_diagnostics)
            .map_err(|e| napi::Error::from_reason(format!("Linting failed: {e}")))
    }
}

/// Lint YAML source with optional configuration.
///
/// Convenience function equivalent to `Linter.withAllRules().lint(source)`.
///
/// # Errors
///
/// Returns an error if the YAML cannot be parsed.
///
/// # Example
///
/// ```javascript
/// const { lint } = require('@fast-yaml/core');
/// const diagnostics = lint('key: value\nkey: duplicate');
/// ```
#[napi]
#[allow(clippy::needless_pass_by_value)]
pub fn lint(source: String, config: Option<LintConfig>) -> napi::Result<Vec<Diagnostic>> {
    let linter = match config {
        Some(cfg) => RustLinter::with_all_rules_and_config(to_rust_lint_config(cfg)?),
        None => RustLinter::with_all_rules(),
    };
    linter
        .lint(&source)
        .map(convert_diagnostics)
        .map_err(|e| napi::Error::from_reason(format!("Linting failed: {e}")))
}
