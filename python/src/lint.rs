//! `PyO3` bindings for fast-yaml-linter.
//!
//! Exposes the YAML linter API to Python with comprehensive diagnostics,
//! rich error reporting, and configurable linting rules.

use crate::limits;
use crate::rule_input::ValueConverter;
use fast_yaml_core::ParseLimits;
use fast_yaml_core::limits::{AliasBytes, Depth, Documents, InputBytes, ScanAhead};
use fast_yaml_linter::config::{IndentSize, RuleName};
use fast_yaml_linter::formatter::Findings;
use fast_yaml_linter::rules::MarkerPresence;
use fast_yaml_linter::{
    ContextLine as RustContextLine, Diagnostic as RustDiagnostic,
    DiagnosticCode as RustDiagnosticCode, DiagnosticContext as RustDiagnosticContext,
    Excerpt as RustExcerpt, Formatter as RustFormatter, LintConfig as RustLintConfig,
    LintError as RustLintError, Linter as RustLinter, Location as RustLocation,
    Severity as RustSeverity, Span as RustSpan, Suggestion as RustSuggestion,
    TextFormatter as RustTextFormatter,
};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};
use serde_norway::{Mapping, Value};
use std::borrow::Cow;
use std::num::NonZeroUsize;
use std::str::FromStr;

#[cfg(feature = "json-output")]
use fast_yaml_linter::JsonFormatter as RustJsonFormatter;

/// Diagnostic severity levels.
///
/// Categorizes diagnostics by importance, from critical errors to informational hints.
///
/// Examples:
///     >>> from `fast_yaml`._core.lint import Severity
///     >>> error = Severity.ERROR
///     >>> warning = Severity.WARNING
#[pyclass(
    module = "fast_yaml._core.lint",
    name = "Severity",
    skip_from_py_object
)]
#[derive(Clone, Copy)]
pub struct PySeverity {
    inner: RustSeverity,
}

#[pymethods]
impl PySeverity {
    /// Critical error that prevents YAML parsing or violates spec.
    #[classattr]
    #[allow(non_snake_case)] // Python convention for constants
    const fn ERROR() -> Self {
        Self {
            inner: RustSeverity::Error,
        }
    }

    /// Potential issue that should be addressed.
    #[classattr]
    #[allow(non_snake_case)] // Python convention for constants
    const fn WARNING() -> Self {
        Self {
            inner: RustSeverity::Warning,
        }
    }

    /// Informational message about style or best practices.
    #[classattr]
    #[allow(non_snake_case)] // Python convention for constants
    const fn INFO() -> Self {
        Self {
            inner: RustSeverity::Info,
        }
    }

    /// Suggestion for improvement.
    #[classattr]
    #[allow(non_snake_case)] // Python convention for constants
    const fn HINT() -> Self {
        Self {
            inner: RustSeverity::Hint,
        }
    }

    /// Get the string representation of the severity.
    const fn as_str(&self) -> &str {
        self.inner.as_str()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)] // Required by PyO3
    fn __str__(&self) -> String {
        self.inner.as_str().to_string()
    }

    #[allow(clippy::trivially_copy_pass_by_ref)] // Required by PyO3
    fn __repr__(&self) -> String {
        format!("Severity.{}", self.inner.as_str().to_uppercase())
    }

    #[allow(clippy::trivially_copy_pass_by_ref)] // Required by PyO3
    fn __eq__(&self, other: &Self) -> bool {
        self.inner == other.inner
    }

    #[allow(clippy::trivially_copy_pass_by_ref)] // Required by PyO3
    fn __hash__(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.inner.hash(&mut hasher);
        hasher.finish()
    }
}

impl From<RustSeverity> for PySeverity {
    fn from(severity: RustSeverity) -> Self {
        Self { inner: severity }
    }
}

