"""
Comprehensive tests for fast-yaml.

Run with: pytest tests/

Covers YAML 1.2.2 specification compliance:
https://yaml.org/spec/1.2.2/
"""

import math
import pytest
from io import StringIO


class TestSafeLoad:
    """Tests for safe_load function."""
    
    def test_empty_document(self):
        import fast_yaml
        assert fast_yaml.safe_load("") is None
        assert fast_yaml.safe_load("---") is None
    
    def test_null_values(self):
        import fast_yaml
        assert fast_yaml.safe_load("~") is None
        assert fast_yaml.safe_load("null") is None
        # Note: yaml-rust2 only recognizes lowercase 'null', not 'Null' or 'NULL'
        # This is a known limitation of the Core Schema implementation

    @pytest.mark.xfail(reason="yaml-rust2 only recognizes lowercase 'null'")
    def test_null_values_case_variants(self):
        import fast_yaml
        assert fast_yaml.safe_load("Null") is None
        assert fast_yaml.safe_load("NULL") is None
    
    def test_boolean_values(self):
        import fast_yaml
        # YAML 1.2 Core Schema only recognizes lowercase true/false
        assert fast_yaml.safe_load("true") is True
        assert fast_yaml.safe_load("false") is False
        # Capitalized versions are strings in YAML 1.2
        assert fast_yaml.safe_load("True") == "True"
        assert fast_yaml.safe_load("False") == "False"
    
    def test_integer_values(self):
        import fast_yaml
        assert fast_yaml.safe_load("42") == 42
        assert fast_yaml.safe_load("-17") == -17
        assert fast_yaml.safe_load("0") == 0
    
    def test_float_values(self):
        import fast_yaml
        assert fast_yaml.safe_load("3.14") == pytest.approx(3.14)
        assert fast_yaml.safe_load("-2.5") == pytest.approx(-2.5)
        assert fast_yaml.safe_load("1.0e10") == pytest.approx(1.0e10)
    
    def test_string_values(self):
        import fast_yaml
        assert fast_yaml.safe_load('"hello"') == "hello"
        assert fast_yaml.safe_load("'world'") == "world"
        assert fast_yaml.safe_load("plain string") == "plain string"
    
    def test_simple_list(self):
        import fast_yaml
        yaml = """
- one
- two
- three
"""
        result = fast_yaml.safe_load(yaml)
        assert result == ["one", "two", "three"]
    
    def test_simple_dict(self):
        import fast_yaml
        yaml = """
name: John
age: 30
active: true
"""
        result = fast_yaml.safe_load(yaml)
        assert result == {"name": "John", "age": 30, "active": True}
    
    def test_nested_structure(self):
        import fast_yaml
        yaml = """
person:
  name: John
  address:
    city: New York
    zip: 10001
  hobbies:
    - reading
    - coding
"""
        result = fast_yaml.safe_load(yaml)
        assert result["person"]["name"] == "John"
        assert result["person"]["address"]["city"] == "New York"
        assert result["person"]["hobbies"] == ["reading", "coding"]
    
    def test_mixed_types_in_list(self):
        import fast_yaml
        yaml = """
- hello
- 42
- 3.14
- true
- null
"""
        result = fast_yaml.safe_load(yaml)
        assert result == ["hello", 42, pytest.approx(3.14), True, None]
    
    def test_multiline_string(self):
        import fast_yaml
        yaml = """
description: |
  This is a
  multiline string
"""
        result = fast_yaml.safe_load(yaml)
        assert "multiline" in result["description"]
    
    def test_flow_style_list(self):
        import fast_yaml
        yaml = "items: [a, b, c]"
        result = fast_yaml.safe_load(yaml)
        assert result == {"items": ["a", "b", "c"]}
    
    def test_flow_style_dict(self):
        import fast_yaml
        yaml = "person: {name: John, age: 30}"
        result = fast_yaml.safe_load(yaml)
        assert result == {"person": {"name": "John", "age": 30}}
    
    @pytest.mark.xfail(reason="yaml-rust2 does not support YAML 1.1 merge key '<<:'")
    def test_anchors_and_aliases(self):
        import fast_yaml
        yaml = """
defaults: &defaults
  adapter: postgres
  host: localhost

development:
  <<: *defaults
  database: dev_db
"""
        result = fast_yaml.safe_load(yaml)
        assert result["development"]["adapter"] == "postgres"
        assert result["development"]["database"] == "dev_db"
    
    def test_from_file_object(self):
        import fast_yaml
        stream = StringIO("key: value")
        result = fast_yaml.safe_load(stream)
        assert result == {"key": "value"}
    
    def test_from_bytes(self):
        import fast_yaml
        data = b"key: value"
        result = fast_yaml.safe_load(data)
        assert result == {"key": "value"}
    
    def test_unicode(self):
        import fast_yaml
        yaml = "greeting: привет мир"
        result = fast_yaml.safe_load(yaml)
        assert result == {"greeting": "привет мир"}
    
    def test_invalid_yaml(self):
        import fast_yaml
        with pytest.raises(ValueError):
            fast_yaml.safe_load("key: value\n  invalid: indent")


