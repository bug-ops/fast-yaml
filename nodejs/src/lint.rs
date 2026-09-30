//! NAPI-RS bindings for fast-yaml-linter.
//!
//! Exposes the YAML linter API to Node.js with comprehensive diagnostics,
//! rich error reporting, and configurable linting rules.

use crate::limits::{max_input_bytes, parse_limits};
use fast_yaml_linter::{
    ContextLine as RustContextLine, Diagnostic as RustDiagnostic,
    DiagnosticContext as RustDiagnosticContext, LintConfig as RustLintConfig, Linter as RustLinter,
    Location as RustLocation, Severity as RustSeverity, Span as RustSpan,
    Suggestion as RustSuggestion,
    config::{IndentSize, RuleConfigError, RuleName},
    rules::MarkerPresence,
};
use napi_derive::napi;
use std::{num::NonZeroUsize, str::FromStr};

use crate::rule_input::RuleInput;

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
            RustSeverity::Warning => Self::Warning,
            RustSeverity::Info => Self::Info,
            RustSeverity::Hint => Self::Hint,
            // Severity is non_exhaustive; treat future levels as the most severe
            RustSeverity::Error | _ => Self::Error,
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
    /// Number of chars of the line dropped before `content`.
    pub column_offset: u32,
    /// Whether chars of the line were dropped after `content`.
    pub truncated_end: bool,
    /// Highlight ranges as [[start, end], ...] (absolute column positions).
    pub highlights: Vec<Vec<u32>>,
}

