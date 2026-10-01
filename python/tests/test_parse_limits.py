"""Configurable parse limits: max_depth, max_alias_bytes and max_scan_ahead (#372, #563)."""

from __future__ import annotations

import pytest

import fast_yaml
from fast_yaml import parallel as yaml_parallel
from fast_yaml._core import batch, lint
from fast_yaml._core import parallel as core_parallel

MIB = 1024 * 1024
MAX_DEPTH = 512
MAX_ALIAS_BYTES = 1 << 30

# 1500 aliases of a 1000-item anchor: over the 64 MiB default estimate, tiny in memory.
ALIAS_HEAVY = "a: &a [" + ",".join(["x"] * 1000) + "]\nb: [" + ",".join(["*a"] * 1500) + "]\n"
NESTED_300 = "- " * 300 + "x"

LOADERS = [
    pytest.param(fast_yaml.safe_load, id="safe_load"),
    pytest.param(lambda s, **kw: list(fast_yaml.safe_load_all(s, **kw))[0], id="safe_load_all"),
    pytest.param(fast_yaml.load, id="load"),
    pytest.param(lambda s, **kw: list(fast_yaml.load_all(s, **kw))[0], id="load_all"),
]


@pytest.mark.parametrize("load", LOADERS)
class TestLoaders:
    def test_default_rejects_alias_heavy(self, load):
        with pytest.raises(ValueError, match="alias expansion exceeds"):
            load(ALIAS_HEAVY)

    def test_raised_alias_budget_succeeds(self, load):
        assert len(load(ALIAS_HEAVY, max_alias_bytes=256 * MIB)["b"]) == 1500

    def test_lowered_alias_budget_rejects(self, load):
        with pytest.raises(ValueError, match="alias expansion exceeds"):
            load(ALIAS_HEAVY, max_alias_bytes=1024)

    def test_default_rejects_depth_300(self, load):
        with pytest.raises(ValueError, match="nesting depth exceeds 256"):
            load(NESTED_300)

    def test_raised_depth_succeeds(self, load):
        assert load(NESTED_300, max_depth=MAX_DEPTH) is not None

    def test_lowered_depth_rejects(self, load):
        with pytest.raises(ValueError, match="nesting depth exceeds 4"):
            load("[[[[[[1]]]]]]", max_depth=4)

    def test_defaults_unchanged(self, load):
        assert load("a: [1, 2]") == {"a": [1, 2]}

    @pytest.mark.parametrize("value", [0, -1, MAX_DEPTH + 1, 2**70])
    def test_invalid_depth(self, load, value):
        with pytest.raises(ValueError, match=rf"max_depth must be between 1 and {MAX_DEPTH}, got"):
            load("a: 1", max_depth=value)

    @pytest.mark.parametrize("value", [0, -1, MAX_ALIAS_BYTES + 1, 2**70])
    def test_invalid_alias_bytes(self, load, value):
        with pytest.raises(
            ValueError,
            match=rf"max_alias_bytes must be between 1 and {MAX_ALIAS_BYTES}, got",
        ):
            load("a: 1", max_alias_bytes=value)

    def test_bounds_accepted(self, load):
        assert load("a: 1", max_depth=2) == {"a": 1}
        assert load("a: 1", max_depth=MAX_DEPTH, max_alias_bytes=MAX_ALIAS_BYTES) == {"a": 1}

    @pytest.mark.parametrize("value", ["3", 1.5, True, False])
    def test_non_integer_is_type_error(self, load, value):
        with pytest.raises(TypeError):
            load("a: 1", max_depth=value)
        with pytest.raises(TypeError):
            load("a: 1", max_alias_bytes=value)

    def test_lower_bound_one_accepted(self, load):
        assert load("1", max_depth=1, max_alias_bytes=1) == 1
        with pytest.raises(ValueError, match="nesting depth exceeds 1"):
            load("[[1]]", max_depth=1)


