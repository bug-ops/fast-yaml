"""Type stubs for fast_yaml._core"""

from __future__ import annotations

from typing import Any

# =============================================================================
# Loader Classes (PyYAML compatibility)
# =============================================================================

class SafeLoader:
    """Safe YAML loader (recommended for untrusted input).

    This is the default and recommended loader. It only loads basic Python
    types and is safe against arbitrary code execution.
    """

    ...

class FullLoader:
    """Full YAML loader with most features.

    Note: Currently behaves identically to SafeLoader for security.
    """

    ...

class Loader:
    """Full YAML loader (PyYAML compatibility).

    Note: Currently behaves identically to SafeLoader for security.
    """

    ...

# =============================================================================
# Dumper Classes (PyYAML compatibility)
# =============================================================================

class SafeDumper:
    """Safe YAML dumper (recommended).

    This is the default and recommended dumper. It only serializes basic Python
    types and is safe against arbitrary code execution.
    """

    ...

class Dumper:
    """Full YAML dumper (PyYAML compatibility).

    Note: Currently behaves identically to SafeDumper for security.
    """

    ...

# =============================================================================
# Exception Hierarchy (PyYAML compatibility)
# =============================================================================

class YAMLError(Exception):
    """Base exception for all YAML errors."""

    ...

class MarkedYAMLError(YAMLError):
    """YAML error with source location information."""

    context: str | None
    context_mark: "Mark | None"
    problem: str | None
    problem_mark: "Mark | None"
    note: str | None

    def __init__(
        self,
        context: str | None = None,
        context_mark: "Mark | None" = None,
        problem: str | None = None,
        problem_mark: "Mark | None" = None,
        note: str | None = None,
    ) -> None: ...

class ScannerError(MarkedYAMLError):
    """Error during YAML scanning (lexical analysis)."""

    ...

class ParserError(MarkedYAMLError):
    """Error during YAML parsing (syntax analysis)."""

    ...

class ComposerError(MarkedYAMLError):
    """Error during YAML composition (building node graph)."""

    ...

class ConstructorError(MarkedYAMLError):
    """Error during YAML construction (building Python objects)."""

    ...

class EmitterError(YAMLError):
    """Error during YAML emission."""

    ...

# =============================================================================
# Mark Class (error location tracking)
# =============================================================================

class Mark:
    """Represents a position in a YAML source file.

    Used to indicate where errors occur during parsing.
    """

    name: str
    line: int
    column: int

    def __init__(self, name: str, line: int, column: int) -> None: ...
    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

# Core parsing functions
def safe_load(yaml_str: str) -> Any:
    """Parse a YAML string and return a Python object.

    Args:
        yaml_str: A YAML document as a string

    Returns:
        The parsed YAML document as Python objects

    Raises:
        ValueError: If the YAML is invalid or input exceeds 100MB limit
    """
    ...

def safe_load_all(yaml_str: str) -> list[Any]:
    """Parse a YAML string containing multiple documents.

    Args:
        yaml_str: A YAML string potentially containing multiple documents

    Returns:
        A list of parsed YAML documents

    Raises:
        ValueError: If the YAML is invalid or input exceeds 100MB limit
    """
    ...

def safe_dump(
    data: Any,
    allow_unicode: bool = True,
    sort_keys: bool = False,
) -> str:
    """Serialize a Python object to a YAML string.

    Args:
        data: A Python object to serialize
        allow_unicode: If True, allow unicode in output (currently always enabled)
        sort_keys: If True, sort dictionary keys

    Returns:
        A YAML string representation of the object

    Raises:
        TypeError: If the object cannot be serialized

    Note:
        The allow_unicode parameter is accepted for PyYAML compatibility,
        but yaml-rust2 always outputs unicode characters.
    """
    ...

def safe_dump_all(documents: Any) -> str:
    """Serialize multiple Python objects to a YAML string.

    Args:
        documents: An iterable of Python objects to serialize

    Returns:
        A YAML string with multiple documents separated by '---'

    Raises:
        TypeError: If any object cannot be serialized
    """
    ...

def version() -> str:
    """Get the version of the fast-yaml library."""
    ...

def load(
    yaml_str: str,
    loader: type | None = None,
) -> Any:
    """Parse a YAML string with an optional Loader (PyYAML compatible).

    Args:
        yaml_str: A YAML document as a string
        loader: Optional loader class (SafeLoader, FullLoader, or Loader)

    Returns:
        The parsed YAML document as Python objects

    Raises:
        YAMLError: If the YAML is invalid
    """
    ...

def load_all(
    yaml_str: str,
    loader: type | None = None,
) -> list[Any]:
    """Parse multiple YAML documents with an optional Loader (PyYAML compatible).

    Args:
        yaml_str: A YAML string potentially containing multiple documents
        loader: Optional loader class (SafeLoader, FullLoader, or Loader)

    Returns:
        A list of parsed YAML documents

    Raises:
        YAMLError: If the YAML is invalid
    """
    ...

def dump(
    data: Any,
    dumper: type | None = None,
    *,
    allow_unicode: bool = True,
    sort_keys: bool = False,
    indent: int = 2,
    width: int = 80,
    explicit_start: bool = False,
) -> str:
    """Serialize a Python object to YAML with an optional Dumper (PyYAML compatible).

    Args:
        data: A Python object to serialize
        dumper: Optional dumper class (SafeDumper or Dumper)

    Returns:
        A YAML string representation of the object

    Raises:
        TypeError: If the object cannot be serialized
    """
    ...

