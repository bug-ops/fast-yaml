//! `PyO3` bindings for fast-yaml-linter.
//!
//! Exposes the YAML linter API to Python with comprehensive diagnostics,
//! rich error reporting, and configurable linting rules.

use fast_yaml_linter::{
    ContextLine as RustContextLine, Diagnostic as RustDiagnostic,
    DiagnosticCode as RustDiagnosticCode, DiagnosticContext as RustDiagnosticContext,
    Formatter as RustFormatter, LintConfig as RustLintConfig, Linter as RustLinter,
    Location as RustLocation, Severity as RustSeverity, Span as RustSpan,
    Suggestion as RustSuggestion, TextFormatter as RustTextFormatter,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyList, PySet};
use std::collections::HashSet;

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
#[pyclass(module = "fast_yaml._core.lint", name = "Severity")]
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
#[pyclass(module = "fast_yaml._core.lint", name = "Location")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PyLocation {
    /// Line number (1-indexed, human-readable).
    #[pyo3(get)]
    pub line: usize,

    /// Column number (1-indexed, human-readable).
    #[pyo3(get)]
    pub column: usize,

    /// Byte offset from the start of the file (0-indexed).
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
#[pyclass(module = "fast_yaml._core.lint", name = "Span")]
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
#[pyclass(module = "fast_yaml._core.lint", name = "ContextLine")]
#[derive(Clone)]
pub struct PyContextLine {
    /// Line number (1-indexed).
    #[pyo3(get)]
    pub line_number: usize,

    /// Source text content.
    #[pyo3(get)]
    pub content: String,

    /// Highlight ranges (column start, column end).
    #[pyo3(get)]
    pub highlights: Vec<(usize, usize)>,
}

#[pymethods]
impl PyContextLine {
    #[new]
    #[pyo3(signature = (line_number, content, highlights))]
    const fn new(line_number: usize, content: String, highlights: Vec<(usize, usize)>) -> Self {
        Self {
            line_number,
            content,
            highlights,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "ContextLine(line_number={}, content={:?}, highlights={:?})",
            self.line_number, self.content, self.highlights
        )
    }
}

// Note: RustContextLine is not publicly exported, so we can't directly convert
// We'll create PyContextLine from the DiagnosticContext fields instead

/// Source code context for diagnostics.
#[pyclass(module = "fast_yaml._core.lint", name = "DiagnosticContext")]
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
                highlights: line.highlights,
            })
            .collect();

        Self { lines }
    }
}

/// A suggested fix for a diagnostic.
#[pyclass(module = "fast_yaml._core.lint", name = "Suggestion")]
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
#[pyclass(module = "fast_yaml._core.lint", name = "Diagnostic")]
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

impl From<RustDiagnostic> for PyDiagnostic {
    fn from(diagnostic: RustDiagnostic) -> Self {
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
            message: diagnostic.message,
            span: diagnostic.span.into(),
            context: diagnostic.context.map(Into::into),
            suggestions,
        }
    }
}

