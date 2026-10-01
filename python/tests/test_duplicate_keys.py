"""Duplicate mapping keys keep the first position and the last value (#522)."""

import json
import subprocess

import pytest

import fast_yaml
from _fy_support import fy_binary

MAPPINGS = [
    pytest.param(
        "a: 1\nb: 2\na: 3\nc: 4\na: 5\n",
        {"a": 5, "b": 2, "c": 4},
        id="literal-3-occurrences",
    ),
    pytest.param("o:\n  x: 1\n  y: 2\n  x: 3\n", {"o": {"x": 3, "y": 2}}, id="nested-block"),
    pytest.param("{a: 1, b: 2, a: 3}\n", {"a": 3, "b": 2}, id="flow"),
    pytest.param(
        "o: {x: 1, y: 2, x: 3}\nz: 0\n",
        {"o": {"x": 3, "y": 2}, "z": 0},
        id="nested-flow",
    ),
    pytest.param("&k a: 1\nb: 2\n*k : 3\n", {"a": 3, "b": 2}, id="anchored-alias-key"),
    pytest.param("!!str a: 1\nb: 2\n!!str a: 3\n", {"a": 3, "b": 2}, id="tagged"),
    pytest.param("a: 1\nb: 2\na: [x]\n", {"a": ["x"], "b": 2}, id="value-kind-changes"),
]


@pytest.mark.parametrize(("doc", "expected"), MAPPINGS)
@pytest.mark.parametrize("loader", [fast_yaml.safe_load, fast_yaml.load])
def test_first_position_last_value(loader, doc, expected):
    result = loader(doc)
    assert result == expected
    assert list(result) == list(expected)


def test_set_collapses_duplicates():
    assert fast_yaml.safe_load("!!set {a, b, a}\n") == {"a", "b"}


def test_omap_keeps_every_pair_in_order():
    assert fast_yaml.safe_load("!!omap [a: 1, b: 2, a: 3]\n") == [{"a": 1}, {"b": 2}, {"a": 3}]


@pytest.mark.parametrize(("doc", "expected"), MAPPINGS)
def test_matches_cli_key_order(doc, expected):
    result = subprocess.run(
        [fy_binary(), "convert", "json", "--pretty", "false"],
        input=doc,
        capture_output=True,
        text=True,
        check=True,
    )
    cli = json.loads(result.stdout)
    assert cli == expected
    assert list(cli) == list(fast_yaml.safe_load(doc))