/// A position in the source file.
///
/// Represents a single point in the YAML source with line, column,
/// and byte offset information for precise error reporting.
///
/// Examples:
///     >>> from `fast_yaml`._core.lint import Location
///     >>> loc = Location(line=10, column=5, offset=145)
///     >>> print(f"Line {loc.line}, Column {loc.column}")
///     Line 10, Column 5
#[pyclass(module = "fast_yaml._core.lint", name = "Location", from_py_object)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PyLocation {
    /// Line number (1-indexed, human-readable).
    #[pyo3(get)]
    pub line: usize,

    /// Column number (1-indexed, human-readable).
    #[pyo3(get)]
    pub column: usize,

    /// Byte offset in the text with document-prefix BOMs removed (0-indexed), also for suggestion spans.
    #[pyo3(get)]
    pub offset: usize,
}

#[pymethods]
impl PyLocation {
    #[new]
    #[pyo3(signature = (line, column, offset))]
    const fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Location(line={}, column={}, offset={})",
            self.line, self.column, self.offset
        )
    }

    const fn __eq__(&self, other: &Self) -> bool {
        self.line == other.line && self.column == other.column && self.offset == other.offset
    }
}

impl From<RustLocation> for PyLocation {
    fn from(loc: RustLocation) -> Self {
        Self {
            line: loc.line,
            column: loc.column,
            offset: loc.offset,
        }
    }
}

/// A span of text in the source file.
///
/// Represents a range from a start location to an end location.
///
/// Examples:
///     >>> from `fast_yaml`._core.lint import Location, Span
///     >>> start = Location(10, 5, 145)
///     >>> end = Location(10, 9, 149)
///     >>> span = Span(start, end)
#[pyclass(module = "fast_yaml._core.lint", name = "Span", from_py_object)]
#[derive(Clone)]
pub struct PySpan {
    /// Start position (inclusive).
    #[pyo3(get)]
    pub start: PyLocation,

    /// End position (exclusive).
    #[pyo3(get)]
    pub end: PyLocation,
}

#[pymethods]
impl PySpan {
    #[new]
    #[pyo3(signature = (start, end))]
    const fn new(start: PyLocation, end: PyLocation) -> Self {
        Self { start, end }
    }

    fn __repr__(&self) -> String {
        format!("Span(start={:?}, end={:?})", self.start, self.end)
    }

    fn __eq__(&self, other: &Self) -> bool {
        self.start == other.start && self.end == other.end
    }
}

impl From<RustSpan> for PySpan {
    fn from(span: RustSpan) -> Self {
        Self {
            start: span.start.into(),
            end: span.end.into(),
        }
    }
}

/// A single line of source context.
#[pyclass(module = "fast_yaml._core.lint", name = "ContextLine", from_py_object)]
#[derive(Clone)]
pub struct PyContextLine {
    /// Line number (1-indexed).
    #[pyo3(get)]
    pub line_number: usize,

    /// Source text content.
    #[pyo3(get)]
    pub content: String,

    /// Number of chars of the line dropped before `content`.
    #[pyo3(get)]
    pub column_offset: usize,

    /// Whether chars of the line were dropped after `content`.
    #[pyo3(get)]
    pub truncated_end: bool,

    /// Highlight ranges (column start, column end) in absolute line columns.
    #[pyo3(get)]
    pub highlights: Vec<(usize, usize)>,
}

#[pymethods]
impl PyContextLine {
    #[new]
    #[pyo3(signature = (line_number, content, highlights, column_offset=0, truncated_end=false))]
    const fn new(
        line_number: usize,
        content: String,
        highlights: Vec<(usize, usize)>,
        column_offset: usize,
        truncated_end: bool,
    ) -> Self {
        Self {
            line_number,
            content,
            column_offset,
            truncated_end,
            highlights,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "ContextLine(line_number={}, content={:?}, highlights={:?}, column_offset={}, truncated_end={})",
            self.line_number, self.content, self.highlights, self.column_offset, self.truncated_end
        )
    }
}