def test_error_message_exact():
    with pytest.raises(ValueError) as info:
        fast_yaml.safe_load("a: 1", max_depth=0)
    assert str(info.value) == "max_depth must be between 1 and 512, got 0"


def test_limits_are_keyword_only():
    with pytest.raises(TypeError):
        fast_yaml.safe_load("a: 1", 512)  # type: ignore[misc]


class TestParallelConfig:
    def test_default_rejects_alias_heavy(self):
        with pytest.raises(ValueError, match="alias expansion exceeds"):
            yaml_parallel.parse_parallel(ALIAS_HEAVY)

    def test_ctor_kwarg_raises_budget(self):
        config = yaml_parallel.ParallelConfig(max_alias_bytes=256 * MIB)
        assert len(yaml_parallel.parse_parallel(ALIAS_HEAVY, config)[0]["b"]) == 1500

    def test_builder_raises_budget(self):
        config = yaml_parallel.ParallelConfig().with_max_alias_bytes(256 * MIB)
        assert len(yaml_parallel.parse_parallel(ALIAS_HEAVY, config)[0]["b"]) == 1500

    def test_ctor_kwarg_raises_depth(self):
        config = yaml_parallel.ParallelConfig(max_depth=MAX_DEPTH)
        assert yaml_parallel.parse_parallel(NESTED_300, config)

    def test_builder_lowers_depth(self):
        config = yaml_parallel.ParallelConfig().with_max_depth(2)
        with pytest.raises(ValueError, match="nesting depth exceeds 2"):
            yaml_parallel.parse_parallel("[[[1]]]", config)

    def test_builders_preserve_each_other(self):
        config = (
            yaml_parallel.ParallelConfig().with_max_depth(MAX_DEPTH).with_max_alias_bytes(256 * MIB)
        )
        assert yaml_parallel.parse_parallel(NESTED_300, config)
        assert yaml_parallel.parse_parallel(ALIAS_HEAVY, config)

    @pytest.mark.parametrize("value", [0, -1, MAX_DEPTH + 1])
    def test_invalid_depth(self, value):
        message = rf"max_depth must be between 1 and {MAX_DEPTH}, got {value}"
        with pytest.raises(ValueError, match=message):
            core_parallel.ParallelConfig(max_depth=value)
        with pytest.raises(ValueError, match=message):
            core_parallel.ParallelConfig().with_max_depth(value)

    @pytest.mark.parametrize("value", [0, -1, MAX_ALIAS_BYTES + 1])
    def test_invalid_alias_bytes(self, value):
        message = rf"max_alias_bytes must be between 1 and {MAX_ALIAS_BYTES}, got {value}"
        with pytest.raises(ValueError, match=message):
            core_parallel.ParallelConfig(max_alias_bytes=value)
        with pytest.raises(ValueError, match=message):
            core_parallel.ParallelConfig().with_max_alias_bytes(value)


class TestLintConfig:
    def test_default_reports_depth_error(self):
        with pytest.raises(ValueError, match="nesting depth exceeds 256"):
            lint.lint(NESTED_300)

    def test_ctor_kwarg_raises_depth(self):
        config = lint.LintConfig(max_depth=MAX_DEPTH)
        assert isinstance(lint.lint(NESTED_300, config), list)

    def test_builder_lowers_depth(self):
        config = lint.LintConfig().with_max_depth(2)
        with pytest.raises(ValueError, match="nesting depth exceeds 2"):
            lint.lint("[[[1]]]", config)

    def test_alias_budget(self):
        with pytest.raises(ValueError, match="alias expansion exceeds"):
            lint.lint(ALIAS_HEAVY)
        config = lint.LintConfig().with_max_alias_bytes(256 * MIB)
        assert isinstance(lint.lint(ALIAS_HEAVY, config), list)

    @pytest.mark.parametrize("value", [0, -1, MAX_DEPTH + 1])
    def test_invalid_depth(self, value):
        message = rf"max_depth must be between 1 and {MAX_DEPTH}, got {value}"
        with pytest.raises(ValueError, match=message):
            lint.LintConfig(max_depth=value)
        with pytest.raises(ValueError, match=message):
            lint.LintConfig().with_max_depth(value)

    @pytest.mark.parametrize("value", [0, -1, MAX_ALIAS_BYTES + 1])
    def test_invalid_alias_bytes(self, value):
        message = rf"max_alias_bytes must be between 1 and {MAX_ALIAS_BYTES}, got {value}"
        with pytest.raises(ValueError, match=message):
            lint.LintConfig(max_alias_bytes=value)
        with pytest.raises(ValueError, match=message):
            lint.LintConfig().with_max_alias_bytes(value)


