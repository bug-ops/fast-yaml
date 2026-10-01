"""Type stubs for fast_yaml._core"""

from __future__ import annotations

import os
from collections.abc import Mapping
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
def safe_load(
    yaml_str: str,
    *,
    max_depth: int | None = None,
    max_alias_bytes: int | None = None,
    max_scan_ahead: int | None = None,
    max_documents: int | None = None,
) -> Any:
    """Parse a YAML string and return a Python object.

    Args:
        yaml_str: A YAML document as a string
        max_depth: Maximum collection nesting depth, 1..=512 (default: 256). Depth 512 needs
            about 1 MiB of thread stack and can abort on stacks of 512 KiB or less; 256 is safe.
            The dumper keeps a fixed depth of 256, so deeper data may fail to dump.
        max_alias_bytes: Alias-expansion budget in bytes, 1..=1 GiB (default: 64 MiB)
        max_scan_ahead: Characters the parser may read past the last node, 1..=1 Gi (default: 4 Mi).
        max_documents: Maximum documents in the stream, 1..=10M (default: 100 000);
            more raises ``ValueError``.

    Returns:
        The parsed YAML document as Python objects

    Raises:
        ValueError: If the YAML is invalid, input exceeds 100MB limit, a limit is out of range,
            or it holds a decimal integer beyond the i64 range with more digits than
                ``sys.get_int_max_str_digits()`` (CPython's ``int()`` limit;
            ``sys.set_int_max_str_digits()`` raises it)
    """
    ...

def safe_load_all(
    yaml_str: str,
    *,
    max_depth: int | None = None,
    max_alias_bytes: int | None = None,
    max_scan_ahead: int | None = None,
    max_documents: int | None = None,
) -> list[Any]:
    """Parse a YAML string containing multiple documents.

    Args:
        yaml_str: A YAML string potentially containing multiple documents
        max_depth: Maximum collection nesting depth, 1..=512 (default: 256). Depth 512 needs
            about 1 MiB of thread stack and can abort on stacks of 512 KiB or less; 256 is safe.
            The dumper keeps a fixed depth of 256, so deeper data may fail to dump.
        max_alias_bytes: Alias-expansion budget in bytes, 1..=1 GiB (default: 64 MiB)
        max_scan_ahead: Characters the parser may read past the last node, 1..=1 Gi (default: 4 Mi).
        max_documents: Maximum documents in the stream, 1..=10M (default: 100 000);
            more raises ``ValueError``.

    Returns:
        A list of parsed YAML documents

    Raises:
        ValueError: If the YAML is invalid, input exceeds 100MB limit, a limit is out of range,
            or it holds a decimal integer beyond the i64 range with more digits than
                ``sys.get_int_max_str_digits()`` (CPython's ``int()`` limit;
            ``sys.set_int_max_str_digits()`` raises it)
    """
    ...

def safe_dump(
    data: Any,
    allow_unicode: bool = True,
    sort_keys: bool = False,
    indent: int = 2,
    width: int = 80,
    default_flow_style: bool | None = None,
    explicit_start: bool = False,
) -> str:
    """Serialize a Python object to a YAML string.

    Args:
        data: A Python object to serialize
        allow_unicode: If True, allow unicode in output (currently always enabled)
        sort_keys: If True, sort dictionary keys
        indent: Number of spaces for indentation (default: 2)
        width: Maximum line width (default: 80)
        default_flow_style: Force flow style for collections (default: None)
        explicit_start: Add explicit document start marker (default: False)

    Returns:
        A YAML string representation of the object

    Raises:
        TypeError: If the object cannot be serialized
        ValueError: If an int has more digits than ``sys.get_int_max_str_digits()`` (CPython's
            limit; ``sys.set_int_max_str_digits()`` raises it)

    Note:
        The allow_unicode parameter is accepted for PyYAML compatibility,
        but yaml-rust2 always outputs unicode characters.
    """
    ...

