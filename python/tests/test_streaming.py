"""Tests for the streaming YAML dump functionality."""

from __future__ import annotations

import io
import os
import tempfile
from pathlib import Path

import pytest

import fast_yaml


class TestSafeDumpTo:
    """Tests for safe_dump_to function."""

    def test_dump_to_file_object(self):
        """Test dumping to a real file object."""
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".yaml") as f:
            temp_path = f.name
            data = {"key": "value", "number": 42}
            bytes_written = fast_yaml.safe_dump_to(data, f)
            assert bytes_written > 0

        try:
            with open(temp_path) as f:
                content = f.read()
            assert "key: value" in content
            assert "number: 42" in content
        finally:
            os.unlink(temp_path)

    def test_dump_to_stringio(self):
        """Test dumping to StringIO."""
        stream = io.StringIO()
        data = {"hello": "world"}
        bytes_written = fast_yaml.safe_dump_to(data, stream)
        assert bytes_written > 0
        content = stream.getvalue()
        assert "hello: world" in content

    def test_dump_to_bytesio(self):
        """Test dumping to BytesIO fails (expects string stream)."""
        stream = io.BytesIO()
        data = {"test": "data"}
        with pytest.raises(TypeError):
            fast_yaml.safe_dump_to(data, stream)

    def test_dump_to_with_sort_keys(self):
        """Test dumping with sorted keys."""
        stream = io.StringIO()
        data = {"z": 1, "a": 2, "m": 3}
        fast_yaml.safe_dump_to(data, stream, sort_keys=True)
        content = stream.getvalue()
        lines = [line.strip() for line in content.split("\n") if line.strip()]
        assert lines[0].startswith("a:")
        assert lines[2].startswith("z:")

    def test_dump_to_with_indent(self):
        """Test dumping with custom indent."""
        stream = io.StringIO()
        data = {"nested": {"key": "value"}}
        fast_yaml.safe_dump_to(data, stream, indent=4)
        content = stream.getvalue()
        assert "nested:" in content
        assert "key: value" in content

    def test_dump_to_with_width(self):
        """Test dumping with custom width."""
        stream = io.StringIO()
        data = {"key": "value"}
        fast_yaml.safe_dump_to(data, stream, width=40)
        assert stream.getvalue()

    def test_dump_to_with_explicit_start(self):
        """Test dumping with explicit document start."""
        stream = io.StringIO()
        data = {"key": "value"}
        fast_yaml.safe_dump_to(data, stream, explicit_start=True)
        content = stream.getvalue()
        assert content.startswith("---")

    def test_dump_to_small_document(self):
        """Test dumping small document (single write)."""
        stream = io.StringIO()
        data = {"small": "doc"}
        bytes_written = fast_yaml.safe_dump_to(data, stream)
        assert bytes_written > 0
        content = stream.getvalue()
        assert "small: doc" in content

    def test_dump_to_large_document(self):
        """Test dumping large document (chunked writes)."""
        stream = io.StringIO()
        data = {"items": list(range(1000)), "nested": {"key": "value"}}
        bytes_written = fast_yaml.safe_dump_to(data, stream)
        assert bytes_written > 0
        content = stream.getvalue()
        assert "items:" in content

    def test_dump_to_with_custom_chunk_size(self):
        """Test dumping with custom chunk size."""
        stream = io.StringIO()
        data = {"data": list(range(100))}
        bytes_written = fast_yaml.safe_dump_to(data, stream, chunk_size=1024)
        assert bytes_written > 0

    def test_dump_to_with_very_small_chunk_size(self):
        """Test dumping with very small chunk size (clamped to 1KB min)."""
        stream = io.StringIO()
        data = {"key": "value"}
        bytes_written = fast_yaml.safe_dump_to(data, stream, chunk_size=100)
        assert bytes_written > 0

    def test_dump_to_with_very_large_chunk_size(self):
        """Test dumping with very large chunk size (clamped to 1MB max)."""
        stream = io.StringIO()
        data = {"key": "value"}
        bytes_written = fast_yaml.safe_dump_to(data, stream, chunk_size=10 * 1024 * 1024)
        assert bytes_written > 0

    def test_dump_to_complex_nested_structure(self):
        """Test dumping complex nested structure."""
        stream = io.StringIO()
        data = {
            "level1": {
                "level2": {
                    "level3": {"level4": {"level5": {"value": "deep"}}},
                    "list": ["item1", "item2", "item3"],
                },
                "another_key": "value",
            }
        }
        bytes_written = fast_yaml.safe_dump_to(data, stream)
        assert bytes_written > 0
        content = stream.getvalue()
        assert "level5:" in content
        assert "value: deep" in content

    def test_dump_to_various_types(self):
        """Test dumping various YAML types."""
        stream = io.StringIO()
        data = {
            "string": "hello",
            "number": 42,
            "float": 3.14,
            "boolean": True,
            "null_value": None,
            "list": [1, 2, 3],
            "dict": {"nested": "value"},
        }
        bytes_written = fast_yaml.safe_dump_to(data, stream)
        assert bytes_written > 0
        content = stream.getvalue()
        assert "string: hello" in content
        assert "boolean: true" in content

    def test_dump_to_unicode_content(self):
        """Test dumping unicode content."""
        stream = io.StringIO()
        data = {
            "chinese": "\u4e2d\u6587",
            "emoji": "\U0001f600",
            "russian": "\u041f\u0440\u0438\u0432\u0435\u0442",
        }
        bytes_written = fast_yaml.safe_dump_to(data, stream)
        assert bytes_written > 0
        content = stream.getvalue()
        assert "\u4e2d\u6587" in content or "chinese:" in content

    def test_dump_to_list_root(self):
        """Test dumping with list as root."""
        stream = io.StringIO()
        data = ["item1", "item2", "item3"]
        bytes_written = fast_yaml.safe_dump_to(data, stream)
        assert bytes_written > 0
        content = stream.getvalue()
        assert "item1" in content

    def test_dump_to_preserves_type_information(self):
        """Test that dumping preserves type information."""
        stream = io.StringIO()
        data = {"int": 42, "float": 3.14, "bool": True, "null": None}
        fast_yaml.safe_dump_to(data, stream)
        content = stream.getvalue()
        reparsed = fast_yaml.safe_load(content)
        assert reparsed["int"] == 42
        assert reparsed["float"] == 3.14
        assert reparsed["bool"] is True
        assert reparsed["null"] is None

    def test_dump_to_round_trip(self):
        """Test round-trip: dump_to then load."""
        stream = io.StringIO()
        original_data = {
            "name": "test",
            "items": [1, 2, 3],
            "nested": {"key": "value"},
        }
        fast_yaml.safe_dump_to(original_data, stream)
        content = stream.getvalue()
        loaded_data = fast_yaml.safe_load(content)
        assert loaded_data == original_data

    def test_dump_to_multiple_calls_same_stream(self):
        """Test multiple dump_to calls to the same stream."""
        stream = io.StringIO()
        fast_yaml.safe_dump_to({"doc": 1}, stream)
        fast_yaml.safe_dump_to({"doc": 2}, stream)
        content = stream.getvalue()
        assert "doc: 1" in content
        assert "doc: 2" in content


