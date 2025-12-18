"""
fast-yaml: A fast YAML parser for Python, powered by Rust.

This module provides a drop-in replacement for PyYAML's safe_* functions,
with significant performance improvements (5-10x faster).

Example:
    >>> import fast_yaml
    >>> data = fast_yaml.safe_load("name: test\\nvalue: 123")
    >>> data
    {'name': 'test', 'value': 123}
    >>> fast_yaml.safe_dump(data)
    'name: test\\nvalue: 123\\n'

For drop-in replacement of PyYAML:
    >>> import fast_yaml as yaml
    >>> yaml.safe_load("key: value")
    {'key': 'value'}
"""

from __future__ import annotations

from collections.abc import Iterator
from typing import IO, Any

from . import lint, parallel

# Loader classes
# Dumper classes
# Exception hierarchy
# Mark class for error locations
from ._core import (
    ComposerError,
    ConstructorError,
    Dumper,
    EmitterError,
    FullLoader,
    Loader,
    Mark,
    MarkedYAMLError,
    ParserError,
    SafeDumper,
    SafeLoader,
    ScannerError,
    YAMLError,
)
from ._core import dump as _dump
from ._core import dump_all as _dump_all
from ._core import load as _load
from ._core import load_all as _load_all
from ._core import safe_dump as _safe_dump
from ._core import safe_dump_all as _safe_dump_all
from ._core import safe_load as _safe_load
from ._core import safe_load_all as _safe_load_all
from ._core import version as _version

__version__ = _version()
__all__ = [
    # Core functions
    "safe_load",
    "safe_load_all",
    "safe_dump",
    "safe_dump_all",
    "load",
    "load_all",
    "dump",
    "dump_all",
    "__version__",
    # Submodules
    "lint",
    "parallel",
    # Loader classes
    "SafeLoader",
    "FullLoader",
    "Loader",
    # Dumper classes
    "SafeDumper",
    "Dumper",
    # Exceptions
    "YAMLError",
    "MarkedYAMLError",
    "ScannerError",
    "ParserError",
    "ComposerError",
    "ConstructorError",
    "EmitterError",
    # Mark
    "Mark",
]


def safe_load(stream: str | bytes | IO[str] | IO[bytes]) -> Any:
    """
    Parse a YAML document and return a Python object.

    This is equivalent to PyYAML's `yaml.safe_load()`.

    Args:
        stream: A YAML document as a string, bytes, or file-like object.

    Returns:
        The parsed YAML document as Python objects (dict, list, str, int, float, bool, None).

    Raises:
        ValueError: If the YAML is invalid.

    Example:
        >>> import fast_yaml
        >>> fast_yaml.safe_load("name: test")
        {'name': 'test'}
        >>> fast_yaml.safe_load("items:\\n  - one\\n  - two")
        {'items': ['one', 'two']}
    """
    if hasattr(stream, "read"):
        # File-like object
        content = stream.read()
        if isinstance(content, bytes):
            content = content.decode("utf-8")
    elif isinstance(stream, bytes):
        content = stream.decode("utf-8")
    else:
        content = stream

    return _safe_load(content)


def safe_load_all(stream: str | bytes | IO[str] | IO[bytes]) -> Iterator[Any]:
    """
    Parse all YAML documents in a stream and return an iterator.

    This is equivalent to PyYAML's `yaml.safe_load_all()`.

    Args:
        stream: A YAML string potentially containing multiple documents.

    Yields:
        Parsed YAML documents.

    Example:
        >>> import fast_yaml
        >>> list(fast_yaml.safe_load_all("---\\nfoo: 1\\n---\\nbar: 2"))
        [{'foo': 1}, {'bar': 2}]
    """
    if hasattr(stream, "read"):
        content = stream.read()
        if isinstance(content, bytes):
            content = content.decode("utf-8")
    elif isinstance(stream, bytes):
        content = stream.decode("utf-8")
    else:
        content = stream

    # _safe_load_all returns a list, convert to iterator
    return iter(_safe_load_all(content))


