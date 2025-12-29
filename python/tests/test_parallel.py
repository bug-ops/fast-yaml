"""Tests for the parallel YAML processing module."""

from __future__ import annotations

import pytest

from fast_yaml._core import parallel


class TestParallelConfig:
    """Tests for ParallelConfig class."""

    def test_default_config(self):
        """Test default configuration."""
        config = parallel.ParallelConfig()
        assert config is not None

    def test_custom_thread_count(self):
        """Test custom thread count."""
        config = parallel.ParallelConfig(thread_count=4)
        assert config is not None

    def test_auto_thread_count(self):
        """Test auto thread count (None)."""
        config = parallel.ParallelConfig(thread_count=None)
        assert config is not None

    def test_custom_chunk_sizes(self):
        """Test custom chunk sizes."""
        config = parallel.ParallelConfig(
            min_chunk_size=1024,
            max_chunk_size=5 * 1024 * 1024,
        )
        assert config is not None

    def test_custom_limits(self):
        """Test custom input limits."""
        config = parallel.ParallelConfig(
            max_input_size=50 * 1024 * 1024,
            max_documents=50_000,
        )
        assert config is not None

    def test_with_thread_count(self):
        """Test with_thread_count builder method."""
        config = parallel.ParallelConfig().with_thread_count(8)
        assert config is not None

    def test_with_max_input_size(self):
        """Test with_max_input_size builder method."""
        config = parallel.ParallelConfig().with_max_input_size(50 * 1024 * 1024)
        assert config is not None

    def test_with_max_documents(self):
        """Test with_max_documents builder method."""
        config = parallel.ParallelConfig().with_max_documents(1000)
        assert config is not None

    def test_with_min_chunk_size(self):
        """Test with_min_chunk_size builder method."""
        config = parallel.ParallelConfig().with_min_chunk_size(2048)
        assert config is not None

    def test_with_max_chunk_size(self):
        """Test with_max_chunk_size builder method."""
        config = parallel.ParallelConfig().with_max_chunk_size(1024 * 1024)
        assert config is not None

    def test_config_repr(self):
        """Test config repr."""
        config = parallel.ParallelConfig()
        assert repr(config) is not None

    def test_builder_chaining(self):
        """Test chaining builder methods."""
        config = (
            parallel.ParallelConfig()
            .with_thread_count(4)
            .with_max_input_size(10 * 1024 * 1024)
            .with_max_documents(100)
        )
        assert config is not None


class TestParseParallel:
    """Tests for parse_parallel function."""

    def test_parse_single_document(self):
        """Test parsing a single document."""
        result = parallel.parse_parallel("key: value\n")
        assert isinstance(result, list)
        assert len(result) == 1
        assert result[0] == {"key": "value"}

    def test_parse_multiple_documents(self):
        """Test parsing multiple documents."""
        yaml_content = """---
doc: 1
---
doc: 2
---
doc: 3
"""
        result = parallel.parse_parallel(yaml_content)
        assert isinstance(result, list)
        assert len(result) == 3
        assert result[0] == {"doc": 1}
        assert result[1] == {"doc": 2}
        assert result[2] == {"doc": 3}

    def test_parse_with_config(self):
        """Test parsing with custom config."""
        config = parallel.ParallelConfig(thread_count=2)
        yaml_content = "---\na: 1\n---\nb: 2\n"
        result = parallel.parse_parallel(yaml_content, config)
        assert len(result) == 2

    def test_parse_empty_input(self):
        """Test parsing empty input."""
        result = parallel.parse_parallel("")
        assert isinstance(result, list)
        assert len(result) == 0

    def test_parse_whitespace_only(self):
        """Test parsing whitespace-only input."""
        result = parallel.parse_parallel("   \n\n   ")
        assert isinstance(result, list)

    def test_parse_complex_documents(self):
        """Test parsing complex documents."""
        yaml_content = """---
name: doc1
nested:
  key: value
  list:
    - item1
    - item2
---
name: doc2
data:
  - a
  - b
  - c
"""
        result = parallel.parse_parallel(yaml_content)
        assert len(result) == 2
        assert result[0]["name"] == "doc1"
        assert result[0]["nested"]["list"] == ["item1", "item2"]
        assert result[1]["data"] == ["a", "b", "c"]

    def test_parse_preserves_order(self):
        """Test that document order is preserved."""
        docs = [f"---\nid: {i}\n" for i in range(10)]
        yaml_content = "".join(docs)
        result = parallel.parse_parallel(yaml_content)
        assert len(result) == 10
        for i, doc in enumerate(result):
            assert doc["id"] == i

    def test_parse_various_types(self):
        """Test parsing various YAML types."""
        yaml_content = """---
string: hello
number: 42
float: 3.14
boolean: true
null_value: null
list: [1, 2, 3]
---
nested:
  deep:
    value: found
"""
        result = parallel.parse_parallel(yaml_content)
        assert len(result) == 2
        assert result[0]["string"] == "hello"
        assert result[0]["number"] == 42
        assert result[0]["float"] == 3.14
        assert result[0]["boolean"] is True
        assert result[0]["null_value"] is None
        assert result[0]["list"] == [1, 2, 3]
        assert result[1]["nested"]["deep"]["value"] == "found"