class TestBatchConfig:
    @pytest.fixture
    def alias_file(self, tmp_path):
        path = tmp_path / "heavy.yaml"
        path.write_text(ALIAS_HEAVY)
        return str(path)

    def test_default_fails_file(self, alias_file):
        result = batch.process_files([alias_file])
        assert result.failed == 1
        assert "alias expansion exceeds" in result.errors()[0][1]

    def test_ctor_kwarg_raises_budget(self, alias_file):
        config = batch.BatchConfig(max_alias_bytes=256 * MIB)
        result = batch.process_files([alias_file], config)
        assert (result.total, result.failed) == (1, 0)

    def test_builder_raises_budget(self, alias_file):
        config = batch.BatchConfig().with_max_alias_bytes(256 * MIB)
        assert batch.process_files([alias_file], config).failed == 0

    def test_depth_limits(self, tmp_path):
        path = tmp_path / "deep.yaml"
        path.write_text(NESTED_300)
        assert batch.process_files([str(path)]).failed == 1
        assert batch.process_files([str(path)], batch.BatchConfig(max_depth=MAX_DEPTH)).failed == 0
        config = batch.BatchConfig().with_max_depth(MAX_DEPTH)
        assert batch.process_files([str(path)], config).failed == 0

    @pytest.mark.parametrize("value", [0, -1, MAX_DEPTH + 1])
    def test_invalid_depth(self, value):
        message = rf"max_depth must be between 1 and {MAX_DEPTH}, got {value}"
        with pytest.raises(ValueError, match=message):
            batch.BatchConfig(max_depth=value)
        with pytest.raises(ValueError, match=message):
            batch.BatchConfig().with_max_depth(value)

    @pytest.mark.parametrize("value", [0, -1, MAX_ALIAS_BYTES + 1])
    def test_invalid_alias_bytes(self, value):
        message = rf"max_alias_bytes must be between 1 and {MAX_ALIAS_BYTES}, got {value}"
        with pytest.raises(ValueError, match=message):
            batch.BatchConfig(max_alias_bytes=value)
        with pytest.raises(ValueError, match=message):
            batch.BatchConfig().with_max_alias_bytes(value)


@pytest.mark.parametrize(
    "make",
    [
        pytest.param(core_parallel.ParallelConfig, id="parallel"),
        pytest.param(lint.LintConfig, id="lint"),
        pytest.param(batch.BatchConfig, id="batch"),
    ],
)
class TestConfigs:
    @pytest.mark.parametrize("value", [True, False, "3", 1.5])
    def test_non_integer_is_type_error(self, make, value):
        with pytest.raises(TypeError):
            make(max_depth=value)
        with pytest.raises(TypeError):
            make(max_alias_bytes=value)
        with pytest.raises(TypeError):
            make().with_max_depth(value)
        with pytest.raises(TypeError):
            make().with_max_alias_bytes(value)

    def test_lower_bound_one_accepted(self, make):
        make(max_depth=1, max_alias_bytes=1)
        make().with_max_depth(1).with_max_alias_bytes(1)

    def test_builder_none_resets_to_default(self, make):
        make().with_max_depth(MAX_DEPTH).with_max_depth(None)
        make().with_max_alias_bytes(MIB).with_max_alias_bytes(None)


