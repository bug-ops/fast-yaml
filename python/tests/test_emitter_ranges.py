"""Indent and formatter depth options are validated, never clamped."""

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
    assert fast_yaml.safe_dump({"a": {"b": 1}}, indent=1) == "a:\n b: 1\n"
    assert fast_yaml.safe_dump({"a": {"b": 1}}, indent=9) == "a:\n         b: 1\n"


def test_batch_config_validates_indent():
    with pytest.raises(ValueError, match="indent must be between 1 and 9, got 0"):
        batch.BatchConfig(indent=0)
    config = batch.BatchConfig()
    with pytest.raises(ValueError, match="indent must be between 1 and 9, got 12"):
        config.with_indent(12)
    assert config.with_indent(9) is not None


@pytest.mark.parametrize("option", ["width", "allow_unicode"])
def test_removed_dump_options_are_rejected(option):
    value = 80 if option == "width" else True
    for dump in (fast_yaml.safe_dump, fast_yaml.dump, fast_yaml.safe_dump_all):
        data = [{"a": 1}] if dump is fast_yaml.safe_dump_all else {"a": 1}
        with pytest.raises(TypeError):
            dump(data, **{option: value})
    with pytest.raises(TypeError):
        parallel.dump_parallel([{"a": 1}], **{option: value})
    with pytest.raises(TypeError):
        batch.BatchConfig(**{option: value})


def test_sort_keys_orders_numbers_by_value_and_sets_alike():
    assert fast_yaml.safe_dump({10: "a", 9: "b", "x": "c"}, sort_keys=True) == (
        "9: b\n10: a\nx: c\n"
    )
    assert fast_yaml.safe_dump([{"b": 1, "a": 2}], sort_keys=True) == "- a: 2\n  b: 1\n"
    assert parallel.dump_parallel([{10: 1, 9: 2}], sort_keys=True) == "9: 2\n10: 1\n"


def test_format_files_honors_max_depth():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "nested.yaml"
        path.write_text("[[[1]]]\n")
        [low] = batch.format_files([str(path)], batch.BatchConfig(max_depth=2))
        assert isinstance(low, batch.FormatResult.Failed)
        assert low.error.kind == batch.FileErrorKind.Limit
        assert "nesting depth exceeds 2" in low.error.message

        [ok] = batch.format_files([str(path)], batch.BatchConfig(max_depth=3))
        assert isinstance(ok, batch.FormatResult.Formatted)
        assert "1" in ok.content