def safe_dump_all(
    documents: Any,
    allow_unicode: bool = True,
    sort_keys: bool = False,
    indent: int = 2,
    width: int = 80,
    default_flow_style: bool | None = None,
    explicit_start: bool = False,
) -> str:
    """Serialize multiple Python objects to a YAML string.

    Args:
        documents: An iterable of Python objects to serialize
        allow_unicode: If True, allow unicode in output (currently always enabled)
        sort_keys: If True, sort dictionary keys
        indent: Number of spaces for indentation (default: 2)
        width: Maximum line width (default: 80)
        default_flow_style: Force flow style for collections (default: None)
        explicit_start: Add explicit document start marker (default: False)

    Returns:
        A YAML string with multiple documents separated by '---'

    Raises:
        TypeError: If any object cannot be serialized
        ValueError: If an int has more digits than ``sys.get_int_max_str_digits()`` (CPython's
            limit; ``sys.set_int_max_str_digits()`` raises it)
    """
    ...

def safe_dump_to(
    data: Any,
    stream: Any,
    allow_unicode: bool = True,
    sort_keys: bool = False,
    indent: int = 2,
    width: int = 80,
    default_flow_style: bool | None = None,
    explicit_start: bool = False,
    chunk_size: int = 8192,
) -> int:
    """Dump YAML directly to a stream without intermediate string.

    This function is memory-efficient for large documents as it streams
    YAML output in chunks rather than building the entire string in memory.

    Args:
        data: Python object to serialize
        stream: File-like object with write() method
        allow_unicode: Allow unicode characters (default: True)
        sort_keys: Sort dictionary keys (default: False)
        indent: Number of spaces for indentation (default: 2)
        width: Maximum line width (default: 80)
        default_flow_style: Force flow style for collections (default: None)
        explicit_start: Add explicit document start marker (default: False)
        chunk_size: Size of write chunks in bytes (default: 8KB)

    Returns:
        Number of bytes written

    Raises:
        TypeError: If object cannot be serialized or stream invalid
        IOError: If write fails

    Example:
        >>> with open('output.yaml', 'w') as f:
        ...     bytes_written = fast_yaml.safe_dump_to({'key': 'value'}, f)
    """
    ...

def version() -> str:
    """Get the version of the fast-yaml library."""
    ...

def load(
    yaml_str: str,
    loader: type | None = None,
    *,
    max_depth: int | None = None,
    max_alias_bytes: int | None = None,
    max_scan_ahead: int | None = None,
    max_documents: int | None = None,
) -> Any:
    """Parse a YAML string with an optional Loader (PyYAML compatible).

    Args:
        yaml_str: A YAML document as a string
        loader: Optional loader class (SafeLoader, FullLoader, or Loader)
        max_depth: Maximum collection nesting depth, 1..=512 (default: 256). Depth 512 needs
            about 1 MiB of thread stack and can abort on stacks of 512 KiB or less; 256 is safe.
            The dumper keeps a fixed depth of 256, so deeper data may fail to dump.
        max_alias_bytes: Alias-expansion budget in bytes, 1..=1 GiB (default: 64 MiB)
        max_scan_ahead: Characters the parser may read past the last node, 1..=1 Gi (default: 4 Mi).
        max_documents: Maximum documents in the stream, 1..=10M (default: 100 000);
            more raises ``ValueError``.

    Returns:
        The parsed YAML document as Python objects

    Raises:
        YAMLError: If the YAML is invalid
        ValueError: If the YAML holds a decimal integer beyond the i64 range with more digits than
            ``sys.get_int_max_str_digits()`` (CPython's ``int()`` limit;
                ``sys.set_int_max_str_digits()``
            raises it)
    """
    ...

