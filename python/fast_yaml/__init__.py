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
from ._core import safe_dump as _safe_dump
from ._core import safe_dump_all as _safe_dump_all
from ._core import safe_load as _safe_load
from ._core import safe_load_all as _safe_load_all
from ._core import version as _version

__version__ = _version()
__all__ = [
    "safe_load",
    "safe_load_all",
    "safe_dump",
    "safe_dump_all",
    "load",
    "dump",
    "__version__",
    "lint",
    "parallel",
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


# Aliases for PyYAML compatibility
# Note: These are aliases for safe_* functions only
# full load/dump with arbitrary Python objects is not supported
load = safe_load
dump = safe_dump