// Note: RustContextLine is not publicly exported, so we can't directly convert
// We'll create PyContextLine from the DiagnosticContext fields instead

/// Source code context for diagnostics.
#[pyclass(
    module = "fast_yaml._core.lint",
    name = "DiagnosticContext",
    from_py_object
)]
#[derive(Clone)]
pub struct PyDiagnosticContext {
    /// Source lines to display.
    #[pyo3(get)]
    pub lines: Vec<PyContextLine>,
}

#[pymethods]
impl PyDiagnosticContext {
    #[new]
    #[pyo3(signature = (lines))]
    const fn new(lines: Vec<PyContextLine>) -> Self {
        Self { lines }
    }

    fn __repr__(&self) -> String {
        format!("DiagnosticContext(lines={} lines)", self.lines.len())
    }
}

impl From<RustDiagnosticContext> for PyDiagnosticContext {
    fn from(context: RustDiagnosticContext) -> Self {
        // Extract lines directly since ContextLine is not publicly exported
        let lines = context
            .lines
            .into_iter()
            .map(|line| PyContextLine {
                line_number: line.line_number,
                content: line.content,
                column_offset: line.column_offset,
                truncated_end: line.truncated_end,
                highlights: line.highlights,
            })
            .collect();

        Self { lines }
    }
}

/// A suggested fix for a diagnostic.
#[pyclass(module = "fast_yaml._core.lint", name = "Suggestion", from_py_object)]
#[derive(Clone)]
pub struct PySuggestion {
    /// Description of the fix.
    #[pyo3(get)]
    pub message: String,

    /// Span to replace.
    #[pyo3(get)]
    pub span: PySpan,

    /// Replacement text (None = deletion).
    #[pyo3(get)]
    pub replacement: Option<String>,
}

#[pymethods]
impl PySuggestion {
    #[new]
    #[pyo3(signature = (message, span, replacement=None))]
    const fn new(message: String, span: PySpan, replacement: Option<String>) -> Self {
        Self {
            message,
            span,
            replacement,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Suggestion(message={:?}, replacement={:?})",
            self.message, self.replacement
        )
    }
}

// Note: RustSuggestion is not publicly exported
// We'll extract suggestions from diagnostics directly during conversion

/// A diagnostic message with location and context.
///
/// Represents a single linting issue with severity, location,
/// message, source context, and optional suggestions for fixes.
///
/// Examples:
///     >>> from `fast_yaml`._core.lint import lint
///     >>> diagnostics = lint("key: value\\nkey: duplicate")
///     >>> for diag in diagnostics:
///     ...     `print(f"{diag.severity.as_str()}`: {diag.message}")
///     error: duplicate key 'key' found
#[pyclass(module = "fast_yaml._core.lint", name = "Diagnostic", from_py_object)]
#[derive(Clone)]
pub struct PyDiagnostic {
    /// Diagnostic code (e.g., "duplicate-key").
    #[pyo3(get)]
    pub code: String,

    /// Severity level.
    #[pyo3(get)]
    pub severity: PySeverity,

    /// Primary error message.
    #[pyo3(get)]
    pub message: String,

    /// Location span where the error occurred.
    #[pyo3(get)]
    pub span: PySpan,

    /// Additional context for display.
    #[pyo3(get)]
    pub context: Option<PyDiagnosticContext>,

    /// Suggested fixes.
    #[pyo3(get)]
    pub suggestions: Vec<PySuggestion>,
}

#[pymethods]
impl PyDiagnostic {
    fn __repr__(&self) -> String {
        format!(
            "Diagnostic(code={:?}, severity={}, message={:?})",
            self.code,
            self.severity.as_str(),
            self.message
        )
    }
}