def load_all(
    yaml_str: str,
    loader: type | None = None,
    *,
    max_depth: int | None = None,
    max_alias_bytes: int | None = None,
    max_scan_ahead: int | None = None,
    max_documents: int | None = None,
) -> list[Any]:
    """Parse multiple YAML documents with an optional Loader (PyYAML compatible).

    Args:
        yaml_str: A YAML string potentially containing multiple documents
        loader: Optional loader class (SafeLoader, FullLoader, or Loader)
        max_depth: Maximum collection nesting depth, 1..=512 (default: 256). Depth 512 needs
            about 1 MiB of thread stack and can abort on stacks of 512 KiB or less; 256 is safe.
            The dumper keeps a fixed depth of 256, so deeper data may fail to dump.
        max_alias_bytes: Alias-expansion budget in bytes, 1..=1 GiB (default: 64 MiB)
        max_scan_ahead: Characters the parser may read past the last node, 1..=1 Gi (default: 4 Mi).
        max_documents: Maximum documents in the stream, 1..=10M (default: 100 000);
            more raises ``ValueError``.

    Returns:
        A list of parsed YAML documents

    Raises:
        YAMLError: If the YAML is invalid
        ValueError: If the YAML holds a decimal integer beyond the i64 range with more digits than
            ``sys.get_int_max_str_digits()`` (CPython's ``int()`` limit;
                ``sys.set_int_max_str_digits()``
            raises it)
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
        """A position in the source text.

        ``offset`` is a byte offset in the text with document-prefix BOMs removed
        (also for suggestion spans); ``line`` and ``column`` refer to the same text.
        """

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
        column_offset: int
        truncated_end: bool
        highlights: list[tuple[int, int]]

        def __init__(
            self,
            line_number: int,
            content: str,
            highlights: list[tuple[int, int]],
            column_offset: int = 0,
            truncated_end: bool = False,
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
        """Configuration for the linter.

        ``rules`` maps rule codes to a severity string or an entry mapping with
        optional ``severity`` (also spelled ``level``, not both), ``enabled`` and
        kebab-case option keys
        (e.g. ``{"line-length": {"max": 120}}``). Unknown rules, option keys,
        wrong types and invalid severities raise ``ValueError``. Order of
        application: keyword arguments, then ``rules``, then ``disabled_rules``.

        ``max_input_bytes`` (1..=1 GiB, default 100 MiB) rejects larger sources
        with ``ValueError``. It bounds linting work on oversized input; the
        source is already in memory when checked, so it is not a memory bound.
        """

        max_line_length: int | None
        indent_size: int
        max_input_bytes: int

        def __init__(
            self,
            max_line_length: int | None = 80,
            indent_size: int = 2,
            require_document_start: bool = False,
            require_document_end: bool = False,
            allow_duplicate_keys: bool = False,
            disabled_rules: set[str] | list[str] | tuple[str, ...] | None = None,
            rules: Mapping[str, str | Mapping[str, object]] | None = None,
            max_depth: int | None = None,
            max_alias_bytes: int | None = None,
            max_scan_ahead: int | None = None,
            max_input_bytes: int | None = None,
            max_documents: int | None = None,
        ) -> None: ...
        def with_max_depth(self, depth: int | None) -> "lint.LintConfig": ...
        def with_max_alias_bytes(self, bytes: int | None) -> "lint.LintConfig": ...
        def with_max_scan_ahead(self, chars: int | None) -> "lint.LintConfig": ...
        def with_max_documents(self, count: int | None) -> "lint.LintConfig": ...
        def with_max_input_bytes(self, bytes: int | None) -> "lint.LintConfig": ...
        def with_max_line_length(self, max: int | None) -> "lint.LintConfig": ...
        def with_indent_size(self, size: int) -> "lint.LintConfig": ...
        def with_disabled_rule(self, code: str) -> "lint.LintConfig": ...
        def with_rule_config(
            self,
            code: str,
            severity: str | None = None,
            enabled: bool | None = None,
            options: Mapping[str, object] | None = None,
        ) -> "lint.LintConfig": ...
        def __repr__(self) -> str: ...

    class Linter:
        """YAML linter with configurable rules."""

        def __init__(self, config: "lint.LintConfig | None" = None) -> None: ...
        @staticmethod
        def with_all_rules() -> "lint.Linter": ...
        def lint(
            self, source: str, path: str | os.PathLike[str] | None = None
        ) -> list["lint.Diagnostic"]:
            """Lint YAML source.

            ``path`` is the file the source comes from; rules whose ``ignore`` patterns match
            it are skipped.

            Raises:
                ValueError: If the YAML cannot be parsed at all, the source exceeds
                    ``max_input_bytes`` (default 100 MiB), or the directory of ``path`` does
                    not exist
            """
            ...
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
    def lint(
        source: str,
        config: "lint.LintConfig | None" = None,
        *,
        path: str | os.PathLike[str] | None = None,
    ) -> list["lint.Diagnostic"]:
        """Lint YAML source with optional configuration.

        ``path`` is the file the source comes from; rules whose ``ignore`` patterns match it
        are skipped.

        Raises:
            ValueError: If the YAML cannot be parsed at all, the source exceeds
                ``max_input_bytes`` (default 100 MiB), or the directory of ``path`` does not
                exist
        """
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
            max_input_bytes: int | None = None,
            max_documents: int | None = None,
            auto_tune: bool = True,
            max_depth: int | None = None,
            max_alias_bytes: int | None = None,
            max_scan_ahead: int | None = None,
        ) -> None: ...
        def with_max_depth(self, depth: int | None) -> "parallel.ParallelConfig": ...
        def with_max_alias_bytes(self, bytes: int | None) -> "parallel.ParallelConfig": ...
        def with_max_scan_ahead(self, chars: int | None) -> "parallel.ParallelConfig": ...
        def with_thread_count(self, count: int | None) -> "parallel.ParallelConfig": ...
        def with_max_input_bytes(self, bytes: int | None) -> "parallel.ParallelConfig": ...
        def with_max_documents(self, count: int | None) -> "parallel.ParallelConfig": ...
        def with_min_chunk_size(self, size: int) -> "parallel.ParallelConfig": ...
        def with_max_chunk_size(self, size: int) -> "parallel.ParallelConfig": ...
        def with_auto_tune(self, auto_tune: bool) -> "parallel.ParallelConfig": ...
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
            ValueError: If parsing fails, limits are exceeded (input size, or more than
                ``max_documents`` documents, 100 000 by default even without a config),
                or a document holds a decimal integer beyond the i64 range with more digits
                than ``sys.get_int_max_str_digits()``
                    (CPython's ``int()`` limit)
        """
        ...

    @staticmethod
    def dump_parallel(
        documents: Any,
        config: "parallel.ParallelConfig | None" = None,
        allow_unicode: bool = True,
        sort_keys: bool = False,
        indent: int = 2,
        width: int = 80,
        default_flow_style: bool | None = None,
        explicit_start: bool = False,
    ) -> str:
        """Dump multiple YAML documents in parallel.

        Args:
            documents: List or iterable of Python objects to serialize
            config: Optional parallel processing configuration
            allow_unicode: Allow unicode characters (default: True)
            sort_keys: If True, sort dictionary keys (default: False)
            indent: Number of spaces for indentation (default: 2)
            width: Maximum line width (default: 80)
            default_flow_style: Force flow style for collections (default: None)
            explicit_start: Add explicit document start marker (default: False)

        Returns:
            A YAML string with multiple documents separated by '---'

        Raises:
            TypeError: If any object cannot be serialized
            ValueError: If the document count exceeds ``config.max_documents``
                (100,000 by default), or an int has more digits than
                ``sys.get_int_max_str_digits()`` (CPython's limit)

        Example:
            >>> import fast_yaml
            >>> docs = [{'a': i} for i in range(1000)]
            >>> yaml_str = fast_yaml.parallel.dump_parallel(docs)
        """
        ...

