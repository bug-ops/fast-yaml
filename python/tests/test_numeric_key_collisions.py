"""Keys that YAML keeps distinct but a Python dict or set would merge (#489)."""

import math

import pytest

import fast_yaml
from fast_yaml import parallel

COLLIDING = [
    pytest.param("1: a\ntrue: b\n", "bool key true", "int", 2, 1, id="int-then-bool"),
    pytest.param("true: a\n1: b\n", "int key 1", "bool", 2, 1, id="bool-then-int"),
    pytest.param("1: a\n1.0: b\n", "float key 1.0", "int", 2, 1, id="int-then-float"),
    pytest.param("1.0: a\n1: b\n", "int key 1", "float", 2, 1, id="float-then-int"),
    pytest.param("true: a\n1.0: b\n", "float key 1.0", "bool", 2, 1, id="bool-then-float"),
    pytest.param("0: a\n-0.0: b\n", "float key -0.0", "int", 2, 1, id="int-zero-negative-zero"),
    pytest.param("1: a\n'1': d\ntrue: b\n", "bool key true", "int", 3, 1, id="str-key-between"),
    pytest.param("m: {1: a, x: 0, true: b}\n", "bool key true", "int", 1, 17, id="nested-flow"),
]


@pytest.mark.parametrize(("doc", "incoming", "kept", "line", "column"), COLLIDING)
def test_cross_kind_collision_is_rejected(doc, incoming, kept, line, column):
    message = (
        f"{incoming} is distinct in YAML but equal as a Python dict key to a key of type {kept}"
        f".*at line {line}, column {column}"
    )
    with pytest.raises(ValueError, match=message):
        fast_yaml.safe_load(doc)


@pytest.mark.parametrize("doc", [c.values[0] for c in COLLIDING])
def test_parallel_path_rejects_cross_kind_collision_without_position(doc):
    with pytest.raises(ValueError, match="is distinct in YAML") as excinfo:
        parallel.parse_parallel(doc)
    assert "at line" not in str(excinfo.value)


@pytest.mark.parametrize("newline", ["\n", "\r\n"])
def test_collision_position_in_second_document(newline):
    doc = newline.join(["a: 1", "---", "1: a", "true: b", ""])
    with pytest.raises(ValueError, match=r"bool key true.*at line 4, column 1"):
        fast_yaml.safe_load_all(doc)


def test_collision_between_merge_source_and_explicit_key():
    doc = "b: &b {1: one}\nc:\n  <<: *b\n  true: z\n"
    with pytest.raises(ValueError, match=r"bool key true.*at line 4, column 3"):
        fast_yaml.safe_load(doc)


def test_collision_between_merged_sources_points_at_merge_key():
    doc = "a: &a {1: one}\nb: &b {true: t}\nc:\n  <<: [*a, *b]\n"
    with pytest.raises(ValueError, match=r"bool key true.*at line 4, column 3"):
        fast_yaml.safe_load(doc)


def test_collision_with_explicit_key_added_after_merge():
    doc = "b: &x {true: b}\nc: {<<: *x, 1: z}\n"
    with pytest.raises(ValueError, match=r"int key 1.*at line 2, column 13"):
        fast_yaml.safe_load(doc)


@pytest.mark.parametrize(
    "doc",
    [
        "!!set {1, true}",
        "!!set {1.0, 1}",
    ],
)
def test_set_collision_is_rejected(doc):
    with pytest.raises(ValueError, match="is distinct in YAML"):
        fast_yaml.safe_load(doc)


def test_set_collision_through_parse_parallel():
    with pytest.raises(ValueError, match="bool key true"):
        parallel.parse_parallel("!!set {1, true}")


def test_set_collision_reports_position():
    with pytest.raises(ValueError, match=r"bool key true.*at line 3, column 5"):
        fast_yaml.safe_load("s: !!set\n  ? 1\n  ? true\n")


@pytest.mark.parametrize(
    ("doc", "incoming"),
    [
        pytest.param(
            "18446744073709551616: a\n18446744073709551616.0: b\n", "float key", id="big-int-float"
        ),
        pytest.param("0x1: a\ntrue: b\n", "bool key true", id="hex-int-bool"),
        pytest.param("!!int 1: a\ntrue: b\n", "bool key true", id="tagged-int-bool"),
        pytest.param("!!int 1: a\n!!float 1: b\n", "float key 1.0", id="tagged-int-float"),
    ],
)
def test_resolved_scalar_spellings_collide(doc, incoming):
    with pytest.raises(ValueError, match=incoming):
        fast_yaml.safe_load(doc)


def test_float_beyond_2_pow_53_does_not_collide_with_nearby_int():
    loaded = fast_yaml.safe_load("9007199254740993: a\n9007199254740992.0: b\n")
    assert len(loaded) == 2


@pytest.mark.parametrize(
    ("doc", "expected"),
    [
        ("1: a\n1: b\n", {1: "b"}),
        ("true: a\ntrue: b\n", {True: "b"}),
        ("1.0: a\n1.0: b\n", {1.0: "b"}),
        ("0.0: a\n-0.0: b\n", {0.0: "b"}),
        ("1: a\n'1': d\n", {1: "a", "1": "d"}),
        ("1: a\nnull: b\n~: c\n", {1: "a", None: "c"}),
        ("1: a\n2.5: b\n", {1: "a", 2.5: "b"}),
        ("1: true\nk: 1\n", {1: True, "k": 1}),
    ],
)
def test_non_colliding_keys_are_unchanged(doc, expected):
    assert fast_yaml.safe_load(doc) == expected


def test_same_kind_duplicate_keeps_first_position_and_last_value():
    loaded = fast_yaml.safe_load("1: a\nx: 0\n1: b\n")
    assert list(loaded.items()) == [(1, "b"), ("x", 0)]


def test_nan_keys_collapse_to_one_entry():
    loaded = fast_yaml.safe_load(".nan: a\n.nan: b\n")
    ((key, value),) = loaded.items()
    assert math.isnan(key)
    assert value == "b"


def test_nan_key_is_overridden_after_merge():
    loaded = fast_yaml.safe_load("b: &x {.nan: b}\nc: {<<: *x, .nan: z}\n")
    ((key, value),) = loaded["c"].items()
    assert math.isnan(key)
    assert value == "z"


def test_nan_keys_collapse_across_spellings():
    assert len(fast_yaml.safe_load(".nan: a\n.NaN: b\n.NAN: c\n")) == 1


def test_nan_members_collapse_in_set():
    assert len(fast_yaml.safe_load("!!set {.nan, .NaN}")) == 1


def test_nan_key_does_not_collide_with_int_key():
    assert len(fast_yaml.safe_load(".nan: a\n1: b\n")) == 2


def test_nan_is_not_shared_across_loads():
    first = next(iter(fast_yaml.safe_load(".nan: a")))
    second = next(iter(fast_yaml.safe_load(".nan: a")))
    assert first is not second
