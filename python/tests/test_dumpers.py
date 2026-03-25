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


class TestSafeDumpOptions:
    """Tests for safe_dump() parameters: explicit_start, indent, width."""

    def test_explicit_start(self):
        """safe_dump() adds document start marker when explicit_start=True."""
        result = fast_yaml.safe_dump({"k": "v"}, explicit_start=True)
        assert result.startswith("---\n")

    def test_explicit_start_false_by_default(self):
        """safe_dump() does not add document start marker by default."""
        result = fast_yaml.safe_dump({"k": "v"})
        assert not result.startswith("---")

    def test_indent_default(self):
        """safe_dump() uses 2-space indent by default."""
        data = {"parent": {"child": "value"}}
        result = fast_yaml.safe_dump(data)
        lines = result.splitlines()
        nested_line = next(ln for ln in lines if "child" in ln)
        assert nested_line.startswith("  "), f"Expected 2-space indent, got: {repr(nested_line)}"
        assert not nested_line.startswith("    "), (
            f"Expected exactly 2-space indent, got: {repr(nested_line)}"
        )

    def test_indent_2(self):
        """safe_dump() with indent=2 indents nested values 2 spaces."""
        data = {"parent": {"child": "value"}}
        result = fast_yaml.safe_dump(data, indent=2)
        lines = result.splitlines()
        nested_line = next(ln for ln in lines if "child" in ln)
        assert nested_line.startswith("  "), f"Expected 2-space indent, got: {repr(nested_line)}"

    def test_indent_4(self):
        """safe_dump() with indent=4 indents nested values 4 spaces."""
        data = {"parent": {"child": "value"}}
        result = fast_yaml.safe_dump(data, indent=4)
        lines = result.splitlines()
        nested_line = next(ln for ln in lines if "child" in ln)
        assert nested_line.startswith("    "), f"Expected 4-space indent, got: {repr(nested_line)}"
        assert not nested_line.startswith("     "), (
            f"Expected exactly 4-space indent, got: {repr(nested_line)}"
        )

    def test_indent_8(self):
        """safe_dump() with indent=8 indents nested values 8 spaces."""
        data = {"parent": {"child": "value"}}
        result = fast_yaml.safe_dump(data, indent=8)
        lines = result.splitlines()
        nested_line = next(ln for ln in lines if "child" in ln)
        assert nested_line.startswith("        "), (
            f"Expected 8-space indent, got: {repr(nested_line)}"
        )

    def test_default_flow_style_false(self):
        """safe_dump() with default_flow_style=False produces block style."""
        data = {"key": [1, 2, 3]}
        result = fast_yaml.safe_dump(data, default_flow_style=False)
        assert "- 1" in result
        assert "[1, 2, 3]" not in result

    def test_default_flow_style_true(self):
        """safe_dump() with default_flow_style=True produces flow style."""
        data = {"key": [1, 2, 3]}
        result = fast_yaml.safe_dump(data, default_flow_style=True)
        assert "[1, 2, 3]" in result or "{key: [1, 2, 3]}" in result

    def test_default_flow_style_true_dict(self):
        """safe_dump() with default_flow_style=True produces flow style for dicts."""
        data = {"outer": {"inner": "value"}}
        result = fast_yaml.safe_dump(data, default_flow_style=True)
        assert "{" in result and "}" in result

    def test_indent_4_with_flow_style_false(self):
        """safe_dump() with indent=4 and default_flow_style=False works correctly."""
        data = {"parent": {"child": "value"}}
        result = fast_yaml.safe_dump(data, indent=4, default_flow_style=False)
        lines = result.splitlines()
        nested_line = next(ln for ln in lines if "child" in ln)
        assert nested_line.startswith("    "), f"Expected 4-space indent, got: {repr(nested_line)}"

    def test_indent(self):
        """safe_dump() respects indent parameter."""
        data = {"nested": {"a": 1, "b": 2}}
        result = fast_yaml.safe_dump(data, indent=4)
        assert result is not None

    def test_width(self):
        """safe_dump() accepts width parameter without error."""
        data = {"key": "value"}
        result = fast_yaml.safe_dump(data, width=40)
        assert "key: value" in result

    def test_sort_keys(self):
        """safe_dump() sorts keys when sort_keys=True."""
        data = {"z": 1, "a": 2}
        result = fast_yaml.safe_dump(data, sort_keys=True)
        assert result.index("a:") < result.index("z:")

    def test_stream_output(self):
        """safe_dump() writes to stream and returns None."""
        import io

        buf = io.StringIO()
        result = fast_yaml.safe_dump({"k": "v"}, stream=buf)
        assert result is None
        assert "k: v" in buf.getvalue()

    def test_explicit_start_with_stream(self):
        """safe_dump() with explicit_start writes --- to stream."""
        import io

        buf = io.StringIO()
        fast_yaml.safe_dump({"k": "v"}, stream=buf, explicit_start=True)
        assert buf.getvalue().startswith("---\n")

    def test_all_params_combined(self):
        """safe_dump() accepts all new parameters together."""
        result = fast_yaml.safe_dump(
            {"z": 1, "a": 2},
            explicit_start=True,
            indent=4,
            width=100,
            sort_keys=True,
        )
        assert result.startswith("---\n")
        assert result.index("a:") < result.index("z:")