# ============================================
# YAML 1.2.2 Compliance Tests
# https://yaml.org/spec/1.2.2/
# ============================================

class TestYAML122Null:
    """YAML 1.2.2 Section 10.2.1.1 - Null"""
    
    def test_tilde_null(self):
        import fast_yaml
        assert fast_yaml.safe_load("~") is None
    
    def test_null_keyword(self):
        import fast_yaml
        assert fast_yaml.safe_load("null") is None
        # Note: yaml-rust2 only recognizes lowercase 'null'

    @pytest.mark.xfail(reason="yaml-rust2 only recognizes lowercase 'null'")
    def test_null_keyword_case_variants(self):
        import fast_yaml
        assert fast_yaml.safe_load("Null") is None
        assert fast_yaml.safe_load("NULL") is None
    
    def test_empty_value_is_null(self):
        import fast_yaml
        result = fast_yaml.safe_load("key:")
        assert result == {"key": None}


class TestYAML122Boolean:
    """YAML 1.2.2 Section 10.2.1.2 - Boolean
    
    IMPORTANT: YAML 1.2 only recognizes true/false.
    yes/no/on/off are NOT booleans (unlike YAML 1.1).
    """
    
    def test_true_variants(self):
        import fast_yaml
        # YAML 1.2 Core Schema only recognizes lowercase true
        assert fast_yaml.safe_load("true") is True
        # Capitalized versions are strings
        assert fast_yaml.safe_load("True") == "True"
        assert fast_yaml.safe_load("TRUE") == "TRUE"

    def test_false_variants(self):
        import fast_yaml
        # YAML 1.2 Core Schema only recognizes lowercase false
        assert fast_yaml.safe_load("false") is False
        # Capitalized versions are strings
        assert fast_yaml.safe_load("False") == "False"
        assert fast_yaml.safe_load("FALSE") == "FALSE"
    
    def test_yaml11_booleans_are_strings(self):
        """YAML 1.1 boolean values should be strings in YAML 1.2"""
        import fast_yaml
        # These are NOT booleans in YAML 1.2!
        yaml11_bools = ["yes", "no", "on", "off", "y", "n", "Yes", "No", "On", "Off"]
        for value in yaml11_bools:
            result = fast_yaml.safe_load(value)
            assert isinstance(result, str), f"'{value}' should be string, not {type(result)}"


class TestYAML122Integer:
    """YAML 1.2.2 Section 10.2.1.3 - Integer"""
    
    def test_decimal(self):
        import fast_yaml
        assert fast_yaml.safe_load("0") == 0
        assert fast_yaml.safe_load("12345") == 12345
        assert fast_yaml.safe_load("+12345") == 12345
        assert fast_yaml.safe_load("-12345") == -12345
    
    def test_octal_yaml12(self):
        """YAML 1.2 requires 0o prefix for octal"""
        import fast_yaml
        assert fast_yaml.safe_load("0o14") == 12  # octal 14 = decimal 12
        assert fast_yaml.safe_load("0o777") == 511
    
    def test_hexadecimal(self):
        import fast_yaml
        assert fast_yaml.safe_load("0xC") == 12
        assert fast_yaml.safe_load("0xc") == 12
        assert fast_yaml.safe_load("0xFF") == 255
        assert fast_yaml.safe_load("0x0") == 0