def safe_dump(
    data: Any,
    stream: IO[str] | None = None,
    *,
    allow_unicode: bool = True,
    sort_keys: bool = False,
    indent: int | None = None,  # TODO: implement
    width: int | None = None,  # TODO: implement
) -> str | None:
    """
    Serialize a Python object to a YAML string.

    This is equivalent to PyYAML's `yaml.safe_dump()`.

    Args:
        data: A Python object to serialize.
        stream: If provided, write to this file-like object and return None.
        allow_unicode: If True, allow unicode characters in output. Default: True.
        sort_keys: If True, sort dictionary keys. Default: False.
        indent: Number of spaces for indentation. Default: 2 (TODO: implement).
        width: Maximum line width. Default: 80 (TODO: implement).

    Returns:
        A YAML string if stream is None, otherwise None.

    Raises:
        TypeError: If the object contains types that cannot be serialized.

    Example:
        >>> import fast_yaml
        >>> fast_yaml.safe_dump({'name': 'test', 'value': 123})
        'name: test\\nvalue: 123\\n'
    """
    result = _safe_dump(
        data,
        allow_unicode=allow_unicode,
        sort_keys=sort_keys,
    )

    if stream is not None:
        stream.write(result)
        return None

    return result


def safe_dump_all(
    documents: Iterator[Any],
    stream: IO[str] | None = None,
    *,
    allow_unicode: bool = True,
    sort_keys: bool = False,
) -> str | None:
    """
    Serialize multiple Python objects to a YAML string with document separators.

    This is equivalent to PyYAML's `yaml.safe_dump_all()`.

    Args:
        documents: An iterable of Python objects to serialize.
        stream: If provided, write to this file-like object and return None.

    Returns:
        A YAML string if stream is None, otherwise None.

    Example:
        >>> import fast_yaml
        >>> fast_yaml.safe_dump_all([{'a': 1}, {'b': 2}])
        '---\\na: 1\\n---\\nb: 2\\n'
    """
    result = _safe_dump_all(list(documents))

    if stream is not None:
        stream.write(result)
        return None

    return result


# PyYAML-compatible load function with optional Loader
def load(
    stream: str | bytes | IO[str] | IO[bytes],
    Loader: type | None = None,  # noqa: N803 - PyYAML API compatibility
) -> Any:
    """
    Parse a YAML document with an optional Loader.

    This is equivalent to PyYAML's `yaml.load()`. For now, all loaders
    behave like SafeLoader (safe by default). The Loader parameter is
    accepted for API compatibility.

    Args:
        stream: A YAML document as a string, bytes, or file-like object.
        Loader: Optional loader class (SafeLoader, FullLoader, Loader).

    Returns:
        The parsed YAML document as Python objects.

    Raises:
        ValueError: If the YAML is invalid.

    Example:
        >>> import fast_yaml
        >>> fast_yaml.load("key: value")
        {'key': 'value'}
        >>> fast_yaml.load("key: value", Loader=fast_yaml.SafeLoader)
        {'key': 'value'}
    """
    if hasattr(stream, "read"):
        content = stream.read()
        if isinstance(content, bytes):
            content = content.decode("utf-8")
    elif isinstance(stream, bytes):
        content = stream.decode("utf-8")
    else:
        content = stream

    # Handle Loader parameter - can be None, a class, or an instance
    if Loader is None:
        loader_instance = None
    elif isinstance(Loader, type):
        # It's a class, create an instance
        loader_instance = Loader()
    else:
        # It's already an instance
        loader_instance = Loader
    return _load(content, loader_instance)


def load_all(
    stream: str | bytes | IO[str] | IO[bytes],
    Loader: type | None = None,  # noqa: N803 - PyYAML API compatibility
) -> Iterator[Any]:
    """
    Parse all YAML documents in a stream with an optional Loader.

    This is equivalent to PyYAML's `yaml.load_all()`. For now, all loaders
    behave like SafeLoader (safe by default). The Loader parameter is
    accepted for API compatibility.

    Args:
        stream: A YAML string potentially containing multiple documents.
        Loader: Optional loader class (SafeLoader, FullLoader, Loader).

    Yields:
        Parsed YAML documents.

    Example:
        >>> import fast_yaml
        >>> list(fast_yaml.load_all("---\\nfoo: 1\\n---\\nbar: 2"))
        [{'foo': 1}, {'bar': 2}]
    """
    if hasattr(stream, "read"):
        content = stream.read()
        if isinstance(content, bytes):
            content = content.decode("utf-8")
    elif isinstance(stream, bytes):
        content = stream.decode("utf-8")
    else:
        content = stream

    # Handle Loader parameter - can be None, a class, or an instance
    if Loader is None:
        loader_instance = None
    elif isinstance(Loader, type):
        # It's a class, create an instance
        loader_instance = Loader()
    else:
        # It's already an instance
        loader_instance = Loader
    return iter(_load_all(content, loader_instance))