impl From<(RustDiagnostic, Option<RustDiagnosticContext>)> for PyDiagnostic {
    fn from((diagnostic, context): (RustDiagnostic, Option<RustDiagnosticContext>)) -> Self {
        // Convert suggestions manually since Suggestion is not publicly exported
        let suggestions = diagnostic
            .suggestions
            .into_iter()
            .map(|suggestion| PySuggestion {
                message: suggestion.message,
                span: suggestion.span.into(),
                replacement: suggestion.replacement,
            })
            .collect();

        Self {
            code: diagnostic.code.as_str().to_string(),
            severity: diagnostic.severity.into(),
            message: diagnostic.message.into_owned(),
            span: diagnostic.span.into(),
            context: context.map(Into::into),
            suggestions,
        }
    }
}

/// Buffers a Python object into an intermediate YAML value with exact types and bounded size.
fn to_value(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    ValueConverter::default().convert(obj, 0)
}

fn parse_max_line_length(max: Option<i128>) -> PyResult<Option<NonZeroUsize>> {
    max.map(|value| {
        usize::try_from(value)
            .ok()
            .and_then(NonZeroUsize::new)
            .ok_or_else(|| PyValueError::new_err("max_line_length must be a positive integer"))
    })
    .transpose()
}

fn parse_indent_size(size: i128) -> PyResult<IndentSize> {
    size.to_string()
        .parse()
        .map_err(|e: <IndentSize as FromStr>::Err| PyValueError::new_err(e.to_string()))
}