def test_parallel_builder_none_restores_default_behavior():
    config = yaml_parallel.ParallelConfig(max_depth=MAX_DEPTH).with_max_depth(None)
    with pytest.raises(ValueError, match="nesting depth exceeds 256"):
        yaml_parallel.parse_parallel(NESTED_300, config)


def test_lint_builder_none_restores_default_behavior():
    config = lint.LintConfig(max_depth=MAX_DEPTH).with_max_depth(None)
    with pytest.raises(ValueError, match="nesting depth exceeds 256"):
        lint.lint(NESTED_300, config)


def test_batch_builder_none_restores_default_behavior(tmp_path):
    path = tmp_path / "deep.yaml"
    path.write_text(NESTED_300)
    config = batch.BatchConfig(max_depth=MAX_DEPTH).with_max_depth(None)
    assert batch.process_files([str(path)], config).failed == 1


TOO_LARGE = r"input size \d+ bytes exceeds maximum allowed 16 bytes"
MAX_INPUT_BYTES = 1 << 30
SOURCE_16 = "a: " + "x" * 12 + "\n"


class TestMaxInputBytes:
    def test_boundary(self):
        assert len(SOURCE_16) == 16
        config = lint.LintConfig(max_input_bytes=16)
        lint.lint(SOURCE_16, config)
        with pytest.raises(ValueError, match=TOO_LARGE):
            lint.lint(SOURCE_16 + "#", config)

    def test_linter_class(self):
        linter = lint.Linter(lint.LintConfig(max_input_bytes=16))
        linter.lint(SOURCE_16)
        with pytest.raises(ValueError, match=TOO_LARGE):
            linter.lint(SOURCE_16 + "#")

    def test_counts_utf8_bytes(self):
        source = "a: " + "é" * 7 + "\n"
        assert len(source) < 16 < len(source.encode())
        with pytest.raises(ValueError, match=TOO_LARGE):
            lint.lint(source, lint.LintConfig(max_input_bytes=16))

    def test_builder(self):
        config = lint.LintConfig().with_max_input_bytes(16)
        with pytest.raises(ValueError, match=TOO_LARGE):
            lint.lint(SOURCE_16 + "#", config)

    def test_builder_none_resets_to_default(self):
        config = lint.LintConfig(max_input_bytes=16).with_max_input_bytes(None)
        lint.lint(SOURCE_16 + "#", config)

    @pytest.mark.parametrize("value", [0, -1, MAX_INPUT_BYTES + 1, 2**200])
    def test_out_of_range(self, value):
        message = rf"max_input_bytes must be between 1 and {MAX_INPUT_BYTES}, got {value}"
        with pytest.raises(ValueError, match=message):
            lint.LintConfig(max_input_bytes=value)
        with pytest.raises(ValueError, match=message):
            lint.LintConfig().with_max_input_bytes(value)

    @pytest.mark.parametrize("value", [True, False, 1.5, "x"])
    def test_wrong_type(self, value):
        with pytest.raises(TypeError):
            lint.LintConfig(max_input_bytes=value)
        with pytest.raises(TypeError):
            lint.LintConfig().with_max_input_bytes(value)

    def test_getter(self):
        assert lint.LintConfig().max_input_bytes == 100 * MIB
        assert lint.LintConfig(max_input_bytes=16).max_input_bytes == 16
        assert lint.LintConfig().with_max_input_bytes(32).max_input_bytes == 32

    def test_bounds_accepted(self):
        lint.LintConfig(max_input_bytes=1)
        lint.LintConfig(max_input_bytes=MAX_INPUT_BYTES)


@pytest.mark.parametrize("load", LOADERS)
def test_flow_nesting_is_capped_at_255_whatever_max_depth_says(load):
    def flow(depth):
        return "[" * depth + "1" + "]" * depth

    assert load(flow(255), max_depth=MAX_DEPTH) is not None
    with pytest.raises(
        ValueError, match="flow collection nesting exceeds the scanner limit of 255"
    ):
        load(flow(256), max_depth=MAX_DEPTH)


