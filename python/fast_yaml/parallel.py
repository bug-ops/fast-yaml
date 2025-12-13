"""
Parallel YAML parsing for large multi-document files.

This module provides multi-threaded YAML parsing that can significantly
speed up processing of large files containing multiple YAML documents.

Example:
    >>> import fast_yaml.parallel as yaml_parallel
    >>> yaml = "---\\nfoo: 1\\n---\\nbar: 2\\n---\\nbaz: 3"
    >>> docs = yaml_parallel.parse_parallel(yaml)
    >>> len(docs)
    3
"""

from __future__ import annotations

from typing import Any

from ._core import parallel as _parallel_module

# Re-export from the submodule
ParallelConfig = _parallel_module.ParallelConfig
_parse_parallel = _parallel_module.parse_parallel

__all__ = [
    "ParallelConfig",
    "parse_parallel",
]


def parse_parallel(
    source: str,
    config: ParallelConfig | None = None,
) -> list[Any]:
    """
    Parse multi-document YAML in parallel.

    Automatically splits YAML documents at '---' boundaries and
    processes them in parallel using Rayon thread pool.

    Args:
        source: YAML source potentially containing multiple documents
        config: Optional parallel processing configuration

    Returns:
        List of parsed YAML documents

    Raises:
        ValueError: If parsing fails or limits exceeded

    Performance:
        - Single document: Falls back to sequential parsing
        - Multi-document: 3-6x faster on 4-8 core systems
        - Use for files > 1MB with multiple documents

    Example:
        >>> import fast_yaml.parallel as yaml_parallel
        >>> yaml = "---\\nfoo: 1\\n---\\nbar: 2\\n---\\nbaz: 3"
        >>> docs = yaml_parallel.parse_parallel(yaml)
        >>> len(docs)
        3

        Custom configuration:

        >>> config = yaml_parallel.ParallelConfig(
        ...     thread_count=8,
        ...     max_input_size=200 * 1024 * 1024,  # 200MB
        ... )
        >>> docs = yaml_parallel.parse_parallel(yaml, config=config)
    """
    return _parse_parallel(source, config)