class TestYAML122Float:
    """YAML 1.2.2 Section 10.2.1.4 - Floating Point"""
    
    def test_standard_float(self):
        import fast_yaml
        assert fast_yaml.safe_load("1.23") == pytest.approx(1.23)
        assert fast_yaml.safe_load("-1.23") == pytest.approx(-1.23)
        assert fast_yaml.safe_load("0.0") == pytest.approx(0.0)
    
    def test_exponential_notation(self):
        import fast_yaml
        assert fast_yaml.safe_load("1.23e+3") == pytest.approx(1230.0)
        assert fast_yaml.safe_load("1.23e-3") == pytest.approx(0.00123)
        assert fast_yaml.safe_load("1.23E+3") == pytest.approx(1230.0)
        assert fast_yaml.safe_load("12.3015e+02") == pytest.approx(1230.15)
    
    def test_positive_infinity(self):
        import fast_yaml
        for inf_str in [".inf", ".Inf", ".INF"]:
            result = fast_yaml.safe_load(inf_str)
            assert result == float('inf'), f"Failed for {inf_str}"
            assert math.isinf(result) and result > 0
    
    def test_negative_infinity(self):
        import fast_yaml
        for neg_inf_str in ["-.inf", "-.Inf", "-.INF"]:
            result = fast_yaml.safe_load(neg_inf_str)
            assert result == float('-inf'), f"Failed for {neg_inf_str}"
            assert math.isinf(result) and result < 0
    
    def test_nan(self):
        import fast_yaml
        for nan_str in [".nan", ".NaN", ".NAN"]:
            result = fast_yaml.safe_load(nan_str)
            assert math.isnan(result), f"Failed for {nan_str}"
    
    def test_special_floats_in_dict(self):
        import fast_yaml
        yaml = """
pos_inf: .inf
neg_inf: -.inf
not_a_number: .nan
"""
        result = fast_yaml.safe_load(yaml)
        assert math.isinf(result["pos_inf"]) and result["pos_inf"] > 0
        assert math.isinf(result["neg_inf"]) and result["neg_inf"] < 0
        assert math.isnan(result["not_a_number"])


class TestYAML122BlockScalars:
    """YAML 1.2.2 Chapter 8 - Block Scalar Styles"""
    
    def test_literal_style(self):
        """Literal style | preserves newlines"""
        import fast_yaml
        yaml = """text: |
  line1
  line2
"""
        result = fast_yaml.safe_load(yaml)
        assert "line1\n" in result["text"]
        assert "line2" in result["text"]
    
    def test_folded_style(self):
        """Folded style > converts newlines to spaces"""
        import fast_yaml
        yaml = """text: >
  line1
  line2
"""
        result = fast_yaml.safe_load(yaml)
        # Newlines between lines should become spaces
        assert "line1" in result["text"]
        assert "line2" in result["text"]
    
    def test_chomping_strip(self):
        """Strip chomping |- removes trailing newlines"""
        import fast_yaml
        yaml = """text: |-
  content
  
"""
        result = fast_yaml.safe_load(yaml)
        assert not result["text"].endswith("\n")
    
    def test_chomping_keep(self):
        """Keep chomping |+ preserves trailing newlines"""
        import fast_yaml
        yaml = """text: |+
  content

"""
        result = fast_yaml.safe_load(yaml)
        assert result["text"].endswith("\n")


class TestYAML122MultiDocument:
    """YAML 1.2.2 Chapter 9 - Document Stream"""
    
    def test_document_separators(self):
        import fast_yaml
        yaml = """---
doc1: first
---
doc2: second
"""
        result = list(fast_yaml.safe_load_all(yaml))
        assert len(result) == 2
        assert result[0] == {"doc1": "first"}
        assert result[1] == {"doc2": "second"}
    
    def test_document_end_marker(self):
        import fast_yaml
        yaml = """---
data: value
...
"""
        result = fast_yaml.safe_load(yaml)
        assert result == {"data": "value"}


class TestYAML122FlowStyles:
    """YAML 1.2.2 Chapter 7 - Flow Style Productions"""
    
    def test_flow_sequence(self):
        import fast_yaml
        assert fast_yaml.safe_load("[1, 2, 3]") == [1, 2, 3]
        assert fast_yaml.safe_load("[ a, b , c ]") == ["a", "b", "c"]
    
    def test_flow_mapping(self):
        import fast_yaml
        result = fast_yaml.safe_load("{a: 1, b: 2}")
        assert result == {"a": 1, "b": 2}
    
    def test_nested_flow(self):
        import fast_yaml
        result = fast_yaml.safe_load("{items: [1, 2], meta: {count: 2}}")
        assert result == {"items": [1, 2], "meta": {"count": 2}}


class TestYAML122Anchors:
    """YAML 1.2.2 Section 3.2.2.2 - Anchors and Aliases"""
    
    def test_simple_anchor_alias(self):
        import fast_yaml
        yaml = """
anchor: &ref value
alias: *ref
"""
        result = fast_yaml.safe_load(yaml)
        assert result["anchor"] == "value"
        assert result["alias"] == "value"
    
    @pytest.mark.xfail(reason="yaml-rust2 does not support YAML 1.1 merge key '<<:'")
    def test_merge_key(self):
        """Merge key << combines mappings"""
        import fast_yaml
        yaml = """
defaults: &defaults
  host: localhost
  port: 5432

production:
  <<: *defaults
  host: prod.example.com
"""
        result = fast_yaml.safe_load(yaml)
        assert result["production"]["host"] == "prod.example.com"
        assert result["production"]["port"] == 5432


