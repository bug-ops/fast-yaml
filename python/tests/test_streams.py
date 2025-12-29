"""Tests for stream/file-like object handling and bytes input."""

from __future__ import annotations

import io

import fast_yaml


class TestSafeLoadStreams:
    """Tests for safe_load with different input types."""

    def test_load_from_string(self):
        """Test loading from string."""
        result = fast_yaml.safe_load("key: value")
        assert result == {"key": "value"}

    def test_load_from_bytes(self):
        """Test loading from bytes."""
        result = fast_yaml.safe_load(b"key: value")
        assert result == {"key": "value"}

    def test_load_from_bytes_unicode(self):
        """Test loading from bytes with unicode content."""
        result = fast_yaml.safe_load("key: \u4e2d\u6587".encode())
        assert result == {"key": "\u4e2d\u6587"}

    def test_load_from_string_io(self):
        """Test loading from StringIO."""
        stream = io.StringIO("key: value")
        result = fast_yaml.safe_load(stream)
        assert result == {"key": "value"}

    def test_load_from_bytes_io(self):
        """Test loading from BytesIO."""
        stream = io.BytesIO(b"key: value")
        result = fast_yaml.safe_load(stream)
        assert result == {"key": "value"}

    def test_load_from_bytes_io_unicode(self):
        """Test loading from BytesIO with unicode."""
        stream = io.BytesIO("key: \u4e2d\u6587".encode())
        result = fast_yaml.safe_load(stream)
        assert result == {"key": "\u4e2d\u6587"}


class TestSafeLoadAllStreams:
    """Tests for safe_load_all with different input types."""

    def test_load_all_from_string(self):
        """Test loading all from string."""
        result = list(fast_yaml.safe_load_all("---\na: 1\n---\nb: 2"))
        assert result == [{"a": 1}, {"b": 2}]

    def test_load_all_from_bytes(self):
        """Test loading all from bytes."""
        result = list(fast_yaml.safe_load_all(b"---\na: 1\n---\nb: 2"))
        assert result == [{"a": 1}, {"b": 2}]

    def test_load_all_from_string_io(self):
        """Test loading all from StringIO."""
        stream = io.StringIO("---\na: 1\n---\nb: 2")
        result = list(fast_yaml.safe_load_all(stream))
        assert result == [{"a": 1}, {"b": 2}]

    def test_load_all_from_bytes_io(self):
        """Test loading all from BytesIO."""
        stream = io.BytesIO(b"---\na: 1\n---\nb: 2")
        result = list(fast_yaml.safe_load_all(stream))
        assert result == [{"a": 1}, {"b": 2}]


class TestLoadStreams:
    """Tests for load with different input types."""

    def test_load_from_bytes(self):
        """Test load from bytes."""
        result = fast_yaml.load(b"key: value")
        assert result == {"key": "value"}

    def test_load_from_string_io(self):
        """Test load from StringIO."""
        stream = io.StringIO("key: value")
        result = fast_yaml.load(stream)
        assert result == {"key": "value"}

    def test_load_from_bytes_io(self):
        """Test load from BytesIO."""
        stream = io.BytesIO(b"key: value")
        result = fast_yaml.load(stream)
        assert result == {"key": "value"}

    def test_load_with_loader_instance(self):
        """Test load with Loader instance instead of class."""
        loader_instance = fast_yaml.SafeLoader()
        result = fast_yaml.load("key: value", Loader=loader_instance)
        assert result == {"key": "value"}


class TestLoadAllStreams:
    """Tests for load_all with different input types."""

    def test_load_all_from_bytes(self):
        """Test load_all from bytes."""
        result = list(fast_yaml.load_all(b"---\na: 1\n---\nb: 2"))
        assert result == [{"a": 1}, {"b": 2}]

    def test_load_all_from_string_io(self):
        """Test load_all from StringIO."""
        stream = io.StringIO("---\na: 1\n---\nb: 2")
        result = list(fast_yaml.load_all(stream))
        assert result == [{"a": 1}, {"b": 2}]

    def test_load_all_from_bytes_io(self):
        """Test load_all from BytesIO."""
        stream = io.BytesIO(b"---\na: 1\n---\nb: 2")
        result = list(fast_yaml.load_all(stream))
        assert result == [{"a": 1}, {"b": 2}]

    def test_load_all_with_loader_instance(self):
        """Test load_all with Loader instance instead of class."""
        loader_instance = fast_yaml.FullLoader()
        result = list(fast_yaml.load_all("---\na: 1\n---\nb: 2", Loader=loader_instance))
        assert result == [{"a": 1}, {"b": 2}]


class TestSafeDumpStreams:
    """Tests for safe_dump with stream output."""

    def test_dump_to_string(self):
        """Test dump returns string when no stream."""
        result = fast_yaml.safe_dump({"key": "value"})
        assert isinstance(result, str)
        assert "key: value" in result

    def test_dump_to_stream(self):
        """Test dump to StringIO stream."""
        stream = io.StringIO()
        result = fast_yaml.safe_dump({"key": "value"}, stream=stream)
        assert result is None
        assert "key: value" in stream.getvalue()

    def test_dump_to_stream_complex(self):
        """Test dump complex data to stream."""
        stream = io.StringIO()
        data = {"name": "test", "items": [1, 2, 3], "nested": {"a": 1}}
        result = fast_yaml.safe_dump(data, stream=stream)
        assert result is None
        content = stream.getvalue()
        assert "name: test" in content
        assert "items:" in content


class TestSafeDumpAllStreams:
    """Tests for safe_dump_all with stream output."""

    def test_dump_all_to_string(self):
        """Test dump_all returns string when no stream."""
        result = fast_yaml.safe_dump_all([{"a": 1}, {"b": 2}])
        assert isinstance(result, str)
        assert "a: 1" in result

    def test_dump_all_to_stream(self):
        """Test dump_all to StringIO stream."""
        stream = io.StringIO()
        result = fast_yaml.safe_dump_all([{"a": 1}, {"b": 2}], stream=stream)
        assert result is None
        content = stream.getvalue()
        assert "a: 1" in content
        assert "b: 2" in content


class TestDumpStreams:
    """Tests for dump with stream output."""

    def test_dump_to_stream(self):
        """Test dump to StringIO stream."""
        stream = io.StringIO()
        result = fast_yaml.dump({"key": "value"}, stream=stream)
        assert result is None
        assert "key: value" in stream.getvalue()

    def test_dump_with_dumper_instance(self):
        """Test dump with Dumper instance instead of class."""
        dumper_instance = fast_yaml.SafeDumper()
        result = fast_yaml.dump({"key": "value"}, Dumper=dumper_instance)
        assert isinstance(result, str)
        assert "key: value" in result


class TestDumpAllStreams:
    """Tests for dump_all with stream output."""

    def test_dump_all_to_stream(self):
        """Test dump_all to StringIO stream."""
        stream = io.StringIO()
        result = fast_yaml.dump_all([{"a": 1}, {"b": 2}], stream=stream)
        assert result is None
        content = stream.getvalue()
        assert "a: 1" in content
        assert "b: 2" in content

    def test_dump_all_with_dumper_instance(self):
        """Test dump_all with Dumper instance instead of class."""
        dumper_instance = fast_yaml.Dumper()
        result = fast_yaml.dump_all([{"a": 1}, {"b": 2}], Dumper=dumper_instance)
        assert isinstance(result, str)
        assert "a: 1" in result