impl From<RustContextLine> for ContextLine {
    fn from(line: RustContextLine) -> Self {
        Self {
            line_number: u32::try_from(line.line_number).unwrap_or(u32::MAX),
            content: line.content,
            column_offset: u32::try_from(line.column_offset).unwrap_or(u32::MAX),
            truncated_end: line.truncated_end,
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

/// Configuration for the linter.
///
/// All fields are optional; defaults are applied during conversion.
#[napi(object, object_to_js = false)]
#[derive(Default)]
pub struct LintConfig {
    /// Maximum line length; unset keeps the rule default, `0` is an error.
    pub max_line_length: Option<f64>,
    /// Expected indentation size in spaces.
    pub indent_size: Option<f64>,
    /// Require document start marker (---).
    pub require_document_start: Option<bool>,
    /// Require document end marker (...).
    pub require_document_end: Option<bool>,
    /// Allow duplicate keys (non-compliant).
    pub allow_duplicate_keys: Option<bool>,
    /// Disabled rule codes.
    pub disabled_rules: Option<Vec<String>>,
    /// Per-rule configuration patch, applied after the fields above.
    ///
    /// Each key is a rule code; the value is a severity string (case-insensitive),
    /// or an object with `enabled`, `severity` and the rule's own options
    /// (kebab-case keys, as in the `fy lint --config` file). Unknown rules, unknown
    /// options, wrong types and `null` (except `line-length.max`) are errors.
    /// `disabledRules` is applied last and wins over `enabled: true`. The value is read
    /// under depth and size limits, so deeply nested or cyclic input is an `InvalidArg` error.
    #[napi(ts_type = "LintRulesConfig")]
    pub rules: Option<RuleInput>,
    /// Maximum collection nesting depth (integer, 1..=512, default: 256).
    /// Stack note: the calling thread needs about 1 MiB of stack at depth 512 (roughly 980 KiB measured in release); on stacks of 512 KiB or less (e.g. a worker with stackSizeMb 0.5) the process can abort and the overflow cannot be caught, while the default 256 is safe. The emitter keeps its own fixed depth of 256, so data parsed deeper may fail to dump.
    pub max_depth: Option<f64>,
    /// Maximum estimated alias-expansion bytes (integer, 1..=1073741824, default: 67108864).
    pub max_alias_bytes: Option<f64>,
    /// Largest source accepted for linting, in bytes (integer, 1..=1073741824, default: 104857600).
    /// Bounds linting work on oversized input; the source is already in memory when checked, so this is not a memory bound.
    pub max_input_bytes: Option<f64>,
}

fn config_error(error: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(error.to_string())
}

/// Validates that a JS number is a finite integer within `min..=max`; `expected` words the error.
fn checked_uint(field: &str, expected: &str, value: f64, min: u64, max: u64) -> napi::Result<u64> {
    #[allow(clippy::cast_precision_loss)]
    let in_range =
        value.is_finite() && value.fract() == 0.0 && value >= min as f64 && value <= max as f64;
    if !in_range {
        return Err(config_error(format!(
            "{field} must be {expected}, got {value}"
        )));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value as u64)
}

fn to_rust_lint_config(config: &LintConfig) -> napi::Result<RustLintConfig> {
    let mut rust = RustLintConfig::new()
        .with_parse_limits(parse_limits(config.max_depth, config.max_alias_bytes)?)
        .with_max_input_bytes(max_input_bytes(config.max_input_bytes)?);
    if let Some(max) = config.max_line_length {
        let max = checked_uint(
            "maxLineLength",
            "a positive integer no greater than 4294967295",
            max,
            1,
            u64::from(u32::MAX),
        )?;
        rust = rust.with_max_line_length(NonZeroUsize::new(
            usize::try_from(max).map_err(config_error)?,
        ));
    }
    if let Some(indent) = config.indent_size {
        let indent = checked_uint("indentSize", "an integer between 1 and 16", indent, 1, 16)?;
        rust = rust.with_indent_size(IndentSize::try_from(indent).map_err(config_error)?);
    }
    if config.require_document_start == Some(true) {
        rust = rust.with_document_start(MarkerPresence::Required);
    }
    if config.require_document_end == Some(true) {
        rust = rust.with_document_end(MarkerPresence::Required);
    }
    if config.allow_duplicate_keys == Some(true) {
        rust = rust.with_disabled_rule(RuleName::DuplicateKey);
    }
    if let Some(rules) = &config.rules {
        rust.rules.apply(&rules.0).map_err(|e| match e {
            RuleConfigError::Malformed { message } => {
                config_error(format!("rules must be an object: {message}"))
            }
            other => config_error(other),
        })?;
    }
    for code in config.disabled_rules.iter().flatten() {
        rust = rust.with_disabled_rule(RuleName::from_str(code).map_err(config_error)?);
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
    #[napi(constructor, catch_unwind)]
    pub fn new(config: Option<LintConfig>) -> napi::Result<Self> {
        let inner = match config {
            Some(cfg) => RustLinter::with_config(to_rust_lint_config(&cfg)?),
            None => RustLinter::with_config(RustLintConfig::default()),
        };
        Ok(Self { inner })
    }

    /// Creates a linter with all default rules enabled.
    #[napi(factory, catch_unwind)]
    pub fn with_all_rules() -> Self {
        Self {
            inner: RustLinter::with_all_rules(),
        }
    }

    /// Lints YAML source code and returns diagnostics.
    ///
    /// # Errors
    ///
    /// Returns an error if the YAML cannot be parsed or the source exceeds `maxInputBytes`
    /// (default 100 MiB).
    #[napi(catch_unwind)]
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
/// Returns an error if the YAML cannot be parsed or the source exceeds `maxInputBytes`
/// (default 100 MiB).
///
/// # Example
///
/// ```javascript
/// const { lint } = require('@fast-yaml/core');
/// const diagnostics = lint('key: value\nkey: duplicate');
/// ```
#[napi(catch_unwind)]
#[allow(clippy::needless_pass_by_value)]
pub fn lint(source: String, config: Option<LintConfig>) -> napi::Result<Vec<Diagnostic>> {
    let linter = match config {
        Some(cfg) => RustLinter::with_all_rules_and_config(to_rust_lint_config(&cfg)?),
        None => RustLinter::with_all_rules(),
    };
    linter
        .lint(&source)
        .map(convert_diagnostics)
        .map_err(|e| napi::Error::from_reason(format!("Linting failed: {e}")))
}