# ============================================
# Original Tests (Compatibility)
# ============================================

class TestSafeLoadAll:
    """Tests for safe_load_all function."""
    
    def test_single_document(self):
        import fast_yaml
        yaml = "key: value"
        result = list(fast_yaml.safe_load_all(yaml))
        assert result == [{"key": "value"}]
    
    def test_multiple_documents(self):
        import fast_yaml
        yaml = """---
foo: 1
---
bar: 2
---
baz: 3
"""
        result = list(fast_yaml.safe_load_all(yaml))
        assert result == [{"foo": 1}, {"bar": 2}, {"baz": 3}]
    
    def test_empty_documents(self):
        import fast_yaml
        yaml = "---\n---\n"
        result = list(fast_yaml.safe_load_all(yaml))
        assert result == [None, None]


class TestSafeDump:
    """Tests for safe_dump function."""
    
    def test_dump_none(self):
        import fast_yaml
        result = fast_yaml.safe_dump(None)
        assert "null" in result.lower() or result.strip() == "~"
    
    def test_dump_bool(self):
        import fast_yaml
        assert "true" in fast_yaml.safe_dump(True).lower()
        assert "false" in fast_yaml.safe_dump(False).lower()
    
    def test_dump_int(self):
        import fast_yaml
        assert "42" in fast_yaml.safe_dump(42)
    
    def test_dump_float(self):
        import fast_yaml
        result = fast_yaml.safe_dump(3.14)
        assert "3.14" in result
    
    def test_dump_string(self):
        import fast_yaml
        result = fast_yaml.safe_dump("hello")
        assert "hello" in result
    
    def test_dump_list(self):
        import fast_yaml
        result = fast_yaml.safe_dump(["a", "b", "c"])
        assert "a" in result
        assert "b" in result
        assert "c" in result
    
    def test_dump_dict(self):
        import fast_yaml
        result = fast_yaml.safe_dump({"name": "John", "age": 30})
        assert "name" in result
        assert "John" in result
        assert "age" in result
        assert "30" in result
    
    def test_dump_nested(self):
        import fast_yaml
        data = {
            "person": {
                "name": "John",
                "hobbies": ["reading", "coding"]
            }
        }
        result = fast_yaml.safe_dump(data)
        assert "person" in result
        assert "name" in result
        assert "hobbies" in result
    
    def test_sort_keys(self):
        import fast_yaml
        data = {"z": 1, "a": 2, "m": 3}
        result = fast_yaml.safe_dump(data, sort_keys=True)
        # 'a' should appear before 'm' which should appear before 'z'
        assert result.index("a") < result.index("m") < result.index("z")
    
    def test_dump_to_stream(self):
        import fast_yaml
        stream = StringIO()
        result = fast_yaml.safe_dump({"key": "value"}, stream)
        assert result is None
        assert "key" in stream.getvalue()
    
    def test_unicode_output(self):
        import fast_yaml
        data = {"greeting": "привет"}
        result = fast_yaml.safe_dump(data, allow_unicode=True)
        assert "привет" in result
    
    def test_roundtrip(self):
        import fast_yaml
        original = {
            "string": "hello",
            "integer": 42,
            "float": 3.14,
            "boolean": True,
            "null": None,
            "list": [1, 2, 3],
            "nested": {"a": 1, "b": 2}
        }
        yaml_str = fast_yaml.safe_dump(original)
        loaded = fast_yaml.safe_load(yaml_str)
        
        assert loaded["string"] == original["string"]
        assert loaded["integer"] == original["integer"]
        assert loaded["float"] == pytest.approx(original["float"])
        assert loaded["boolean"] == original["boolean"]
        assert loaded["null"] == original["null"]
        assert loaded["list"] == original["list"]
        assert loaded["nested"] == original["nested"]
    
    # YAML 1.2.2 Special Float Dump Tests
    def test_dump_positive_infinity(self):
        import fast_yaml
        result = fast_yaml.safe_dump(float('inf'))
        assert ".inf" in result.lower()
    
    def test_dump_negative_infinity(self):
        import fast_yaml
        result = fast_yaml.safe_dump(float('-inf'))
        assert "-.inf" in result.lower()
    
    def test_dump_nan(self):
        import fast_yaml
        result = fast_yaml.safe_dump(float('nan'))
        assert ".nan" in result.lower()
    
    def test_dump_special_floats_roundtrip(self):
        import fast_yaml
        # Test roundtrip for special floats
        for value in [float('inf'), float('-inf')]:
            yaml_str = fast_yaml.safe_dump(value)
            loaded = fast_yaml.safe_load(yaml_str)
            assert loaded == value, f"Roundtrip failed for {value}"
        
        # NaN requires special comparison (nan != nan)
        yaml_str = fast_yaml.safe_dump(float('nan'))
        loaded = fast_yaml.safe_load(yaml_str)
        assert math.isnan(loaded)
    
    def test_dump_special_floats_in_structure(self):
        import fast_yaml
        data = {
            "pos_inf": float('inf'),
            "neg_inf": float('-inf'),
            "nan_val": float('nan'),
            "normal": 3.14
        }
        yaml_str = fast_yaml.safe_dump(data)
        loaded = fast_yaml.safe_load(yaml_str)
        
        assert loaded["pos_inf"] == float('inf')
        assert loaded["neg_inf"] == float('-inf')
        assert math.isnan(loaded["nan_val"])
        assert loaded["normal"] == pytest.approx(3.14)