fn parse_rule_name(code: &str) -> PyResult<RuleName> {
    RuleName::from_str(code).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Configuration for the linter.
///
/// Controls linting behavior including rule enablement,
/// formatting preferences, and validation strictness.
///
/// Linting a source larger than `max_input_bytes` (default 100 MiB) raises `ValueError`.
///
/// Examples:
///     >>> from `fast_yaml`._core.lint import `LintConfig`
///     >>> config = `LintConfig(max_line_length=120`, `indent_size=4`)
///     >>> config = config.with_disabled_rule("line-length")
#[pyclass(module = "fast_yaml._core.lint", name = "LintConfig", from_py_object)]
#[derive(Clone)]
pub struct PyLintConfig {
    inner: RustLintConfig,
}

#[pymethods]
impl PyLintConfig {
    #[new]
    #[pyo3(signature = (
        max_line_length=Some(80),
        indent_size=2,
        require_document_start=false,
        require_document_end=false,
        allow_duplicate_keys=false,
        disabled_rules=None,
        rules=None,
        max_depth=None,
        max_alias_bytes=None,
        max_input_bytes=None,
        max_scan_ahead=None,
        max_documents=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        max_line_length: Option<i128>,
        indent_size: i128,
        require_document_start: bool,
        require_document_end: bool,
        allow_duplicate_keys: bool,
        disabled_rules: Option<Bound<'_, PyAny>>,
        rules: Option<Bound<'_, PyAny>>,
        max_depth: Option<&Bound<'_, PyAny>>,
        max_alias_bytes: Option<&Bound<'_, PyAny>>,
        max_input_bytes: Option<&Bound<'_, PyAny>>,
        max_scan_ahead: Option<&Bound<'_, PyAny>>,
        max_documents: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let parse_limits =
            limits::parse_limits(max_depth, max_alias_bytes, max_scan_ahead, max_documents)?;
        let mut inner = RustLintConfig::new()
            .with_max_line_length(parse_max_line_length(max_line_length)?)
            .with_indent_size(parse_indent_size(indent_size)?)
            .with_parse_limits(parse_limits)
            .with_max_input_bytes(limits::bounded::<InputBytes>(
                "max_input_bytes",
                max_input_bytes,
            )?);

        if require_document_start {
            inner = inner.with_document_start(MarkerPresence::Required);
        }
        if require_document_end {
            inner = inner.with_document_end(MarkerPresence::Required);
        }
        if allow_duplicate_keys {
            inner = inner.with_disabled_rule(RuleName::DuplicateKey);
        }

        if let Some(rules_obj) = rules {
            let value = ValueConverter::default()
                .convert_rules(&rules_obj, |name| parse_rule_name(name).map(drop))?;
            inner
                .rules
                .apply(value)
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
        }

        if let Some(disabled) = disabled_rules {
            if disabled.is_instance_of::<PyString>() {
                return Err(PyTypeError::new_err(
                    "disabled_rules must be a sequence of rule codes, not a single string",
                ));
            }
            for rule in disabled.try_iter()? {
                let code: String = rule?.extract()?;
                inner = inner.with_disabled_rule(parse_rule_name(&code)?);
            }
        }

        Ok(Self { inner })
    }

    /// Sets the maximum line length (`None` removes the limit).
    fn with_max_line_length(&self, max: Option<i128>) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .with_max_line_length(parse_max_line_length(max)?),
        })
    }

    /// Sets the indentation size.
    fn with_indent_size(&self, size: i128) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .with_indent_size(parse_indent_size(size)?),
        })
    }

    /// Sets the maximum collection nesting depth (1..=512, default 256); `None` resets to the default.
    ///
    /// Depth 512 needs about 1 MiB of thread stack and can abort on stacks of 512 KiB or less.
    fn with_max_depth(&self, depth: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let parse_limits = ParseLimits {
            max_depth: limits::bounded::<Depth>("max_depth", depth)?,
            ..self.inner.parse_limits
        };
        Ok(Self {
            inner: self.inner.clone().with_parse_limits(parse_limits),
        })
    }

    /// Sets the alias-expansion budget in bytes (1..=1 GiB, default 64 MiB); `None` resets to the default.
    fn with_max_alias_bytes(&self, bytes: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let parse_limits = ParseLimits {
            max_alias_bytes: limits::bounded::<AliasBytes>("max_alias_bytes", bytes)?,
            ..self.inner.parse_limits
        };
        Ok(Self {
            inner: self.inner.clone().with_parse_limits(parse_limits),
        })
    }

    /// Sets the characters the parser may read past the last node (1..=1 Gi, default 4 Mi); `None` resets to the default.
    ///
    /// A flow collection at the root or in a `- ` entry, one scalar, or a run of comments longer than this is rejected.
    fn with_max_scan_ahead(&self, chars: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let parse_limits = ParseLimits {
            max_scan_ahead: limits::bounded::<ScanAhead>("max_scan_ahead", chars)?,
            ..self.inner.parse_limits
        };
        Ok(Self {
            inner: self.inner.clone().with_parse_limits(parse_limits),
        })
    }

    /// Sets the maximum number of documents in the source (1..=10M, default 100 000); `None` resets to the default.
    fn with_max_documents(&self, count: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let parse_limits = ParseLimits {
            max_documents: limits::bounded::<Documents>("max_documents", count)?,
            ..self.inner.parse_limits
        };
        Ok(Self {
            inner: self.inner.clone().with_parse_limits(parse_limits),
        })
    }

    /// Sets the largest source accepted for linting, in bytes (1..=1 GiB, default 100 MiB); `None` resets to the default.
    ///
    /// Bounds linting work on oversized input; the source is already in memory when checked, so this is not a memory bound.
    fn with_max_input_bytes(&self, bytes: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .with_max_input_bytes(limits::bounded::<InputBytes>("max_input_bytes", bytes)?),
        })
    }

    /// Disables a rule by code.
    ///
    /// Raises:
    ///     ValueError: If the rule code is unknown.
    fn with_disabled_rule(&self, code: &str) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .with_disabled_rule(parse_rule_name(code)?),
        })
    }

    /// Applies a per-rule configuration patch.
    ///
    /// Args:
    ///     code: Rule code (e.g. "line-length").
    ///     severity: Severity override ("error" | "warning" | "info" | "hint", case-insensitive) or None.
    ///     enabled: Whether the rule is enabled (None keeps the current state).
    ///     options: Rule-specific options with kebab-case keys (e.g. {"max": 120}) or None.
    ///
    /// Returns:
    ///     A new LintConfig with the patch applied.
    ///
    /// Raises:
    ///     ValueError: If the rule, severity, option key or option value is invalid.
    #[pyo3(signature = (code, severity=None, enabled=None, options=None))]
    fn with_rule_config(
        &self,
        code: &str,
        severity: Option<&str>,
        enabled: Option<bool>,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let name = parse_rule_name(code)?;
        let mut entry = match options.map(to_value).transpose()? {
            Some(Value::Mapping(mapping)) => mapping,
            Some(_) => return Err(PyValueError::new_err("options must be a mapping")),
            None => Mapping::new(),
        };
        for meta in ["enabled", "severity"] {
            if entry.contains_key(meta) {
                return Err(PyValueError::new_err(format!(
                    "'{meta}' is not a rule option; pass it as the '{meta}' argument"
                )));
            }
        }
        if let Some(severity) = severity {
            entry.insert("severity".into(), severity.into());
        }
        if let Some(enabled) = enabled {
            entry.insert("enabled".into(), enabled.into());
        }

        let mut inner = self.inner.clone();
        inner
            .rules
            .apply_rule(name, Value::Mapping(entry))
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Gets the maximum line length.
    #[getter]
    fn max_line_length(&self) -> Option<usize> {
        self.inner
            .rules
            .line_length
            .options
            .max
            .map(NonZeroUsize::get)
    }

    /// Gets the indentation size.
    #[getter]
    const fn indent_size(&self) -> usize {
        self.inner.rules.indentation.options.indent_size.get()
    }

    /// Gets the largest source accepted for linting, in bytes.
    #[getter]
    const fn max_input_bytes(&self) -> usize {
        self.inner.max_input_bytes.get()
    }

    fn __repr__(&self) -> String {
        let max = self
            .max_line_length()
            .map_or_else(|| "None".to_string(), |max| max.to_string());
        format!(
            "LintConfig(max_line_length={max}, indent_size={}, max_input_bytes={})",
            self.indent_size(),
            self.max_input_bytes()
        )
    }
}

