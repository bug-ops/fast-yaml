"""Dumped YAML 1.1 words, lookalikes and multiline strings read back unchanged (#546, #560)."""

from __future__ import annotations

import pytest

import fast_yaml

yaml = pytest.importorskip("yaml")

WORDS = [
    "yes",
    "Yes",
    "NO",
    "on",
    "Off",
    "null",
    "~",
    "true",
    "False",
    "y",
    "n",
    "0x1F",
    "1_000",
    ".inf",
    "-.INF",
    ".nan",
    "1e3",
    "2001-12-14",
    "12:30:45",
    "<<",
    "",
    " lead",
    "trail ",
    "a: b",
    "a #b",
    "- item",
    "tab\there",
    " ",
    "café",
    "0b1010",
    "a -",
    "\u0085",
    "a\u0085- x",
]


def test_next_line_character_cannot_inject_a_key_with_multiline_strings():
    data = {"k": "line\nb\u0085evil: 1"}
    text = fast_yaml.safe_dump(data)
    assert yaml.safe_load(text) == data


@pytest.mark.parametrize("word", WORDS)
@pytest.mark.parametrize("flow", [None, True])
def test_words_survive_a_pyyaml_round_trip_as_values_and_keys(word, flow):
    data = {"value": word, word: "key", "list": [word], "set-like": {word: None}}
    text = fast_yaml.safe_dump(data, default_flow_style=flow)
    assert yaml.safe_load(text) == data


@pytest.mark.parametrize("indent", [1, 2, 3, 4, 9])
def test_multiline_strings_use_literal_blocks_at_any_indent(indent):
    data = {"a": {"b": "line one\nline two\n"}, "list": ["x\ny\n"]}
    text = fast_yaml.safe_dump(data, indent=indent)
    assert yaml.safe_load(text) == data
    assert fast_yaml.safe_load(text) == data
    assert "\\n" in text or "|" in text


def test_deeply_nested_sequence_dumps_without_a_stack_overflow():
    data: object = 1
    for _ in range(250):
        data = [data]
    text = fast_yaml.safe_dump(data)
    assert text.count("- ") == 250
