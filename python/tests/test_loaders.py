"""Tests for PyYAML-compatible loader classes."""

import pytest

import fast_yaml


class TestSafeLoader:
    """Tests for SafeLoader class."""

    def test_instantiate(self):
        """SafeLoader can be instantiated."""
        loader = fast_yaml.SafeLoader()
        assert loader is not None

    def test_repr(self):
        """SafeLoader has a string representation."""
        loader = fast_yaml.SafeLoader()
        assert repr(loader) == "SafeLoader()"

    def test_load_with_safe_loader(self):
        """load() works with SafeLoader instance."""
        yaml_str = "name: test\nvalue: 123"
        loader = fast_yaml.SafeLoader()
        result = fast_yaml.load(yaml_str, loader)
        assert result == {"name": "test", "value": 123}

    def test_load_all_with_safe_loader(self):
        """load_all() works with SafeLoader instance."""
        yaml_str = "---\nfoo: 1\n---\nbar: 2"
        loader = fast_yaml.SafeLoader()
        result = fast_yaml.load_all(yaml_str, loader)
        assert list(result) == [{"foo": 1}, {"bar": 2}]


class TestFullLoader:
    """Tests for FullLoader class."""

    def test_instantiate(self):
        """FullLoader can be instantiated."""
        loader = fast_yaml.FullLoader()
        assert loader is not None

    def test_repr(self):
        """FullLoader has a string representation."""
        loader = fast_yaml.FullLoader()
        assert repr(loader) == "FullLoader()"

    def test_load_with_full_loader(self):
        """load() works with FullLoader instance."""
        yaml_str = "name: test\nvalue: 123"
        loader = fast_yaml.FullLoader()
        result = fast_yaml.load(yaml_str, loader)
        assert result == {"name": "test", "value": 123}

    def test_load_all_with_full_loader(self):
        """load_all() works with FullLoader instance."""
        yaml_str = "---\nfoo: 1\n---\nbar: 2"
        loader = fast_yaml.FullLoader()
        result = fast_yaml.load_all(yaml_str, loader)
        assert list(result) == [{"foo": 1}, {"bar": 2}]


class TestLoader:
    """Tests for Loader class (alias for SafeLoader)."""

    def test_instantiate(self):
        """Loader can be instantiated."""
        loader = fast_yaml.Loader()
        assert loader is not None

    def test_repr(self):
        """Loader has a string representation."""
        loader = fast_yaml.Loader()
        assert repr(loader) == "Loader()"

    def test_load_with_loader(self):
        """load() works with Loader instance."""
        yaml_str = "name: test\nvalue: 123"
        loader = fast_yaml.Loader()
        result = fast_yaml.load(yaml_str, loader)
        assert result == {"name": "test", "value": 123}

    def test_load_all_with_loader(self):
        """load_all() works with Loader instance."""
        yaml_str = "---\nfoo: 1\n---\nbar: 2"
        loader = fast_yaml.Loader()
        result = fast_yaml.load_all(yaml_str, loader)
        assert list(result) == [{"foo": 1}, {"bar": 2}]


class TestLoadFunction:
    """Tests for load() function."""

    def test_load_without_loader(self):
        """load() works without loader parameter (defaults to safe)."""
        yaml_str = "name: test\nvalue: 123"
        result = fast_yaml.load(yaml_str)
        assert result == {"name": "test", "value": 123}

    def test_load_with_none_loader(self):
        """load() works with loader=None."""
        yaml_str = "name: test\nvalue: 123"
        result = fast_yaml.load(yaml_str, None)
        assert result == {"name": "test", "value": 123}

    def test_load_simple_types(self):
        """load() handles simple types correctly."""
        test_cases = [
            ("null", None),
            ("~", None),
            ("true", True),
            ("false", False),
            ("123", 123),
            ("1.5", 1.5),
            ("hello", "hello"),
        ]
        for yaml_str, expected in test_cases:
            result = fast_yaml.load(yaml_str)
            if expected is None or isinstance(expected, bool):
                assert result is expected or result == expected
            else:
                assert result == expected

    def test_load_dict(self):
        """load() handles dictionaries."""
        yaml_str = """
name: John
age: 30
city: NYC
"""
        result = fast_yaml.load(yaml_str)
        assert result == {"name": "John", "age": 30, "city": "NYC"}

    def test_load_list(self):
        """load() handles lists."""
        yaml_str = """
- apple
- banana
- cherry
"""
        result = fast_yaml.load(yaml_str)
        assert list(result) == ["apple", "banana", "cherry"]

    def test_load_nested(self):
        """load() handles nested structures."""
        yaml_str = """
person:
  name: John
  hobbies:
    - reading
    - coding
"""
        result = fast_yaml.load(yaml_str)
        assert result == {"person": {"name": "John", "hobbies": ["reading", "coding"]}}

    def test_load_empty(self):
        """load() returns None for empty document."""
        result = fast_yaml.load("")
        assert result is None

    def test_load_invalid_yaml(self):
        """load() raises ValueError for invalid YAML."""
        with pytest.raises(ValueError):
            fast_yaml.load("{ invalid: yaml: }")