/// YAML linter with configurable rules.
///
/// Orchestrates the linting process by parsing YAML source,
/// running enabled rules, and collecting diagnostics.
///
/// Examples:
///     >>> from `fast_yaml`._core.lint import Linter
///     >>> linter = `Linter.with_all_rules()`
///     >>> diagnostics = linter.lint("name: value\\nage: 30")
#[pyclass(module = "fast_yaml._core.lint", name = "Linter")]
pub struct PyLinter {
    inner: RustLinter,
}

#[pymethods]
impl PyLinter {
    #[new]
    #[pyo3(signature = (config=None))]
    fn new(config: Option<PyLintConfig>) -> Self {
        Self {
            inner: match config {
                Some(cfg) => RustLinter::with_config(cfg.inner),
                None => RustLinter::with_config(RustLintConfig::default()),
            },
        }
    }

    /// Creates a linter with all default rules enabled.
    #[staticmethod]
    fn with_all_rules() -> Self {
        Self {
            inner: RustLinter::with_all_rules(),
        }
    }

    /// Lints YAML source code.
    ///
    /// Args:
    ///     source: YAML source code as string
    ///
    /// Returns:
    ///     List of diagnostics (errors, warnings, hints)
    ///
    /// Raises:
    ///     `ValueError`: If YAML cannot be parsed at all, or the source exceeds `max_input_bytes`
    ///         (default 100 MiB)
    fn lint(&self, py: Python<'_>, source: &str) -> PyResult<Vec<PyDiagnostic>> {
        // Release GIL during CPU-intensive linting
        let result = py.detach(|| lint_with_excerpts(&self.inner, source));

        result
            .map(|diagnostics| diagnostics.into_iter().map(Into::into).collect())
            .map_err(|e| PyValueError::new_err(format!("Linting failed: {e}")))
    }

    #[allow(clippy::unused_self)] // PyO3 requires &self for __repr__
    fn __repr__(&self) -> String {
        "Linter()".to_string()
    }
}