class TestSafeDumpToErrors:
    """Tests for error handling in safe_dump_to."""

    def test_dump_to_invalid_stream_no_write_method(self):
        """Test dumping to object without write method."""

        class BadStream:
            pass

        data = {"key": "value"}
        with pytest.raises((TypeError, AttributeError)):
            fast_yaml.safe_dump_to(data, BadStream())

    def test_dump_to_none_stream(self):
        """Test dumping to None stream."""
        data = {"key": "value"}
        with pytest.raises((TypeError, AttributeError)):
            fast_yaml.safe_dump_to(data, None)

    def test_dump_to_readonly_file(self):
        """Test dumping to read-only file object."""
        with tempfile.NamedTemporaryFile(mode="r", delete=False, suffix=".yaml") as f:
            temp_path = f.name

        try:
            with open(temp_path) as f:
                data = {"key": "value"}
                with pytest.raises((TypeError, io.UnsupportedOperation)):
                    fast_yaml.safe_dump_to(data, f)
        finally:
            os.unlink(temp_path)

    def test_dump_to_closed_stream(self):
        """Test dumping to closed stream."""
        stream = io.StringIO()
        stream.close()
        data = {"key": "value"}
        with pytest.raises((ValueError, io.UnsupportedOperation)):
            fast_yaml.safe_dump_to(data, stream)


