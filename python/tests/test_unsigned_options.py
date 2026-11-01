"""Unsigned numeric options raise ValueError, not OverflowError, when out of range."""

from __future__ import annotations

import io

import pytest

import fast_yaml
from fast_yaml._core import batch, parallel

DOC = {"a": 1}

CALLS = [
    ("workers", lambda v: batch.BatchConfig(workers=v), 0, 128),
    ("workers", lambda v: batch.BatchConfig().with_workers(v), 0, 128),
    ("thread_count", lambda v: parallel.ParallelConfig(thread_count=v), 0, 128),
    ("thread_count", lambda v: parallel.ParallelConfig().with_thread_count(v), 0, 128),
    (
        "sequential_threshold",
        lambda v: batch.BatchConfig(sequential_threshold=v),
        0,
        4294967295,
    ),
    ("min_chunk_size", lambda v: parallel.ParallelConfig(min_chunk_size=v), 1, 4294967295),
    ("min_chunk_size", lambda v: parallel.ParallelConfig().with_min_chunk_size(v), 1, 4294967295),
    ("indent", lambda v: batch.BatchConfig(indent=v), 1, 9),
    ("indent", lambda v: batch.BatchConfig().with_indent(v), 1, 9),
    ("indent", lambda v: fast_yaml.safe_dump(DOC, indent=v), 1, 9),
    ("indent", lambda v: fast_yaml.safe_dump_all([DOC], indent=v), 1, 9),
    ("indent", lambda v: fast_yaml.safe_dump_to(DOC, io.StringIO(), indent=v), 1, 9),
    ("indent", lambda v: parallel.dump_parallel([DOC], indent=v), 1, 9),
    (
        "chunk_size",
        lambda v: fast_yaml.safe_dump_to(DOC, io.StringIO(), chunk_size=v),
        1024,
        1048576,
    ),
]


@pytest.mark.parametrize(("option", "call", "low", "high"), CALLS)
def test_negative_and_out_of_range_values_raise_value_error(option, call, low, high):
    for bad in (-1, high + 1, -(2**80), 2**80):
        message = f"{option} must be between {low} and {high}, got {bad}"
        with pytest.raises(ValueError, match=message):
            call(bad)
    if low > 0:
        with pytest.raises(ValueError, match=f"{option} must be between {low} and {high}, got 0"):
            call(0)


@pytest.mark.parametrize(("call", "low", "high"), [c[1:] for c in CALLS])
def test_bounds_are_accepted_and_bool_is_a_type_error(call, low, high):
    call(low)
    call(high)
    with pytest.raises(TypeError):
        call(True)
    with pytest.raises(TypeError):
        call("4")


def test_safe_dump_to_rejects_chunk_size_instead_of_clamping():
    with pytest.raises(ValueError, match="chunk_size must be between 1024 and 1048576, got 10"):
        fast_yaml.safe_dump_to(DOC, io.StringIO(), chunk_size=10)
    stream = io.StringIO()
    fast_yaml.safe_dump_to(DOC, stream, chunk_size=1024)
    assert stream.getvalue() == "a: 1\n"
