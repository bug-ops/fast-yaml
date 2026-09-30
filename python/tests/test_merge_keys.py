"""Merge key (<<) order and precedence, identical across safe_load and parse_parallel."""

import pytest
import yaml as pyyaml

import fast_yaml
from fast_yaml import parallel

BASE = "b: &b {x: 1, y: 2}\n"

ORDERED_CASES = [
    pytest.param(
        BASE + "m:\n  k: 0\n  <<: *b\n  y: 9\n",
        [("x", 1), ("y", 9), ("k", 0)],
        id="explicit-before-merge",
    ),
    pytest.param(
        BASE + "m:\n  <<: *b\n  k: 0\n  y: 9\n",
        [("x", 1), ("y", 9), ("k", 0)],
        id="merge-first",
    ),
    pytest.param(
        "a: &a {x: 1, p: A}\nb: &b {y: 2, p: B}\nm:\n  <<: [*a, *b]\n  k: 0\n",
        [("x", 1), ("p", "A"), ("y", 2), ("k", 0)],
        id="sequence-earlier-wins-forward-order",
    ),
    pytest.param(
        "a: &a {x: 1}\nb: &b\n  <<: *a\n  y: 2\nm:\n  <<: *b\n  z: 3\n",
        [("x", 1), ("y", 2), ("z", 3)],
        id="nested-anchor",
    ),
    pytest.param("m:\n  <<: {x: 1}\n  y: 2\n", [("x", 1), ("y", 2)], id="inline-map"),
    pytest.param("m:\n  k: 0\n  <<: {k: 1}\n", [("k", 0)], id="explicit-wins"),
    pytest.param(
        "a: &a {x: 1}\nb: &b {y: 2}\nm:\n  <<: *a\n  <<: *b\n",
        [("y", 2)],
        id="repeated-merge-last-wins",
    ),
    pytest.param("b: &b {x: 1, y: 2}\nm: {k: 0, <<: *b, y: 9}\n", [("x", 1), ("y", 9), ("k", 0)], id="flow-mapping"),
    pytest.param("m:\n  <<: {}\n  k: 0\n", [("k", 0)], id="empty-source"),
    pytest.param(
        "a: &a {x: 1}\nm:\n  <<: [*a, 5, null, [{w: 0}], {z: 3}]\n  k: 0\n",
        [("x", 1), ("z", 3), ("k", 0)],
        id="mixed-sequence",
    ),
    pytest.param(
        BASE + "m:\n  <<: *b\n  n: {p: 1}\n",
        [("x", 1), ("y", 2), ("n", {"p": 1})],
        id="shallow",
    ),
]


@pytest.mark.parametrize(("doc", "expected"), ORDERED_CASES)
def test_safe_load_order(doc, expected):
    assert list(fast_yaml.safe_load(doc)["m"].items()) == expected


@pytest.mark.parametrize(("doc", "expected"), ORDERED_CASES)
def test_parse_parallel_order(doc, expected):
    assert list(parallel.parse_parallel(doc)[0]["m"].items()) == expected


@pytest.mark.parametrize("merge", ["1", "null", "[1]", "[[{x: 1}]]", "text"])
def test_non_mapping_merge_ignored(merge):
    doc = f"m:\n  <<: {merge}\n  k: 0\n"
    assert fast_yaml.safe_load(doc)["m"] == {"k": 0}
    assert parallel.parse_parallel(doc)[0]["m"] == {"k": 0}


def test_set_merge_source_contributes_null_values():
    # Dict equality only: set iteration order is hash-dependent
    doc = "s: &s !!set {x, y}\nm:\n  <<: *s\n  k: 0\n"
    assert fast_yaml.safe_load(doc)["m"] == {"x": None, "y": None, "k": 0}


@pytest.mark.parametrize(
    "doc",
    [
        BASE + "m:\n  k: 0\n  <<: *b\n  y: 9\n",
        BASE + "m:\n  <<: *b\n  k: 0\n",
        "a: &a {x: 1}\nb: &b {y: 2}\nm:\n  <<: [*a, *b]\n  k: 0\n",
        "a: &a {x: 1}\nb: &b\n  <<: *a\n  y: 2\nm:\n  <<: *b\n  z: 3\n",
    ],
)
def test_matches_pyyaml_values(doc):
    assert fast_yaml.safe_load(doc) == pyyaml.safe_load(doc)


def test_matches_pyyaml_key_order_for_single_source():
    doc = BASE + "m:\n  k: 0\n  <<: *b\n  y: 9\n"
    assert list(fast_yaml.safe_load(doc)["m"].items()) == list(pyyaml.safe_load(doc)["m"].items())


def test_quoted_merge_key_is_merged_today():
    # Known divergence from PyYAML, tracked by a follow-up issue
    doc = BASE + "m:\n  '<<': *b\n  k: 0\n"
    assert list(fast_yaml.safe_load(doc)["m"].items()) == [("x", 1), ("y", 2), ("k", 0)]
    assert list(parallel.parse_parallel(doc)[0]["m"].items()) == [("x", 1), ("y", 2), ("k", 0)]


def test_set_merge_source_through_parse_parallel():
    doc = "s: &s !!set {x, y}\nm:\n  <<: *s\n  k: 0\n"
    assert dict(parallel.parse_parallel(doc)[0]["m"]) == {"x": None, "y": None, "k": 0}


def test_multi_document_stream():
    doc = "a: &a {x: 1}\nm:\n  <<: *a\n---\nb: &b {y: 2}\nm:\n  k: 0\n  <<: *b\n"
    expected = [[("x", 1)], [("y", 2), ("k", 0)]]
    assert [list(d["m"].items()) for d in fast_yaml.safe_load_all(doc)] == expected
    assert [list(d["m"].items()) for d in parallel.parse_parallel(doc)] == expected