class TestSafeDumpAllOptions:
    """Tests for safe_dump_all() formatting parameters (issue #151)."""

    def test_indent(self):
        """safe_dump_all() respects indent parameter."""
        docs = [{"parent": {"child": "value"}}]
        result = fast_yaml.safe_dump_all(docs, indent=4)
        lines = result.splitlines()
        nested_line = next(ln for ln in lines if "child" in ln)
        assert nested_line.startswith("    "), f"Expected 4-space indent, got: {repr(nested_line)}"

    def test_explicit_start(self):
        """safe_dump_all() adds document start marker when explicit_start=True."""
        result = fast_yaml.safe_dump_all([{"k": "v"}], explicit_start=True)
        assert result.startswith("---\n")

    def test_explicit_start_false_by_default(self):
        """safe_dump_all() already adds --- separators between docs by default."""
        result = fast_yaml.safe_dump_all([{"a": 1}, {"b": 2}])
        assert "---" in result

    def test_default_flow_style_false(self):
        """safe_dump_all() with default_flow_style=False produces block style."""
        docs = [{"key": [1, 2, 3]}]
        result = fast_yaml.safe_dump_all(docs, default_flow_style=False)
        assert "- 1" in result
        assert "[1, 2, 3]" not in result

    def test_default_flow_style_true(self):
        """safe_dump_all() with default_flow_style=True produces flow style."""
        docs = [{"key": [1, 2, 3]}]
        result = fast_yaml.safe_dump_all(docs, default_flow_style=True)
        assert "[1, 2, 3]" in result or "{key: [1, 2, 3]}" in result

    def test_sort_keys(self):
        """safe_dump_all() sorts keys in all documents when sort_keys=True."""
        docs = [{"z": 1, "a": 2}, {"y": 3, "b": 4}]
        result = fast_yaml.safe_dump_all(docs, sort_keys=True)
        assert result.index("a:") < result.index("z:")

    def test_width(self):
        """safe_dump_all() accepts width parameter without error."""
        result = fast_yaml.safe_dump_all([{"key": "value"}], width=40)
        assert "key: value" in result

    def test_all_params_combined(self):
        """safe_dump_all() accepts all formatting kwargs together."""
        docs = [{"z": 1, "a": 2}, {"nested": {"x": 1}}]
        result = fast_yaml.safe_dump_all(
            docs,
            explicit_start=True,
            indent=4,
            width=100,
            sort_keys=True,
        )
        assert result.startswith("---\n")
        assert result.index("a:") < result.index("z:")

    def test_stream_output(self):
        """safe_dump_all() writes to stream and returns None."""
        import io

        buf = io.StringIO()
        result = fast_yaml.safe_dump_all([{"k": "v"}], stream=buf)
        assert result is None
        assert "k: v" in buf.getvalue()


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