/// Configuration for the linter.
///
/// Controls linting behavior including rule enablement,
/// formatting preferences, and validation strictness.
///
/// Examples:
///     >>> from `fast_yaml`._core.lint import `LintConfig`
///     >>> config = `LintConfig(max_line_length=120`, `indent_size=4`)
///     >>> config = config.with_disabled_rule("line-length")
#[pyclass(module = "fast_yaml._core.lint", name = "LintConfig")]
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
        disabled_rules=None
    ))]
    fn new(
        max_line_length: Option<usize>,
        indent_size: usize,
        require_document_start: bool,
        require_document_end: bool,
        allow_duplicate_keys: bool,
        disabled_rules: Option<Bound<'_, PySet>>,
    ) -> PyResult<Self> {
        // Validate indent_size (must be between 1 and 16)
        if indent_size == 0 || indent_size > 16 {
            return Err(PyValueError::new_err(
                "indent_size must be between 1 and 16 (inclusive)",
            ));
        }

        // Validate max_line_length if provided (must be between 1 and 1000)
        if let Some(max) = max_line_length
            && (max == 0 || max > 1000)
        {
            return Err(PyValueError::new_err(
                "max_line_length must be between 1 and 1000 (inclusive)",
            ));
        }

        let mut disabled_rules_set = HashSet::new();
        if let Some(rules) = disabled_rules {
            for rule in rules.iter() {
                let rule_str: String = rule.extract()?;
                disabled_rules_set.insert(rule_str);
            }
        }

        Ok(Self {
            inner: RustLintConfig {
                max_line_length,
                indent_size,
                require_document_start,
                require_document_end,
                allow_duplicate_keys,
                disabled_rules: disabled_rules_set,
            },
        })
    }

    /// Sets the maximum line length.
    fn with_max_line_length(&self, max: Option<usize>) -> PyResult<Self> {
        // Validate max_line_length if provided
        if let Some(max_val) = max
            && (max_val == 0 || max_val > 1000)
        {
            return Err(PyValueError::new_err(
                "max_line_length must be between 1 and 1000 (inclusive)",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().with_max_line_length(max),
        })
    }

    /// Sets the indentation size.
    fn with_indent_size(&self, size: usize) -> PyResult<Self> {
        // Validate indent_size
        if size == 0 || size > 16 {
            return Err(PyValueError::new_err(
                "indent_size must be between 1 and 16 (inclusive)",
            ));
        }
        Ok(Self {
            inner: self.inner.clone().with_indent_size(size),
        })
    }

    /// Disables a rule by code.
    fn with_disabled_rule(&self, code: &str) -> Self {
        Self {
            inner: self.inner.clone().with_disabled_rule(code),
        }
    }

    /// Gets the maximum line length.
    #[getter]
    const fn max_line_length(&self) -> Option<usize> {
        self.inner.max_line_length
    }

    /// Gets the indentation size.
    #[getter]
    const fn indent_size(&self) -> usize {
        self.inner.indent_size
    }

    fn __repr__(&self) -> String {
        format!(
            "LintConfig(max_line_length={:?}, indent_size={})",
            self.inner.max_line_length, self.inner.indent_size
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
                None => RustLinter::new(),
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
    ///     `ValueError`: If YAML cannot be parsed at all
    fn lint(&self, py: Python<'_>, source: &str) -> PyResult<Vec<PyDiagnostic>> {
        // Release GIL during CPU-intensive linting
        let result = py.detach(|| self.inner.lint(source));

        result
            .map(|diagnostics| diagnostics.into_iter().map(Into::into).collect())
            .map_err(|e| PyValueError::new_err(format!("Linting failed: {e}")))
    }

    #[allow(clippy::unused_self)] // PyO3 requires &self for __repr__
    fn __repr__(&self) -> String {
        "Linter()".to_string()
    }
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
        let rust_diagnostics: Vec<RustDiagnostic> = diagnostics
            .iter()
            .map(|item| {
                let py_diag: PyDiagnostic = item.extract()?;

                // Convert context back to Rust type
                let context = py_diag.context.map(|py_ctx| RustDiagnosticContext {
                    lines: py_ctx
                        .lines
                        .into_iter()
                        .map(|py_line| RustContextLine {
                            line_number: py_line.line_number,
                            content: py_line.content,
                            highlights: py_line.highlights,
                        })
                        .collect(),
                });

                // Convert suggestions back to Rust type
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

                Ok(RustDiagnostic {
                    code: RustDiagnosticCode::new(py_diag.code),
                    severity: py_diag.severity.inner,
                    message: py_diag.message,
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
                    context,
                    suggestions,
                })
            })
            .collect::<PyResult<Vec<_>>>()?;

        Ok(self.inner.format(&rust_diagnostics, source))
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
        let rust_diagnostics: Vec<RustDiagnostic> = diagnostics
            .iter()
            .map(|item| {
                let py_diag: PyDiagnostic = item.extract()?;

                // Convert context back to Rust type
                let context = py_diag.context.map(|py_ctx| RustDiagnosticContext {
                    lines: py_ctx
                        .lines
                        .into_iter()
                        .map(|py_line| RustContextLine {
                            line_number: py_line.line_number,
                            content: py_line.content,
                            highlights: py_line.highlights,
                        })
                        .collect(),
                });

                // Convert suggestions back to Rust type
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

                Ok(RustDiagnostic {
                    code: RustDiagnosticCode::new(py_diag.code),
                    severity: py_diag.severity.inner,
                    message: py_diag.message,
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
                    context,
                    suggestions,
                })
            })
            .collect::<PyResult<Vec<_>>>()?;

        Ok(self.inner.format(&rust_diagnostics, source))
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
        linter.lint(source)
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