class TestSafeDumpToPerformance:
    """Performance-related tests for safe_dump_to."""

    def test_dump_to_very_large_document(self):
        """Test dumping very large document."""
        stream = io.StringIO()
        data = {
            "large_list": list(range(10000)),
            "large_dict": {f"key_{i}": f"value_{i}" for i in range(1000)},
        }
        bytes_written = fast_yaml.safe_dump_to(data, stream)
        assert bytes_written > 0

    def test_dump_to_deeply_nested(self):
        """Test dumping deeply nested structure."""
        stream = io.StringIO()
        data = {"level": "top"}
        current = data
        for i in range(50):
            current["nested"] = {f"level_{i}": f"value_{i}"}
            current = current["nested"]
        bytes_written = fast_yaml.safe_dump_to(data, stream)
        assert bytes_written > 0

    def test_dump_to_many_small_writes(self):
        """Test that chunked writes work correctly."""
        stream = io.StringIO()
        data = {"items": [{"id": i, "data": f"test_{i}" * 100} for i in range(100)]}
        bytes_written = fast_yaml.safe_dump_to(data, stream, chunk_size=2048)
        assert bytes_written > 0


class TestSafeDumpToEdgeCases:
    """Edge case tests for safe_dump_to."""

    def test_dump_to_empty_dict(self):
        """Test dumping empty dict."""
        stream = io.StringIO()
        bytes_written = fast_yaml.safe_dump_to({}, stream)
        assert bytes_written > 0
        content = stream.getvalue()
        assert content.strip() == "{}"

    def test_dump_to_empty_list(self):
        """Test dumping empty list."""
        stream = io.StringIO()
        bytes_written = fast_yaml.safe_dump_to([], stream)
        assert bytes_written > 0
        content = stream.getvalue()
        assert content.strip() == "[]"

    def test_dump_to_single_value(self):
        """Test dumping single value."""
        stream = io.StringIO()
        bytes_written = fast_yaml.safe_dump_to("hello", stream)
        assert bytes_written > 0
        content = stream.getvalue()
        assert "hello" in content

    def test_dump_to_special_float_values(self):
        """Test dumping special float values."""
        stream = io.StringIO()
        import math

        data = {
            "infinity": math.inf,
            "neg_infinity": -math.inf,
            "not_a_number": math.nan,
        }
        bytes_written = fast_yaml.safe_dump_to(data, stream)
        assert bytes_written > 0

    def test_dump_to_with_pathlib_path(self):
        """Test dumping to file opened via pathlib."""
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "test.yaml"
            with path.open("w") as f:
                data = {"key": "value"}
                bytes_written = fast_yaml.safe_dump_to(data, f)
                assert bytes_written > 0
            with path.open() as f:
                content = f.read()
            assert "key: value" in content


class TestSafeDumpToMemoryBehavior:
    """Tests to verify memory efficiency of safe_dump_to."""

    def test_dump_to_returns_bytes_written(self):
        """Test that bytes_written is returned correctly."""
        stream = io.StringIO()
        data = {"key": "value", "number": 42}
        bytes_written = fast_yaml.safe_dump_to(data, stream)
        content = stream.getvalue()
        assert bytes_written == len(content)

    def test_dump_to_chunked_bytes_count(self):
        """Test bytes_written is correct for chunked writes."""
        stream = io.StringIO()
        data = {"items": list(range(500))}
        bytes_written = fast_yaml.safe_dump_to(data, stream, chunk_size=1024)
        content = stream.getvalue()
        assert bytes_written == len(content)

    def test_dump_to_large_document_memory_efficient(self):
        """Test that large documents use chunked writes."""
        stream = io.StringIO()
        data = {f"key_{i}": f"value_{i}" * 100 for i in range(1000)}
        bytes_written = fast_yaml.safe_dump_to(data, stream, chunk_size=4096)
        assert bytes_written > 10000