/// Rust diagnostics with the excerpts the Python diagnostics carry.
fn given_findings(
    diagnostics: &Bound<'_, PyList>,
) -> PyResult<Vec<(RustDiagnostic, Option<RustDiagnosticContext>)>> {
    diagnostics
        .iter()
        .map(|item| {
            let py_diag: PyDiagnostic = item.extract()?;

            let context = py_diag.context.map(|py_ctx| RustDiagnosticContext {
                lines: py_ctx
                    .lines
                    .into_iter()
                    .map(|py_line| RustContextLine {
                        line_number: py_line.line_number,
                        content: py_line.content,
                        column_offset: py_line.column_offset,
                        truncated_end: py_line.truncated_end,
                        highlights: py_line.highlights,
                    })
                    .collect(),
            });

            let suggestions = py_diag
                .suggestions
                .into_iter()
                .map(|py_suggestion| RustSuggestion {
                    message: py_suggestion.message,
                    span: RustSpan::new(
                        RustLocation::new(
                            py_suggestion.span.start.line,
                            py_suggestion.span.start.column,
                            py_suggestion.span.start.offset,
                        ),
                        RustLocation::new(
                            py_suggestion.span.end.line,
                            py_suggestion.span.end.column,
                            py_suggestion.span.end.offset,
                        ),
                    ),
                    replacement: py_suggestion.replacement,
                })
                .collect();

            let diagnostic = RustDiagnostic {
                code: RustDiagnosticCode::new(py_diag.code),
                severity: py_diag.severity.inner,
                message: Cow::Owned(py_diag.message),
                span: RustSpan::new(
                    RustLocation::new(
                        py_diag.span.start.line,
                        py_diag.span.start.column,
                        py_diag.span.start.offset,
                    ),
                    RustLocation::new(
                        py_diag.span.end.line,
                        py_diag.span.end.column,
                        py_diag.span.end.offset,
                    ),
                ),
                excerpt: RustExcerpt::Omitted,
                suggestions,
            };
            Ok((diagnostic, context))
        })
        .collect()
}

/// Lints `source` and cuts the excerpt of every diagnostic from the same BOM-free text.
fn lint_with_excerpts(
    linter: &RustLinter,
    source: &str,
) -> Result<Vec<(RustDiagnostic, Option<RustDiagnosticContext>)>, RustLintError> {
    let input = linter.source(source)?;
    let diagnostics = linter.lint_source(&input)?;
    Ok(Findings::cut(diagnostics, &input.context()))
}

/// Format diagnostics as colored terminal output.
///
/// Converts diagnostics to human-readable text with optional ANSI colors.
#[pyclass(module = "fast_yaml._core.lint", name = "TextFormatter")]
pub struct PyTextFormatter {
    inner: RustTextFormatter,
}

#[pymethods]
impl PyTextFormatter {
    #[new]
    #[pyo3(signature = (use_colors=true))]
    const fn new(use_colors: bool) -> Self {
        Self {
            inner: RustTextFormatter::new().with_color(use_colors),
        }
    }

    /// Format diagnostics to human-readable text.
    ///
    /// Args:
    ///     diagnostics: List of diagnostics to format
    ///     source: Original YAML source code
    ///
    /// Returns:
    ///     Formatted string
    fn format(&self, diagnostics: &Bound<'_, PyList>, source: &str) -> PyResult<String> {
        let _ = source;
        let findings = given_findings(diagnostics)?;
        Ok(self.inner.format(Findings::Given(&findings)))
    }
}

#[cfg(feature = "json-output")]
#[pyclass(module = "fast_yaml._core.lint", name = "JsonFormatter")]
pub struct PyJsonFormatter {
    inner: RustJsonFormatter,
}

