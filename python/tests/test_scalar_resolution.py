import math

import pytest

import fast_yaml

CASES = [
    ('!!int "7"', 7),
    ("!!int '7'", 7),
    ('!!float "1.5"', 1.5),
    ('!!bool "true"', True),
    ('!!bool "True"', True),
    ("!!bool FALSE", False),
    ('!!null "null"', None),
    ("!!null Null", None),
    ('!!null ""', None),
    ('!!str "7"', "7"),
    ("!!str 7", "7"),
    ("!!int 3.0", 3),
    ("!!int -2.7", -2),
    ("!!int 1.0e2", 100),
    ("!!int 99999999999999999999", 99999999999999999999),
    ("!!int true", "true"),
    ("!!null 1", "1"),
    ("!!binary 12", "12"),
    ("!!float 0x1F", "0x1F"),
    ("! 42", "42"),
    ('! "x"', "x"),
    ("!foo 42", 42),
    ("~", None),
    ("Null", None),
    ("True", True),
    ("FALSE", False),
    ("0o17", 15),
    ("0x1F", 31),
    ("+42", 42),
    ("007", 7),
    ("1.5e10", 1.5e10),
    ("-.inf", -math.inf),
    ("+.inf", math.inf),
    ("+.INF", math.inf),
    (".5", 0.5),
    ("-.5", -0.5),
    ("+.5e1", 5.0),
    ("+.nan", "+.nan"),
    (".e5", ".e5"),
    ("99999999999999999999", 99999999999999999999),
    ("0xFFFFFFFFFFFFFFFFFF", "0xFFFFFFFFFFFFFFFFFF"),
    ('"true"', "true"),
    ("'7'", "7"),
    ("9223372036854775807", 2**63 - 1),
    ("9223372036854775808", 2**63),
    ("-9223372036854775808", -(2**63)),
    ("-9223372036854775809", -(2**63) - 1),
    ("+9223372036854775808", 2**63),
    ("!!int 9223372036854775807", 2**63 - 1),
    ("!!int 9223372036854775808", 2**63),
    ('!!int "9223372036854775808"', 2**63),
    ("!!int -9223372036854775808", -(2**63)),
    ("!!int -9223372036854775809", -(2**63) - 1),
    ("!!int -99999999999999999999", -99999999999999999999),
    ("!!int 9223372036854776000", 9223372036854776000),
    ('"9223372036854775808"', "9223372036854775808"),
    ("!!int 0xFFFFFFFFFFFFFFFFFF", "0xFFFFFFFFFFFFFFFFFF"),
    ("+-5", "+-5"),
    ("--5", "--5"),
    ("0x-1", "0x-1"),
    ("!!int +-5", "+-5"),
    ("!!int", ""),
    ("!!bool", ""),
    ("!!float", ""),
    ("!!null", None),
    ("!", ""),
    ("!!foo 7", "7"),
    ("!!seq 7", "7"),
    ("!!timestamp 5", "5"),
    ('!foo "42"', "42"),
    ('!!bool "FALSE"', False),
    ("|\n    7", "7\n"),
    ("!!int |\n    7", "7\n"),
    ("!!int >\n    7", "7\n"),
]


@pytest.mark.parametrize(("doc", "expected"), CASES)
def test_scalar_resolution(doc, expected):
    result = fast_yaml.safe_load(f"v: {doc}")["v"]
    assert result == expected
    assert type(result) is type(expected)


def test_tagged_scalars_in_collections_anchors_and_documents():
    assert fast_yaml.safe_load('[!!int "1", !!bool "true"]') == [1, True]
    assert fast_yaml.safe_load('{a: !!float "2.5", !!int "3": x}') == {"a": 2.5, 3: "x"}
    assert fast_yaml.safe_load('a: &x !!int "7"\nb: *x') == {"a": 7, "b": 7}
    assert list(fast_yaml.safe_load_all('--- !!int "1"\n--- !!bool "false"\n')) == [1, False]


def test_nan_resolution():
    assert math.isnan(fast_yaml.safe_load("v: .nan")["v"])
    assert math.isnan(fast_yaml.safe_load('v: !!float ".nan"')["v"])


@pytest.mark.parametrize("text", ["+.inf", "+.Inf", "+.INF", ".5", "-.5"])
def test_dump_keeps_float_lookalike_strings(text):
    for data in ({"k": text}, {text: 1}, [text]):
        assert fast_yaml.safe_load(fast_yaml.safe_dump(data)) == data
        assert fast_yaml.safe_load(fast_yaml.safe_dump(data, default_flow_style=True)) == data