# PyYAML-compatible dump function with optional Dumper
def dump(
    data: Any,
    stream: IO[str] | None = None,
    Dumper: type | None = None,  # noqa: N803 - PyYAML API compatibility
    *,
    allow_unicode: bool = True,
    sort_keys: bool = False,
    indent: int | None = None,
    width: int | None = None,
    explicit_start: bool = False,
) -> str | None:
    """
    Serialize a Python object to a YAML string with an optional Dumper.

    This is equivalent to PyYAML's `yaml.dump()`. For now, all dumpers
    behave like SafeDumper (safe by default). The Dumper parameter is
    accepted for API compatibility.

    Args:
        data: A Python object to serialize.
        stream: If provided, write to this file-like object and return None.
        Dumper: Optional dumper class (SafeDumper, Dumper).
        allow_unicode: If True, allow unicode characters in output. Default: True.
        sort_keys: If True, sort dictionary keys. Default: False.
        indent: Number of spaces for indentation. Default: 2.
        width: Maximum line width. Default: 80.
        explicit_start: If True, add explicit document start marker (---).

    Returns:
        A YAML string if stream is None, otherwise None.

    Raises:
        TypeError: If the object contains types that cannot be serialized.

    Example:
        >>> import fast_yaml
        >>> fast_yaml.dump({'name': 'test'})
        'name: test\\n'
        >>> fast_yaml.dump({'name': 'test'}, Dumper=fast_yaml.SafeDumper)
        'name: test\\n'
    """
    # Handle Dumper parameter - can be None, a class, or an instance
    if Dumper is None:
        dumper_instance = None
    elif isinstance(Dumper, type):
        dumper_instance = Dumper()
    else:
        dumper_instance = Dumper

    # Call the underlying _dump function with explicit parameters
    result: str = _dump(
        data,
        dumper_instance,
        allow_unicode=allow_unicode,
        sort_keys=sort_keys,
        indent=indent if indent is not None else 2,
        width=width if width is not None else 80,
        explicit_start=explicit_start,
    )

    if stream is not None:
        stream.write(result)
        return None

    return result


def dump_all(
    documents: Iterator[Any],
    stream: IO[str] | None = None,
    Dumper: type | None = None,  # noqa: N803 - PyYAML API compatibility
    *,
    allow_unicode: bool = True,
    sort_keys: bool = False,
    indent: int | None = None,
    width: int | None = None,
    explicit_start: bool = False,
) -> str | None:
    """
    Serialize multiple Python objects to a YAML string with document separators.

    This is equivalent to PyYAML's `yaml.dump_all()`. For now, all dumpers
    behave like SafeDumper (safe by default). The Dumper parameter is
    accepted for API compatibility.

    Args:
        documents: An iterable of Python objects to serialize.
        stream: If provided, write to this file-like object and return None.
        Dumper: Optional dumper class (SafeDumper, Dumper).
        allow_unicode: If True, allow unicode characters in output. Default: True.
        sort_keys: If True, sort dictionary keys. Default: False.
        indent: Number of spaces for indentation. Default: 2.
        width: Maximum line width. Default: 80.
        explicit_start: If True, add explicit document start markers (---).

    Returns:
        A YAML string if stream is None, otherwise None.

    Example:
        >>> import fast_yaml
        >>> fast_yaml.dump_all([{'a': 1}, {'b': 2}])
        '---\\na: 1\\n---\\nb: 2\\n'
    """
    # Handle Dumper parameter - can be None, a class, or an instance
    if Dumper is None:
        dumper_instance = None
    elif isinstance(Dumper, type):
        dumper_instance = Dumper()
    else:
        dumper_instance = Dumper

    # Call the underlying _dump_all function with explicit parameters
    result: str = _dump_all(
        list(documents),
        dumper_instance,
        allow_unicode=allow_unicode,
        sort_keys=sort_keys,
        indent=indent if indent is not None else 2,
        width=width if width is not None else 80,
        explicit_start=explicit_start,
    )

    if stream is not None:
        stream.write(result)
        return None

    return result
