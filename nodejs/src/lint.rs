//! NAPI-RS bindings for fast-yaml-linter.
//!
//! Exposes the YAML linter API to Node.js with comprehensive diagnostics,
//! rich error reporting, and configurable linting rules.

use fast_yaml_linter::{
    ContextLine as RustContextLine, Diagnostic as RustDiagnostic,
    DiagnosticContext as RustDiagnosticContext, LintConfig as RustLintConfig, Linter as RustLinter,
    Location as RustLocation, Severity as RustSeverity, Span as RustSpan,
    Suggestion as RustSuggestion,
};
use napi_derive::napi;
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
            line: loc.line as u32,
            column: loc.column as u32,
            offset: loc.offset as u32,
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
            line_number: line.line_number as u32,
            content: line.content,
            highlights: line
                .highlights
                .into_iter()
                .map(|(start, end)| vec![start as u32, end as u32])
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
#[napi(object)]
#[derive(Debug, Clone, Default)]
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
}

fn to_rust_lint_config(config: LintConfig) -> RustLintConfig {
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
    if let Some(rules) = config.disabled_rules {
        rust.disabled_rules = HashSet::from_iter(rules);
    }
    rust
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
            Some(cfg) => RustLinter::with_config(to_rust_lint_config(cfg)),
            None => RustLinter::new(),
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
pub fn lint(source: String, config: Option<LintConfig>) -> napi::Result<Vec<Diagnostic>> {
    let linter = match config {
        Some(cfg) => RustLinter::with_all_rules_and_config(to_rust_lint_config(cfg)),
        None => RustLinter::with_all_rules(),
    };
    linter
        .lint(&source)
        .map(convert_diagnostics)
        .map_err(|e| napi::Error::from_reason(format!("Linting failed: {e}")))
}