class TestLoadAllFunction:
    """Tests for load_all() function."""

    def test_load_all_without_loader(self):
        """load_all() works without loader parameter."""
        yaml_str = "---\nfoo: 1\n---\nbar: 2"
        result = fast_yaml.load_all(yaml_str)
        assert list(result) == [{"foo": 1}, {"bar": 2}]

    def test_load_all_with_none_loader(self):
        """load_all() works with loader=None."""
        yaml_str = "---\nfoo: 1\n---\nbar: 2"
        result = fast_yaml.load_all(yaml_str, None)
        assert list(result) == [{"foo": 1}, {"bar": 2}]

    def test_load_all_single_document(self):
        """load_all() handles single document."""
        yaml_str = "name: test"
        result = fast_yaml.load_all(yaml_str)
        assert list(result) == [{"name": "test"}]

    def test_load_all_multiple_documents(self):
        """load_all() handles multiple documents."""
        yaml_str = """---
name: doc1
---
name: doc2
---
name: doc3
"""
        result = list(fast_yaml.load_all(yaml_str))
        assert len(result) == 3
        assert result[0] == {"name": "doc1"}
        assert result[1] == {"name": "doc2"}
        assert result[2] == {"name": "doc3"}

    def test_load_all_explicit_end(self):
        """load_all() handles documents with explicit end markers."""
        yaml_str = """---
foo: 1
...
---
bar: 2
..."""
        result = fast_yaml.load_all(yaml_str)
        assert list(result) == [{"foo": 1}, {"bar": 2}]

    def test_load_all_empty(self):
        """load_all() returns empty list for empty input."""
        result = fast_yaml.load_all("")
        assert list(result) == []

    def test_load_all_invalid_yaml(self):
        """load_all() raises ValueError for invalid YAML."""
        with pytest.raises(ValueError):
            fast_yaml.load_all("{ invalid: yaml: }")


class TestLoaderCompatibility:
    """Tests for PyYAML API compatibility."""

    def test_all_loaders_behave_same(self):
        """All loaders produce identical output for safe YAML."""
        yaml_str = "name: test\nvalue: 123"

        result_safe = fast_yaml.load(yaml_str, fast_yaml.SafeLoader())
        result_full = fast_yaml.load(yaml_str, fast_yaml.FullLoader())
        result_loader = fast_yaml.load(yaml_str, fast_yaml.Loader())
        result_default = fast_yaml.load(yaml_str)

        assert result_safe == result_full == result_loader == result_default

    def test_loader_types_are_different(self):
        """Loader classes are distinct types."""
        safe_loader = fast_yaml.SafeLoader()
        full_loader = fast_yaml.FullLoader()
        loader = fast_yaml.Loader()

        assert type(safe_loader).__name__ == "SafeLoader"
        assert type(full_loader).__name__ == "FullLoader"
        assert type(loader).__name__ == "Loader"


class TestInputValidation:
    """Tests for input validation and limits."""

    def test_load_size_limit(self):
        """load() enforces 100MB size limit."""
        # Create a string just over 100MB
        large_yaml = "key: " + ("x" * (100 * 1024 * 1024))
        with pytest.raises(ValueError, match="exceeds maximum"):
            fast_yaml.load(large_yaml)

    def test_load_all_size_limit(self):
        """load_all() enforces 100MB size limit."""
        # Create a string just over 100MB
        large_yaml = "key: " + ("x" * (100 * 1024 * 1024))
        with pytest.raises(ValueError, match="exceeds maximum"):
            fast_yaml.load_all(large_yaml)
