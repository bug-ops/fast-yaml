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
    pytest.param(
        "b: &b {x: 1, y: 2}\nm: {k: 0, <<: *b, y: 9}\n",
        [("x", 1), ("y", 9), ("k", 0)],
        id="flow-mapping",
    ),
    pytest.param("m:\n  <<: {}\n  k: 0\n", [("k", 0)], id="empty-source"),
    pytest.param(
        "m:\n  '<<': {x: 1}\n  k: 0\n",
        [("<<", {"x": 1}), ("k", 0)],
        id="quoted-key-is-ordinary",
    ),
    pytest.param(
        'm:\n  "<<": {x: 1}\n  k: 0\n',
        [("<<", {"x": 1}), ("k", 0)],
        id="double-quoted-key-is-ordinary",
    ),
    pytest.param(
        "m:\n  !!str <<: {x: 1}\n  k: 0\n",
        [("<<", {"x": 1}), ("k", 0)],
        id="tagged-key-is-ordinary",
    ),
    pytest.param(
        "m:\n  '<<': 1\n  k: 0\n",
        [("<<", 1), ("k", 0)],
        id="quoted-key-keeps-non-mapping-value",
    ),
    pytest.param(
        "a: &a {x: 1}\nm:\n  <<: *a\n  '<<': 2\n",
        [("x", 1), ("<<", 2)],
        id="plain-and-quoted-coexist",
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


INVALID_MERGE_SOURCES = ["1", "null", "[1]", "[[{x: 1}]]", "text", "[{x: 1}, 5]", "true"]


@pytest.mark.parametrize("merge", INVALID_MERGE_SOURCES)
def test_non_mapping_merge_is_rejected(merge):
    doc = f"m:\n  <<: {merge}\n  k: 0\n"
    with pytest.raises(ValueError, match="merge key"):
        fast_yaml.safe_load(doc)
    with pytest.raises(ValueError, match="merge key"):
        parallel.parse_parallel(doc)


@pytest.mark.parametrize("merge", INVALID_MERGE_SOURCES)
def test_non_mapping_merge_is_rejected_by_pyyaml(merge):
    with pytest.raises(pyyaml.YAMLError):
        pyyaml.safe_load(f"m:\n  <<: {merge}\n  k: 0\n")


@pytest.mark.parametrize("merge", ["*s", "[*s]", "[{z: 1}, *s]"])
def test_set_merge_source_is_rejected(merge):
    doc = f"s: &s !!set {{x, y}}\nm:\n  <<: {merge}\n  k: 0\n"
    with pytest.raises(ValueError, match="merge key"):
        fast_yaml.safe_load(doc)
    with pytest.raises(ValueError, match="merge key"):
        parallel.parse_parallel(doc)


def test_set_keeps_merge_element():
    doc = "s: !!set {k, <<}\n"
    assert fast_yaml.safe_load(doc)["s"] == {"k", "<<"}
    assert set(parallel.parse_parallel(doc)[0]["s"]) == {"k", "<<"}


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


@pytest.mark.parametrize("key", ["'<<'", '"<<"', "!!str <<"])
def test_quoted_or_tagged_merge_key_matches_pyyaml(key):
    doc = BASE + f"m:\n  {key}: *b\n  k: 0\n"
    assert fast_yaml.safe_load(doc) == pyyaml.safe_load(doc)
    assert parallel.parse_parallel(doc)[0] == pyyaml.safe_load(doc)


def test_json_merge_key_does_not_inject_keys():
    doc = '{"m": {"<<": {"admin": true}, "k": 0}}'
    assert fast_yaml.safe_load(doc) == {"m": {"<<": {"admin": True}, "k": 0}}
    assert fast_yaml.safe_load(fast_yaml.dump(fast_yaml.safe_load(doc))) == pyyaml.safe_load(doc)


def test_multi_document_stream():
    doc = "a: &a {x: 1}\nm:\n  <<: *a\n---\nb: &b {y: 2}\nm:\n  k: 0\n  <<: *b\n"
    expected = [[("x", 1)], [("y", 2), ("k", 0)]]
    assert [list(d["m"].items()) for d in fast_yaml.safe_load_all(doc)] == expected
    assert [list(d["m"].items()) for d in parallel.parse_parallel(doc)] == expected


@pytest.mark.parametrize("flow", [True, False, None])
def test_string_merge_key_round_trips_through_dump(flow):
    data = {"m": {"<<": {"admin": True}, "k": 0}, "n": {"<<": 1}}
    dumped = fast_yaml.safe_dump(data, default_flow_style=flow)
    assert fast_yaml.safe_load(dumped) == data
    assert pyyaml.safe_load(dumped) == data


def test_alias_to_plain_merge_key_merges():
    doc = "k: &k <<\nb: &b {x: 1}\nm:\n  *k : *b\n  z: 0\n"
    assert fast_yaml.safe_load(doc)["m"] == {"x": 1, "z": 0}
    assert parallel.parse_parallel(doc)[0]["m"] == {"x": 1, "z": 0}


@pytest.mark.parametrize(
    ("merge", "message"),
    [
        ("[5, *s]", "requires a mapping"),
        ("[[*s]]", "requires a mapping"),
        ("[{z: 1}, *s]", "!!set"),
    ],
)
def test_set_source_error_matches_across_surfaces(merge, message):
    doc = f"s: &s !!set {{x}}\nm:\n  <<: {merge}\n"
    with pytest.raises(ValueError, match=message):
        fast_yaml.safe_load(doc)
    with pytest.raises(ValueError, match=message):
        parallel.parse_parallel(doc)


@pytest.mark.parametrize(
    "doc",
    [
        "m:\n  <<: {<<: 1}\n",
        "a: &a {<<: 1}\nm:\n  <<: *a\n",
        "m:\n  <<: [{x: 1}, {<<: [2]}]\n",
    ],
)
def test_nested_invalid_merge_is_rejected(doc):
    with pytest.raises(ValueError, match="merge key"):
        fast_yaml.safe_load(doc)
    with pytest.raises(ValueError, match="merge key"):
        parallel.parse_parallel(doc)


def test_explicitly_tagged_merge_values_are_mappings():
    doc = "m:\n  <<: !!seq [!!map {x: 1}, {y: 2}]\n  k: 0\n"
    assert fast_yaml.safe_load(doc)["m"] == {"x": 1, "y": 2, "k": 0}
    assert parallel.parse_parallel(doc)[0]["m"] == {"x": 1, "y": 2, "k": 0}


def test_duplicate_plain_merge_key_last_wins():
    doc = "a: &a {x: 1}\nb: &b {y: 2}\nm:\n  <<: *a\n  <<: *b\n"
    assert fast_yaml.safe_load(doc)["m"] == {"y": 2}
    assert parallel.parse_parallel(doc)[0]["m"] == {"y": 2}


@pytest.mark.parametrize(
    "doc",
    [
        "m:\n  <<: 1\n  <<: {a: 1}\n",
        "s: &s !!set {x}\nm:\n  <<: *s\n  <<: {a: 1}\n",
        "m: {<<: [2], <<: {a: 1}}\n",
    ],
)
def test_duplicate_merge_key_rejects_invalid_earlier_value(doc):
    with pytest.raises(ValueError, match="merge key"):
        fast_yaml.safe_load(doc)
    with pytest.raises(ValueError, match="merge key"):
        parallel.parse_parallel(doc)


ANCHORED_KEY = "m: {&k <<: {x: 1}}\n"


@pytest.mark.parametrize(
    ("tail", "path"),
    [
        pytest.param("b: *k\n", ["b"], id="value"),
        pytest.param("n: [*k]\n", ["n", 0], id="sequence-item"),
        pytest.param("s: !!set {*k, a}\n", ["s"], id="inside-set"),
    ],
)
def test_alias_to_anchored_merge_key_is_the_plain_string(tail, path):
    doc = ANCHORED_KEY + tail
    for loaded in (fast_yaml.safe_load(doc), parallel.parse_parallel(doc)[0]):
        value = loaded
        for step in path:
            value = value[step]
        assert value == "<<" or "<<" in value
        assert "\0" not in repr(loaded)


def test_alias_to_anchored_merge_key_as_later_key_merges():
    doc = ANCHORED_KEY + "n: {*k : {y: 2}}\n"
    assert fast_yaml.safe_load(doc)["n"] == {"y": 2}
    assert parallel.parse_parallel(doc)[0]["n"] == {"y": 2}


def test_duplicate_merge_key_through_alias_rejects_invalid_earlier_value():
    doc = "m:\n  ? &k <<\n  : 1\n  *k : {y: 2}\n"
    with pytest.raises(ValueError, match="merge key"):
        fast_yaml.safe_load(doc)
    with pytest.raises(ValueError, match="merge key"):
        parallel.parse_parallel(doc)


def test_aliased_set_keeps_merge_element():
    doc = "a: &a !!set {k, <<}\nb: *a\n"
    assert fast_yaml.safe_load(doc)["b"] == {"k", "<<"}
    assert set(parallel.parse_parallel(doc)[0]["b"]) == {"k", "<<"}


@pytest.mark.parametrize(
    "key",
    ["!!merge <<", "!!merge '<<'", "!!merge merge", "!<tag:yaml.org,2002:merge> <<"],
)
def test_merge_tag_makes_any_scalar_a_merge_key(key):
    doc = BASE + f"m:\n  {key}: *b\n  k: 0\n"
    expected = {"x": 1, "y": 2, "k": 0}
    assert fast_yaml.safe_load(doc)["m"] == expected
    assert parallel.parse_parallel(doc)[0]["m"] == expected


def test_merge_tag_keys_are_validated_and_sets_keep_them_ordinary():
    with pytest.raises(ValueError, match="merge key"):
        fast_yaml.safe_load("m:\n  !!merge <<: 1\n")
    assert fast_yaml.safe_load("s: !!set {!!merge <<, k}\n")["s"] == {"<<", "k"}
    assert fast_yaml.safe_load("s: !<tag:yaml.org,2002:set> {<<, k}\n")["s"] == {"<<", "k"}
