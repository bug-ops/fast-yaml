"""Hostile `rules` input must raise, never crash the process (#324 security audit)."""

from __future__ import annotations

from collections.abc import Mapping
from types import MappingProxyType

import pytest

from fast_yaml._core import lint


def test_deeply_nested_input():
    inner: list = []
    for _ in range(3000):
        inner = [inner]
    nested = {"truthy": {"allowed-values": inner}}
    with pytest.raises(ValueError, match="nested deeper"):
        lint.LintConfig(rules=nested)


def test_cyclic_dict():
    cycle: dict = {}
    cycle["max"] = cycle
    with pytest.raises(ValueError, match="nested deeper"):
        lint.LintConfig(rules={"line-length": cycle})


def test_cyclic_list():
    cycle: list = []
    cycle.append(cycle)
    with pytest.raises(ValueError, match="nested deeper"):
        lint.LintConfig().with_rule_config("truthy", options={"allowed-values": cycle})


def test_shared_reference_bomb_under_unknown_rule():
    value: object = "x"
    for _ in range(24):
        value = [value, value]
    with pytest.raises(ValueError, match="unknown rule 'no-such-rule'"):
        lint.LintConfig(rules={"no-such-rule": value})


def test_shared_reference_bomb_node_budget():
    value: object = "x"
    for _ in range(6):
        value = [value] * 12
    with pytest.raises(ValueError, match="more than"):
        lint.LintConfig(rules={"truthy": {"allowed-values": value}})


def test_non_string_key():
    with pytest.raises(ValueError, match="keys must be strings"):
        lint.LintConfig(rules={1: "error"})
    with pytest.raises(ValueError, match="keys must be strings"):
        lint.LintConfig(rules={"line-length": {1: 2}})


def test_unsupported_value_type():
    with pytest.raises(ValueError, match="unsupported value of type 'bytes'"):
        lint.LintConfig(rules={"line-length": {"max": b"10"}})


def test_integer_out_of_range():
    with pytest.raises(ValueError, match="out of range"):
        lint.LintConfig(rules={"line-length": {"max": 2**100}})


def test_disabled_rules_string_is_type_error():
    with pytest.raises(TypeError, match="sequence of rule codes"):
        lint.LintConfig(disabled_rules="braces")


def test_mapping_proxy_and_nested_mapping_are_accepted():
    config = lint.LintConfig(rules=MappingProxyType({"line-length": MappingProxyType({"max": 42})}))
    assert config.max_line_length == 42
    config = lint.LintConfig().with_rule_config("line-length", options=MappingProxyType({"max": 7}))
    assert config.max_line_length == 7


def test_custom_mapping_is_accepted():
    class Rules(Mapping):
        def __getitem__(self, key):
            return {"max": 9}

        def __iter__(self):
            return iter(["line-length"])

        def __len__(self):
            return 1

    assert lint.LintConfig(rules=Rules()).max_line_length == 9


def test_mapping_with_non_string_key():
    with pytest.raises(ValueError, match="keys must be strings"):
        lint.LintConfig(rules=MappingProxyType({1: "error"}))


def test_cyclic_mapping():
    class Cyclic(Mapping):
        def __getitem__(self, key):
            return self

        def __iter__(self):
            return iter(["max"])

        def __len__(self):
            return 1

    with pytest.raises(ValueError, match="nested deeper"):
        lint.LintConfig(rules={"line-length": Cyclic()})


def test_tuple_and_list_are_accepted():
    lint.LintConfig(rules={"truthy": {"allowed-values": ("true", "false")}})
    lint.LintConfig(rules={"truthy": {"allowed-values": ["true", "false"]}})