def dump_all(
    documents: list[Any],
    dumper: type | None = None,
    *,
    allow_unicode: bool = True,
    sort_keys: bool = False,
    indent: int = 2,
    width: int = 80,
    explicit_start: bool = False,
) -> str:
    """Serialize multiple Python objects to YAML (PyYAML compatible).

    Args:
        documents: A list of Python objects to serialize
        dumper: Optional dumper class (SafeDumper or Dumper)

    Returns:
        A YAML string with multiple documents separated by '---'

    Raises:
        TypeError: If any object cannot be serialized
    """
    ...

# Lint submodule (PyO3 submodule, not a class - noqa: N801)
class lint:  # noqa: N801
    """YAML linting submodule."""

    class Severity:
        """Diagnostic severity levels."""

        ERROR: str
        WARNING: str
        INFO: str
        HINT: str

        def as_str(self) -> str: ...
        def __str__(self) -> str: ...
        def __repr__(self) -> str: ...
        def __eq__(self, other: object) -> bool: ...
        def __hash__(self) -> int: ...

    class Location:
        """A position in the source file."""

        line: int
        column: int
        offset: int

        def __init__(self, line: int, column: int, offset: int) -> None: ...
        def __repr__(self) -> str: ...
        def __eq__(self, other: object) -> bool: ...

    class Span:
        """A span of text in the source file."""

        start: "lint.Location"
        end: "lint.Location"

        def __init__(self, start: "lint.Location", end: "lint.Location") -> None: ...
        def __repr__(self) -> str: ...
        def __eq__(self, other: object) -> bool: ...

    class ContextLine:
        """A single line of source context."""

        line_number: int
        content: str
        highlights: list[tuple[int, int]]

        def __init__(
            self, line_number: int, content: str, highlights: list[tuple[int, int]]
        ) -> None: ...
        def __repr__(self) -> str: ...

    class DiagnosticContext:
        """Source code context for diagnostics."""

        lines: list["lint.ContextLine"]

        def __init__(self, lines: list["lint.ContextLine"]) -> None: ...
        def __repr__(self) -> str: ...

    class Suggestion:
        """A suggested fix for a diagnostic."""

        message: str
        span: "lint.Span"
        replacement: str | None

        def __init__(
            self, message: str, span: "lint.Span", replacement: str | None = None
        ) -> None: ...
        def __repr__(self) -> str: ...

    class Diagnostic:
        """A diagnostic message with location and context."""

        code: str
        severity: "lint.Severity"
        message: str
        span: "lint.Span"
        context: "lint.DiagnosticContext | None"
        suggestions: list["lint.Suggestion"]

        def __repr__(self) -> str: ...

    class LintConfig:
        """Configuration for the linter."""

        max_line_length: int | None
        indent_size: int

        def __init__(
            self,
            max_line_length: int | None = 80,
            indent_size: int = 2,
            require_document_start: bool = False,
            require_document_end: bool = False,
            allow_duplicate_keys: bool = False,
            disabled_rules: set[str] | None = None,
        ) -> None: ...
        def with_max_line_length(self, max: int | None) -> "lint.LintConfig": ...
        def with_indent_size(self, size: int) -> "lint.LintConfig": ...
        def with_disabled_rule(self, code: str) -> "lint.LintConfig": ...
        def __repr__(self) -> str: ...

    class Linter:
        """YAML linter with configurable rules."""

        def __init__(self, config: "lint.LintConfig | None" = None) -> None: ...
        @staticmethod
        def with_all_rules() -> "lint.Linter": ...
        def lint(self, source: str) -> list["lint.Diagnostic"]: ...
        def __repr__(self) -> str: ...

    class TextFormatter:
        """Format diagnostics as colored terminal output."""

        def __init__(self, use_colors: bool = True) -> None: ...
        def format(self, diagnostics: list["lint.Diagnostic"], source: str) -> str: ...

    class JsonFormatter:
        """Format diagnostics as JSON (requires json-output feature)."""

        def __init__(self, pretty: bool = False) -> None: ...
        def format(self, diagnostics: list["lint.Diagnostic"], source: str) -> str: ...

    @staticmethod
    def lint(source: str, config: "lint.LintConfig | None" = None) -> list["lint.Diagnostic"]:
        """Lint YAML source with optional configuration."""
        ...

    @staticmethod
    def format_diagnostics(
        diagnostics: "list[lint.Diagnostic]",  # type: ignore[name-defined]
        source: str,
        format: str = "text",
        use_colors: bool = True,
    ) -> str:
        """Format diagnostics to string."""
        ...

# Parallel submodule (PyO3 submodule, not a class - noqa: N801)
class parallel:  # noqa: N801
    """Parallel YAML processing submodule."""

    class ParallelConfig:
        """Configuration for parallel YAML processing."""

        def __init__(
            self,
            thread_count: int | None = None,
            min_chunk_size: int = 4096,
            max_chunk_size: int = 10 * 1024 * 1024,
            max_input_size: int = 100 * 1024 * 1024,
            max_documents: int = 100_000,
        ) -> None: ...
        def with_thread_count(self, count: int | None) -> "parallel.ParallelConfig": ...
        def with_max_input_size(self, size: int) -> "parallel.ParallelConfig": ...
        def with_max_documents(self, count: int) -> "parallel.ParallelConfig": ...
        def with_min_chunk_size(self, size: int) -> "parallel.ParallelConfig": ...
        def with_max_chunk_size(self, size: int) -> "parallel.ParallelConfig": ...
        def __repr__(self) -> str: ...

    @staticmethod
    def parse_parallel(source: str, config: "parallel.ParallelConfig | None" = None) -> list[Any]:
        """Parse multi-document YAML in parallel.

        Args:
            source: YAML source potentially containing multiple documents
            config: Optional parallel processing configuration

        Returns:
            List of parsed YAML documents

        Raises:
            ValueError: If parsing fails or limits exceeded
        """
        ...