# Batch submodule (PyO3 submodule, not a class - noqa: N801)
class batch:  # noqa: N801
    """Batch file processing submodule."""

    class FileOutcome:
        """Outcome of processing a single file."""

        Success: "batch.FileOutcome"
        Changed: "batch.FileOutcome"
        Unchanged: "batch.FileOutcome"
        Error: "batch.FileOutcome"

        def __repr__(self) -> str: ...
        def __eq__(self, other: object) -> bool: ...
        def __hash__(self) -> int: ...

    class FileResult:
        """Result for a single file with path context."""

        path: str
        outcome: "batch.FileOutcome"
        duration_ms: float
        error: str | None

        def is_success(self) -> bool:
            """Returns True if processing was successful."""
            ...

        def was_changed(self) -> bool:
            """Returns True if file content was changed."""
            ...

        def __repr__(self) -> str: ...

    class BatchResult:
        """Aggregated results from batch processing."""

        total: int
        success: int
        changed: int
        failed: int
        duration_ms: float

        def is_success(self) -> bool:
            """Returns True if all files processed successfully."""
            ...

        def files_per_second(self) -> float:
            """Returns processing throughput."""
            ...

        def errors(self) -> list[tuple[str, str]]:
            """Returns list of (path, error_message) tuples."""
            ...

        def __repr__(self) -> str: ...

    class BatchConfig:
        """Configuration for batch file processing.

        ``max_depth`` applies to ``process_files`` and ``format_files``;
        ``max_scan_ahead`` and ``max_documents`` to both; ``max_alias_bytes`` to
        ``process_files`` only.
        ``indent`` must be in 1..=9 and ``width`` in 20..=1000.
        Non-integer values raise ``TypeError``, out-of-range values ``ValueError``.
        """

        def __init__(
            self,
            workers: int | None = None,
            max_input_bytes: int | None = None,
            sequential_threshold: int = 4096,
            indent: int = 2,
            width: int = 80,
            sort_keys: bool = False,
            max_depth: int | None = None,
            max_alias_bytes: int | None = None,
            max_scan_ahead: int | None = None,
            max_documents: int | None = None,
        ) -> None: ...
        def with_max_depth(self, depth: int | None) -> "batch.BatchConfig": ...
        def with_max_alias_bytes(self, bytes: int | None) -> "batch.BatchConfig": ...
        def with_max_scan_ahead(self, chars: int | None) -> "batch.BatchConfig": ...
        def with_max_documents(self, count: int | None) -> "batch.BatchConfig": ...
        def with_workers(self, workers: int | None) -> "batch.BatchConfig": ...
        def with_indent(self, indent: int) -> "batch.BatchConfig": ...
        def with_width(self, width: int) -> "batch.BatchConfig": ...
        def with_sort_keys(self, sort_keys: bool) -> "batch.BatchConfig": ...
        def __repr__(self) -> str: ...

    @staticmethod
    def process_files(
        paths: list[str],
        config: "batch.BatchConfig | None" = None,
    ) -> "batch.BatchResult":
        """Process files and return batch result.

        Parses and validates YAML files in parallel.

        Args:
            paths: List of file paths to process
            config: Optional batch processing configuration

        Returns:
            BatchResult with processing statistics
        """
        ...

    @staticmethod
    def format_files(
        paths: list[str],
        config: "batch.BatchConfig | None" = None,
    ) -> list[tuple[str, str | None, str | None]]:
        """Format files and return formatted content (dry-run).

        Formats YAML files without writing changes back.

        Args:
            paths: List of file paths to format
            config: Optional batch processing configuration

        Returns:
            List of (path, content, error) tuples
        """
        ...

    @staticmethod
    def format_files_in_place(
        paths: list[str],
        config: "batch.BatchConfig | None" = None,
    ) -> "batch.BatchResult":
        """Format files in place (write changes back).

        Formats YAML files and writes changes atomically.

        Args:
            paths: List of file paths to format
            config: Optional batch processing configuration

        Returns:
            BatchResult with changed/unchanged counts
        """
        ...
