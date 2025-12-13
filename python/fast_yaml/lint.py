"""
YAML linting with rich diagnostics.

This module provides comprehensive YAML validation with detailed error
reporting, source context, and suggested fixes.

Example:
    >>> import fast_yaml.lint as yaml_lint
    >>> diagnostics = yaml_lint.lint("key: value\\nkey: duplicate")
    >>> for d in diagnostics:
    ...     print(f"{d.severity.as_str()}: {d.message}")
    error: duplicate key 'key' found
"""

from __future__ import annotations

from ._core import lint as _lint_module

# Re-export from the submodule
ContextLine = _lint_module.ContextLine
Diagnostic = _lint_module.Diagnostic
DiagnosticContext = _lint_module.DiagnosticContext
LintConfig = _lint_module.LintConfig
Linter = _lint_module.Linter
Location = _lint_module.Location
Severity = _lint_module.Severity
Span = _lint_module.Span
Suggestion = _lint_module.Suggestion
TextFormatter = _lint_module.TextFormatter
_format_diagnostics = _lint_module.format_diagnostics
_lint = _lint_module.lint

__all__ = [
    "Severity",
    "Location",
    "Span",
    "ContextLine",
    "DiagnosticContext",
    "Suggestion",
    "Diagnostic",
    "LintConfig",
    "Linter",
    "TextFormatter",
    "lint",
    "format_diagnostics",
]


def lint(source: str, config: LintConfig | None = None) -> list[Diagnostic]:
    """
    Lint YAML source code.

    Validates YAML syntax, structure, and style according to configurable
    rules. Returns a list of diagnostics with precise locations and context.

    Args:
        source: YAML source code as string
        config: Optional linter configuration

    Returns:
        List of diagnostics sorted by location

    Raises:
        ValueError: If YAML is completely unparseable

    Example:
        >>> import fast_yaml.lint as yaml_lint
        >>> source = "key: value\\nkey: duplicate"
        >>> diagnostics = yaml_lint.lint(source)
        >>> for d in diagnostics:
        ...     print(f"{d.severity.as_str()}: {d.message}")
        error: duplicate key 'key' found
    """
    return _lint(source, config)


def format_diagnostics(
    diagnostics: list[Diagnostic],
    source: str,
    format: str = "text",
    use_colors: bool = True,
) -> str:
    """
    Format diagnostics to string.

    Args:
        diagnostics: List of diagnostics to format
        source: Original YAML source code
        format: Output format ("text" or "json")
        use_colors: Use ANSI colors for text format

    Returns:
        Formatted diagnostic output

    Example:
        >>> output = format_diagnostics(diagnostics, source, format="text")
        >>> print(output)
        Error: duplicate key 'key' found
          --> input.yaml:2:1
           |
         1 | key: value
         2 | key: duplicate
           | ^^^ duplicate key
    """
    return _format_diagnostics(diagnostics, source, format, use_colors)