class TestParseParallelErrors:
    """Tests for error handling in parse_parallel."""

    def test_parse_invalid_yaml(self):
        """Test parsing invalid YAML."""
        with pytest.raises((ValueError, Exception)):
            parallel.parse_parallel("key: [\n")

    def test_parse_invalid_in_multi_doc(self):
        """Test parsing with invalid document in multi-doc stream."""
        yaml_content = """---
valid: true
---
invalid: [
---
also_valid: true
"""
        with pytest.raises((ValueError, Exception)):
            parallel.parse_parallel(yaml_content)


class TestParseParallelLimits:
    """Tests for input limits in parse_parallel."""

    def test_max_documents_limit(self):
        """Test that max_documents limit is enforced."""
        config = parallel.ParallelConfig(max_documents=5)
        # Create more documents than allowed
        docs = [f"---\nid: {i}\n" for i in range(10)]
        yaml_content = "".join(docs)

        with pytest.raises((ValueError, Exception)):
            parallel.parse_parallel(yaml_content, config)

    def test_within_document_limit(self):
        """Test parsing within document limit."""
        config = parallel.ParallelConfig(max_documents=10)
        docs = [f"---\nid: {i}\n" for i in range(5)]
        yaml_content = "".join(docs)

        result = parallel.parse_parallel(yaml_content, config)
        assert len(result) == 5


class TestParseParallelPerformance:
    """Performance-related tests for parse_parallel."""

    def test_large_number_of_documents(self):
        """Test parsing many small documents."""
        docs = [f"---\nid: {i}\nvalue: test_{i}\n" for i in range(100)]
        yaml_content = "".join(docs)

        result = parallel.parse_parallel(yaml_content)
        assert len(result) == 100
        # Verify order is preserved
        for i, doc in enumerate(result):
            assert doc["id"] == i

    def test_single_thread_mode(self):
        """Test single-threaded mode."""
        config = parallel.ParallelConfig(thread_count=1)
        yaml_content = "---\na: 1\n---\nb: 2\n---\nc: 3\n"

        result = parallel.parse_parallel(yaml_content, config)
        assert len(result) == 3

    def test_many_threads(self):
        """Test with many threads."""
        config = parallel.ParallelConfig(thread_count=8)
        docs = [f"---\nid: {i}\n" for i in range(50)]
        yaml_content = "".join(docs)

        result = parallel.parse_parallel(yaml_content, config)
        assert len(result) == 50