class TestSafeDumpAll:
    """Tests for safe_dump_all function."""
    
    def test_dump_multiple_documents(self):
        import fast_yaml
        docs = [{"a": 1}, {"b": 2}, {"c": 3}]
        result = fast_yaml.safe_dump_all(docs)
        assert result.count("---") >= 2
        assert "a" in result
        assert "b" in result
        assert "c" in result


class TestPyYAMLCompatibility:
    """Tests for PyYAML API compatibility."""
    
    def test_load_alias(self):
        import fast_yaml
        # fast_yaml.load should work like safe_load
        result = fast_yaml.load("key: value")
        assert result == {"key": "value"}
    
    def test_dump_alias(self):
        import fast_yaml
        # fast_yaml.dump should work like safe_dump
        result = fast_yaml.dump({"key": "value"})
        assert "key" in result


class TestEdgeCases:
    """Tests for edge cases and special scenarios."""
    
    def test_empty_dict(self):
        import fast_yaml
        assert fast_yaml.safe_load("{}") == {}
        result = fast_yaml.safe_dump({})
        loaded = fast_yaml.safe_load(result)
        assert loaded == {} or loaded is None
    
    def test_empty_list(self):
        import fast_yaml
        assert fast_yaml.safe_load("[]") == []
        result = fast_yaml.safe_dump([])
        loaded = fast_yaml.safe_load(result)
        assert loaded == [] or loaded is None
    
    def test_deeply_nested(self):
        import fast_yaml
        data = {"a": {"b": {"c": {"d": {"e": "deep"}}}}}
        yaml_str = fast_yaml.safe_dump(data)
        loaded = fast_yaml.safe_load(yaml_str)
        assert loaded["a"]["b"]["c"]["d"]["e"] == "deep"
    
    def test_large_list(self):
        import fast_yaml
        data = list(range(1000))
        yaml_str = fast_yaml.safe_dump(data)
        loaded = fast_yaml.safe_load(yaml_str)
        assert loaded == data
    
    def test_special_characters_in_string(self):
        import fast_yaml
        data = {"text": "hello: world\nline2\ttab"}
        yaml_str = fast_yaml.safe_dump(data)
        loaded = fast_yaml.safe_load(yaml_str)
        assert "hello" in loaded["text"]


# Benchmark tests (run with pytest-benchmark)
# Check if pytest-benchmark is available
try:
    import pytest_benchmark  # noqa: F401
    HAS_BENCHMARK = True
except ImportError:
    HAS_BENCHMARK = False


@pytest.mark.skipif(not HAS_BENCHMARK, reason="pytest-benchmark not installed")
class TestBenchmarks:
    """Performance benchmark tests."""

    @pytest.fixture
    def small_yaml(self):
        return "name: test\nvalue: 123"
    
    @pytest.fixture
    def medium_yaml(self):
        items = [{"id": i, "name": f"item_{i}", "active": i % 2 == 0} 
                 for i in range(100)]
        import fast_yaml
        return fast_yaml.safe_dump({"items": items})
    
    def test_benchmark_load_small(self, benchmark, small_yaml):
        import fast_yaml
        result = benchmark(fast_yaml.safe_load, small_yaml)
        assert result is not None
    
    def test_benchmark_load_medium(self, benchmark, medium_yaml):
        import fast_yaml
        result = benchmark(fast_yaml.safe_load, medium_yaml)
        assert result is not None
    
    def test_benchmark_dump_small(self, benchmark):
        import fast_yaml
        data = {"name": "test", "value": 123}
        result = benchmark(fast_yaml.safe_dump, data)
        assert result is not None


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
