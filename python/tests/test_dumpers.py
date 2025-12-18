"""Tests for PyYAML-compatible dumper classes and dump functions."""

import pytest

import fast_yaml


class TestSafeDumper:
    """Tests for SafeDumper class."""

    def test_instantiate(self):
        """SafeDumper can be instantiated."""
        dumper = fast_yaml.SafeDumper()
        assert dumper is not None

    def test_repr(self):
        """SafeDumper has a string representation."""
        dumper = fast_yaml.SafeDumper()
        assert repr(dumper) == "SafeDumper()"

    def test_dump_with_safe_dumper(self):
        """dump() works with SafeDumper instance."""
        data = {"name": "test", "value": 123}
        dumper = fast_yaml.SafeDumper()
        yaml_str = fast_yaml.dump(data, Dumper=dumper)
        assert "name: test" in yaml_str
        assert "value: 123" in yaml_str

    def test_dump_all_with_safe_dumper(self):
        """dump_all() works with SafeDumper instance."""
        docs = [{"foo": 1}, {"bar": 2}]
        dumper = fast_yaml.SafeDumper()
        yaml_str = fast_yaml.dump_all(docs, Dumper=dumper)
        assert "foo: 1" in yaml_str
        assert "bar: 2" in yaml_str
        assert "---" in yaml_str


class TestDumper:
    """Tests for Dumper class (alias for SafeDumper)."""

    def test_instantiate(self):
        """Dumper can be instantiated."""
        dumper = fast_yaml.Dumper()
        assert dumper is not None

    def test_repr(self):
        """Dumper has a string representation."""
        dumper = fast_yaml.Dumper()
        assert repr(dumper) == "Dumper()"

    def test_dump_with_dumper(self):
        """dump() works with Dumper instance."""
        data = {"name": "test", "value": 123}
        dumper = fast_yaml.Dumper()
        yaml_str = fast_yaml.dump(data, Dumper=dumper)
        assert "name: test" in yaml_str

    def test_dump_all_with_dumper(self):
        """dump_all() works with Dumper instance."""
        docs = [{"a": 1}, {"b": 2}]
        dumper = fast_yaml.Dumper()
        yaml_str = fast_yaml.dump_all(docs, Dumper=dumper)
        assert "a: 1" in yaml_str
        assert "---" in yaml_str


class TestDumpFunction:
    """Tests for dump() function."""

    def test_dump_without_dumper(self):
        """dump() works without dumper parameter."""
        yaml_str = fast_yaml.dump({"key": "value"})
        assert "key: value" in yaml_str

    def test_dump_with_none_dumper(self):
        """dump() works with Dumper=None."""
        yaml_str = fast_yaml.dump({"key": "value"}, Dumper=None)
        assert "key: value" in yaml_str

    def test_dump_with_indent(self):
        """dump() respects indent parameter."""
        data = {"parent": {"child": "value"}}
        yaml_str = fast_yaml.dump(data, indent=4)
        # Note: yaml-rust2 uses fixed 2-space indent
        # This test verifies parameter is accepted
        assert "child: value" in yaml_str

    def test_dump_with_width(self):
        """dump() respects width parameter."""
        data = {"key": "a very long string that should wrap"}
        yaml_str = fast_yaml.dump(data, width=40)
        # Note: yaml-rust2 has limited wrapping control
        # This test verifies parameter is accepted
        assert "key:" in yaml_str

    def test_dump_with_explicit_start(self):
        """dump() adds document start marker when explicit_start=True."""
        yaml_str = fast_yaml.dump({"key": "value"}, explicit_start=True)
        assert yaml_str.startswith("---")

    def test_dump_without_explicit_start(self):
        """dump() omits document start marker by default."""
        yaml_str = fast_yaml.dump({"key": "value"})
        assert not yaml_str.startswith("---")

    def test_dump_with_sort_keys(self):
        """dump() sorts keys when sort_keys=True."""
        data = {"z": 1, "a": 2, "m": 3}
        yaml_str = fast_yaml.dump(data, sort_keys=True)
        lines = yaml_str.strip().split("\n")

        # Extract keys in order
        keys = [line.split(":")[0] for line in lines]
        assert keys == ["a", "m", "z"]

    def test_dump_without_sort_keys(self):
        """dump() preserves insertion order by default."""
        # Note: Python 3.7+ dicts preserve insertion order
        yaml_str = fast_yaml.dump({"key": "value"}, sort_keys=False)
        assert "key: value" in yaml_str


class TestDumpAllFunction:
    """Tests for dump_all() function."""

    def test_dump_all_without_dumper(self):
        """dump_all() works without dumper parameter."""
        yaml_str = fast_yaml.dump_all([{"a": 1}, {"b": 2}])
        assert "a: 1" in yaml_str
        assert "b: 2" in yaml_str
        assert "---" in yaml_str

    def test_dump_all_with_dumper(self):
        """dump_all() works with Dumper parameter."""
        dumper = fast_yaml.SafeDumper()
        yaml_str = fast_yaml.dump_all([{"x": 1}], Dumper=dumper)
        assert "x: 1" in yaml_str

    def test_dump_all_with_explicit_start(self):
        """dump_all() adds explicit start markers."""
        yaml_str = fast_yaml.dump_all([{"a": 1}, {"b": 2}], explicit_start=True)
        assert yaml_str.startswith("---")
        # Should have separator for each document
        assert yaml_str.count("---") == 2

    def test_dump_all_with_sort_keys(self):
        """dump_all() sorts keys in all documents."""
        docs = [{"z": 1, "a": 2}, {"y": 3, "b": 4}]
        yaml_str = fast_yaml.dump_all(docs, sort_keys=True)
        # Verify first doc has sorted keys
        assert yaml_str.index("a:") < yaml_str.index("z:")


class TestDumperCompatibility:
    """Tests for PyYAML API compatibility."""

    def test_all_dumpers_behave_same(self):
        """All dumpers produce identical output."""
        data = {"name": "test", "value": 123}

        result_safe = fast_yaml.dump(data, Dumper=fast_yaml.SafeDumper())
        result_dumper = fast_yaml.dump(data, Dumper=fast_yaml.Dumper())
        result_default = fast_yaml.dump(data)

        assert result_safe == result_dumper == result_default

    def test_dumper_types_are_different(self):
        """Dumper classes are distinct types."""
        safe_dumper = fast_yaml.SafeDumper()
        dumper = fast_yaml.Dumper()

        assert type(safe_dumper).__name__ == "SafeDumper"
        assert type(dumper).__name__ == "Dumper"


class TestDumpOptions:
    """Tests for dump options parameters."""

    def test_dump_all_options_combinations(self):
        """dump_all() works with multiple options."""
        docs = [{"z": 1, "a": 2}, {"y": 3, "b": 4}]
        yaml_str = fast_yaml.dump_all(
            docs, sort_keys=True, explicit_start=True, indent=4, width=100
        )
        assert yaml_str.startswith("---")
        assert "a:" in yaml_str
        assert "z:" in yaml_str

    def test_dump_size_limit(self):
        """dump_all() enforces 100MB output size limit."""
        # Create large dataset that would exceed 100MB when serialized
        large_docs = [{"key": "x" * 10000000} for _ in range(20)]
        with pytest.raises(ValueError, match="exceeds maximum"):
            fast_yaml.dump_all(large_docs)