class TestParseParallelEdgeCases:
    """Edge case tests for parse_parallel."""

    def test_unicode_content(self):
        """Test parsing unicode content."""
        yaml_content = """---
chinese: \u4e2d\u6587
emoji: \U0001f600
russian: \u041f\u0440\u0438\u0432\u0435\u0442
---
japanese: \u3053\u3093\u306b\u3061\u306f
"""
        result = parallel.parse_parallel(yaml_content)
        assert len(result) == 2
        assert result[0]["chinese"] == "\u4e2d\u6587"
        assert result[1]["japanese"] == "\u3053\u3093\u306b\u3061\u306f"

    def test_multiline_strings(self):
        """Test parsing multiline strings."""
        yaml_content = """---
literal: |
  Line 1
  Line 2
  Line 3
---
folded: >
  This is
  a folded
  string
"""
        result = parallel.parse_parallel(yaml_content)
        assert len(result) == 2
        assert "Line 1" in result[0]["literal"]
        assert "Line 2" in result[0]["literal"]

    def test_anchors_and_aliases(self):
        """Test parsing anchors and aliases."""
        yaml_content = """---
defaults: &defaults
  timeout: 30
  retries: 3
production: *defaults
"""
        result = parallel.parse_parallel(yaml_content)
        assert len(result) == 1
        assert result[0]["production"]["timeout"] == 30
        assert result[0]["production"]["retries"] == 3

    def test_document_end_marker(self):
        """Test parsing with document end markers."""
        yaml_content = """---
doc: 1
...
---
doc: 2
...
"""
        result = parallel.parse_parallel(yaml_content)
        assert len(result) == 2

    def test_no_explicit_start(self):
        """Test parsing without explicit document start."""
        yaml_content = "key: value\n"
        result = parallel.parse_parallel(yaml_content)
        assert len(result) == 1
        assert result[0] == {"key": "value"}

    def test_mixed_document_markers(self):
        """Test parsing with mixed document markers."""
        yaml_content = """
key1: value1
---
key2: value2
---
key3: value3
"""
        result = parallel.parse_parallel(yaml_content)
        assert len(result) >= 2  # At least 2-3 documents

    def test_special_float_values(self):
        """Test parsing special float values."""
        yaml_content = """---
infinity: .inf
neg_infinity: -.inf
not_a_number: .nan
"""
        result = parallel.parse_parallel(yaml_content)
        assert len(result) == 1
        import math

        assert math.isinf(result[0]["infinity"])
        assert result[0]["infinity"] > 0
        assert math.isinf(result[0]["neg_infinity"])
        assert result[0]["neg_infinity"] < 0
        assert math.isnan(result[0]["not_a_number"])

    def test_null_values(self):
        """Test parsing null values."""
        yaml_content = """---
null1: null
null2: ~
null3:
"""
        result = parallel.parse_parallel(yaml_content)
        assert len(result) == 1
        assert result[0]["null1"] is None
        assert result[0]["null2"] is None
        assert result[0]["null3"] is None


class TestParseParallelComplex:
    """Complex scenario tests for parse_parallel."""

    def test_deeply_nested_structure(self):
        """Test parsing deeply nested structures."""
        yaml_content = """---
level1:
  level2:
    level3:
      level4:
        level5:
          value: deep
"""
        result = parallel.parse_parallel(yaml_content)
        assert result[0]["level1"]["level2"]["level3"]["level4"]["level5"]["value"] == "deep"

    def test_large_arrays(self):
        """Test parsing large arrays."""
        items = ", ".join(str(i) for i in range(100))
        yaml_content = f"---\narray: [{items}]\n"

        result = parallel.parse_parallel(yaml_content)
        assert len(result[0]["array"]) == 100

    def test_mixed_types_in_documents(self):
        """Test documents with different root types."""
        yaml_content = """---
- item1
- item2
---
key: value
---
just a string
"""
        result = parallel.parse_parallel(yaml_content)
        assert len(result) == 3
        assert result[0] == ["item1", "item2"]
        assert result[1] == {"key": "value"}
        assert result[2] == "just a string"