MAX_SCAN_AHEAD = 1 << 30
# A root flow sequence is tokenized whole before its first event: the shape the limit bounds.
ROOT_FLOW = "[" + ",".join(["1"] * 200) + "]"
SCAN_AHEAD = "parser lookahead exceeds 64 characters"


@pytest.mark.parametrize("load", LOADERS)
class TestScanAheadLoaders:
    def test_default_accepts_root_flow(self, load):
        assert load(ROOT_FLOW) == [1] * 200

    def test_lowered_limit_rejects_root_flow(self, load):
        with pytest.raises(ValueError, match=SCAN_AHEAD):
            load(ROOT_FLOW, max_scan_ahead=64)

    def test_lowered_limit_accepts_streaming_flow(self, load):
        assert load("a: " + ROOT_FLOW, max_scan_ahead=64) == {"a": [1] * 200}

    def test_raised_limit_accepts_root_flow(self, load):
        assert load(ROOT_FLOW, max_scan_ahead=MIB) == [1] * 200

    @pytest.mark.parametrize("value", [0, -1, MAX_SCAN_AHEAD + 1])
    def test_invalid_value(self, load, value):
        message = rf"max_scan_ahead must be between 1 and {MAX_SCAN_AHEAD}, got {value}"
        with pytest.raises(ValueError, match=message):
            load("a: 1", max_scan_ahead=value)

    @pytest.mark.parametrize("value", [True, "3", 1.5])
    def test_non_integer_is_type_error(self, load, value):
        with pytest.raises(TypeError):
            load("a: 1", max_scan_ahead=value)


class TestScanAheadConfigs:
    def test_parallel(self):
        low = yaml_parallel.ParallelConfig(max_scan_ahead=64)
        with pytest.raises(ValueError, match=SCAN_AHEAD):
            yaml_parallel.parse_parallel(ROOT_FLOW, low)
        high = yaml_parallel.ParallelConfig().with_max_scan_ahead(MIB)
        assert yaml_parallel.parse_parallel(ROOT_FLOW, high)

    def test_lint(self):
        with pytest.raises(ValueError, match=SCAN_AHEAD):
            lint.lint(ROOT_FLOW, lint.LintConfig(max_scan_ahead=64))
        config = lint.LintConfig().with_max_scan_ahead(64).with_max_scan_ahead(None)
        assert isinstance(lint.lint(ROOT_FLOW, config), list)

    def test_batch_process_and_format(self, tmp_path):
        path = tmp_path / "flow.yaml"
        path.write_text(ROOT_FLOW)
        low = batch.BatchConfig(max_scan_ahead=64)
        assert batch.process_files([str(path)], low).failed == 1
        [(_, content, error)] = batch.format_files([str(path)], low)
        assert content is None
        assert error is not None and SCAN_AHEAD in error
        high = batch.BatchConfig().with_max_scan_ahead(MIB)
        assert batch.process_files([str(path)], high).failed == 0
        [(_, content, error)] = batch.format_files([str(path)], high)
        assert error is None
        assert content

    @pytest.mark.parametrize(
        "make",
        [core_parallel.ParallelConfig, lint.LintConfig, batch.BatchConfig],
    )
    @pytest.mark.parametrize("value", [0, -1, MAX_SCAN_AHEAD + 1])
    def test_invalid_value(self, make, value):
        message = rf"max_scan_ahead must be between 1 and {MAX_SCAN_AHEAD}, got {value}"
        with pytest.raises(ValueError, match=message):
            make(max_scan_ahead=value)
        with pytest.raises(ValueError, match=message):
            make().with_max_scan_ahead(value)


class TestInvalidCharacterAfterHugeFlow:
    def test_reports_the_right_document_without_scanning_past_the_limit(self):
        big = "[" + ",".join(["1"] * 400_000) + "]"
        source = f"a\n---\n- {big}\n--- b\n--- c\x01"
        with pytest.raises(ValueError, match=r"U\+0001 is not allowed.*\(document 4\)"):
            list(fast_yaml.safe_load_all(source, max_scan_ahead=64 * 1024))