#[cfg(feature = "json-output")]
#[pymethods]
impl PyJsonFormatter {
    #[new]
    #[pyo3(signature = (pretty=false))]
    #[allow(clippy::missing_const_for_fn)] // RustJsonFormatter::new is not const
    fn new(pretty: bool) -> Self {
        Self {
            inner: RustJsonFormatter::new(pretty),
        }
    }

    fn format(&self, diagnostics: &Bound<'_, PyList>, source: &str) -> PyResult<String> {
        let _ = source;
        let findings = given_findings(diagnostics)?;
        Ok(self.inner.format(Findings::Given(&findings)))
    }
}

/// Lint YAML source with optional configuration.
///
/// Convenience function equivalent to Linter(config).lint(source).
///
/// Args:
///     source: YAML source code
///     config: Optional linter configuration
///
/// Returns:
///     List of diagnostics
///
/// Raises:
///     `ValueError`: If YAML cannot be parsed at all, or the source exceeds `max_input_bytes`
///         (default 100 MiB)
///
/// Example:
///     >>> from `fast_yaml`._core.lint import lint
///     >>> diagnostics = lint("key: value\\nkey: duplicate")
///     >>> for diag in diagnostics:
///     ...     `print(f"{diag.severity.as_str()}`: {diag.message}")
///     error: duplicate key 'key' found
#[pyfunction]
#[pyo3(signature = (source, config=None))]
fn lint(py: Python<'_>, source: &str, config: Option<PyLintConfig>) -> PyResult<Vec<PyDiagnostic>> {
    // Release GIL during CPU-intensive linting
    let result = py.detach(|| {
        let linter = match config {
            Some(cfg) => RustLinter::with_config(cfg.inner),
            None => RustLinter::with_all_rules(),
        };
        lint_with_excerpts(&linter, source)
    });

    result
        .map(|diagnostics| diagnostics.into_iter().map(Into::into).collect())
        .map_err(|e| PyValueError::new_err(format!("Linting failed: {e}")))
}

/// Format diagnostics to string.
///
/// Args:
///     diagnostics: List of diagnostics
///     source: Original YAML source
///     format: Output format ("text" or "json")
///     `use_colors`: Use ANSI colors for text output
///
/// Returns:
///     Formatted string
#[pyfunction]
#[pyo3(signature = (diagnostics, source, format="text", use_colors=true))]
fn format_diagnostics(
    diagnostics: &Bound<'_, PyList>,
    source: &str,
    format: &str,
    use_colors: bool,
) -> PyResult<String> {
    match format {
        "text" => {
            let formatter = PyTextFormatter::new(use_colors);
            formatter.format(diagnostics, source)
        }
        #[cfg(feature = "json-output")]
        "json" => {
            let formatter = PyJsonFormatter::new(false); // default: compact JSON
            formatter.format(diagnostics, source)
        }
        _ => Err(PyValueError::new_err(format!(
            "Unknown format '{format}', use 'text' or 'json'"
        ))),
    }
}

/// Register the lint submodule.
pub fn register_lint_module(py: Python<'_>, parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    let lint_module = PyModule::new(py, "lint")?;

    lint_module.add_class::<PySeverity>()?;
    lint_module.add_class::<PyLocation>()?;
    lint_module.add_class::<PySpan>()?;
    lint_module.add_class::<PyContextLine>()?;
    lint_module.add_class::<PyDiagnosticContext>()?;
    lint_module.add_class::<PySuggestion>()?;
    lint_module.add_class::<PyDiagnostic>()?;
    lint_module.add_class::<PyLintConfig>()?;
    lint_module.add_class::<PyLinter>()?;
    lint_module.add_class::<PyTextFormatter>()?;

    #[cfg(feature = "json-output")]
    lint_module.add_class::<PyJsonFormatter>()?;

    lint_module.add_function(wrap_pyfunction!(lint, &lint_module)?)?;
    lint_module.add_function(wrap_pyfunction!(format_diagnostics, &lint_module)?)?;

    parent_module.add_submodule(&lint_module)?;
    Ok(())
}
