"""Indent, width and formatter depth options are validated, never clamped."""

from __future__ import annotations

import tempfile
from pathlib import Path

import pytest

import fast_yaml
from fast_yaml._core import batch, parallel


@pytest.mark.parametrize(
    ("option", "value", "message"),
    [
        ("indent", 0, "indent must be between 1 and 9, got 0"),
        ("indent", 10, "indent must be between 1 and 9, got 10"),
        ("width", 19, "width must be between 20 and 1000, got 19"),
        ("width", 1001, "width must be between 20 and 1000, got 1001"),
    ],
)
def test_dump_rejects_out_of_range_options(option, value, message):
    for dump in (fast_yaml.safe_dump, fast_yaml.dump):
        with pytest.raises(ValueError, match=message):
            dump({"a": 1}, **{option: value})
    with pytest.raises(ValueError, match=message):
        fast_yaml.safe_dump_all([{"a": 1}], **{option: value})
    with pytest.raises(ValueError, match=message):
        parallel.dump_parallel([{"a": 1}], **{option: value})


def test_dump_accepts_the_range_limits():
    assert fast_yaml.safe_dump({"a": {"b": 1}}, indent=1, width=20) == "a:\n b: 1\n"
    assert fast_yaml.safe_dump({"a": {"b": 1}}, indent=9, width=1000) == "a:\n         b: 1\n"


def test_batch_config_validates_indent_and_width():
    with pytest.raises(ValueError, match="indent must be between 1 and 9, got 0"):
        batch.BatchConfig(indent=0)
    with pytest.raises(ValueError, match="width must be between 20 and 1000, got 5"):
        batch.BatchConfig(width=5)
    config = batch.BatchConfig()
    with pytest.raises(ValueError, match="indent must be between 1 and 9, got 12"):
        config.with_indent(12)
    with pytest.raises(ValueError, match="width must be between 20 and 1000, got 2000"):
        config.with_width(2000)
    assert config.with_indent(9).with_width(20) is not None


def test_format_files_honors_max_depth():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "nested.yaml"
        path.write_text("[[[1]]]\n")
        [(_, content, error)] = batch.format_files([str(path)], batch.BatchConfig(max_depth=2))
        assert content is None
        assert "nesting depth exceeds 2" in error

        [(_, content, error)] = batch.format_files([str(path)], batch.BatchConfig(max_depth=3))
        assert error is None
        assert "1" in content
