import pytest

import fast_yaml


def test_large_integer_type():
    result = fast_yaml.safe_load(
        "x: 99999999999999999999999999999999999999999999999999999999999999999999999999999999"
    )
    assert isinstance(result["x"], int)


def test_large_integer_value():
    big = 99999999999999999999999999999999999999999999999999999999999999999999999999999999
    result = fast_yaml.safe_load(f"x: {big}")
    assert result["x"] == big


def test_normal_integer_unaffected():
    result = fast_yaml.safe_load("x: 42")
    assert result["x"] == 42
    assert isinstance(result["x"], int)


def test_negative_large_integer():
    result = fast_yaml.safe_load("x: -99999999999999999999999999999999")
    assert isinstance(result["x"], int)
    assert result["x"] == -99999999999999999999999999999999


def test_float_unaffected():
    result = fast_yaml.safe_load("x: 1.5e10")
    assert isinstance(result["x"], float)


BIG_POS = 9223372036854775808
BIG_NEG = -99999999999999999999


def test_parse_parallel_large_integers_are_int():
    from fast_yaml._core import parallel

    doc = parallel.parse_parallel(f"a: {BIG_POS}\nb: {BIG_NEG}\nc: +{BIG_POS}\n")[0]
    assert doc == {"a": BIG_POS, "b": BIG_NEG, "c": BIG_POS}
    assert all(isinstance(v, int) for v in doc.values())


def test_parse_parallel_quoted_large_integer_stays_str():
    from fast_yaml._core import parallel

    doc = parallel.parse_parallel(f"a: \"{BIG_POS}\"\nb: '{BIG_NEG}'\n")[0]
    assert doc == {"a": str(BIG_POS), "b": str(BIG_NEG)}


@pytest.mark.parametrize(
    "literal",
    [
        "9223372036854775807",
        "9223372036854775808",
        "-9223372036854775808",
        "-9223372036854775809",
    ],
)
def test_parse_parallel_matches_safe_load_at_i64_boundaries(literal):
    from fast_yaml._core import parallel

    text = f"v: {literal}\nk:\n  {literal}: x\nl: [{literal}]\n"
    result = parallel.parse_parallel(text)[0]
    assert result == fast_yaml.safe_load(text)
    assert isinstance(result["v"], int)
    assert result["v"] == int(literal)


def test_parse_parallel_large_integer_anchor_and_tags():
    from fast_yaml._core import parallel

    text = f"a: &x {BIG_POS}\nb: *x\nc: !!str {BIG_POS}\nd: !!int {BIG_POS}\ne: !!float {BIG_POS}\n"
    result = parallel.parse_parallel(text)[0]
    assert result == fast_yaml.safe_load(text)
    assert result["b"] == BIG_POS
    assert result["c"] == str(BIG_POS)
    assert isinstance(result["d"], int)


def test_parse_parallel_matches_safe_load_on_large_integers():
    from fast_yaml._core import parallel

    text = (
        f"---\nk: {BIG_POS}\nlist: [{BIG_NEG}, 1, '{BIG_POS}']\n{BIG_POS}: key\n"
        f"---\nnested:\n  x: {BIG_NEG}\n"
    )
    assert parallel.parse_parallel(text) == list(fast_yaml.safe_load_all(text))


class _IntSub(int):
    def __str__(self):
        return "custom"


@pytest.mark.parametrize("big", [1 << 63, (1 << 64) - 1, 1 << 70, -(1 << 63) - 1, -(10**30)])
def test_safe_dump_big_int_value(big):
    out = fast_yaml.safe_dump({"a": big})
    assert out.strip() == f"a: {big}"
    assert fast_yaml.safe_load(out) == {"a": big}


def test_safe_dump_i64_bounds_stay_integers():
    out = fast_yaml.safe_dump([(1 << 63) - 1, -(1 << 63)])
    assert out == f"- {(1 << 63) - 1}\n- {-(1 << 63)}\n"


def test_safe_dump_big_int_key():
    out = fast_yaml.safe_dump({1 << 70: 1, -(1 << 70): 2})
    assert fast_yaml.safe_load(out) == {1 << 70: 1, -(1 << 70): 2}


def test_safe_dump_big_int_in_user_dict():
    from collections import UserDict

    out = fast_yaml.safe_dump(UserDict({"a": 1 << 70, 1 << 65: [1 << 66]}))
    assert fast_yaml.safe_load(out) == {"a": 1 << 70, 1 << 65: [1 << 66]}


def test_safe_dump_int_subclass_uses_digits():
    assert fast_yaml.safe_dump(_IntSub(1 << 70)).strip() == str(1 << 70)


def test_safe_dump_bool_unaffected_by_big_int_path():
    assert fast_yaml.safe_dump([True, 1 << 70]) == f"- true\n- {1 << 70}\n"


def test_dump_all_big_int():
    out = fast_yaml.dump_all([{"a": 1 << 70}, [-(1 << 70)]])
    assert list(fast_yaml.safe_load_all(out)) == [{"a": 1 << 70}, [-(1 << 70)]]


def test_dump_parallel_big_int():
    from fast_yaml._core import parallel

    out = parallel.dump_parallel([{"a": 1 << 70}, {"b": -(1 << 70)}])
    assert list(fast_yaml.safe_load_all(out)) == [{"a": 1 << 70}, {"b": -(1 << 70)}]


def test_round_trip_parse_parallel_then_safe_dump():
    from fast_yaml._core import parallel

    text = "a: 9223372036854775808\nb: -99999999999999999999\n"
    assert fast_yaml.safe_dump(parallel.parse_parallel(text)[0]) == text


def _hostile_int(**overrides):
    return type("Hostile", (int,), overrides)(10**30)


@pytest.mark.parametrize(
    "overrides",
    [
        {"__format__": lambda self, spec: "1\ninjected: true"},
        {"__format__": lambda self, spec: "bad"},
        {"__str__": lambda self: "1\ninjected: true"},
        {"__repr__": lambda self: "1\ninjected: true"},
    ],
    ids=["format-injection", "format-text", "str", "repr"],
)
def test_safe_dump_int_subclass_overrides_cannot_inject(overrides):
    out = fast_yaml.safe_dump({"a": _hostile_int(**overrides)})
    assert out == f"a: {10**30}\n"
    assert fast_yaml.safe_load(out) == {"a": 10**30}


def test_safe_dump_int_enum_big_value():
    import enum

    class Big(enum.IntEnum):
        X = 1 << 70

    assert fast_yaml.safe_dump([Big.X]) == f"- {1 << 70}\n"


def test_safe_dump_int_beyond_str_digits_limit_raises_value_error():
    import sys

    limit = sys.get_int_max_str_digits()
    if limit == 0:
        pytest.skip("int max str digits disabled")
    with pytest.raises(ValueError):
        fast_yaml.safe_dump(10 ** (limit + 1))


def test_dump_budget_counts_big_int_text():
    big = 10**4000
    with pytest.raises(ValueError, match="output size exceeds"):
        fast_yaml.safe_dump([big] * 30000)


RADIX_LITERALS = [
    ("0xFFFFFFFFFFFFFFFFFF", 0xFFFFFFFFFFFFFFFFFF),
    ("+0XFF0000000000000000", 0xFF0000000000000000),
    ("-0xFFFFFFFFFFFFFFFFFF", -0xFFFFFFFFFFFFFFFFFF),
    ("0o7777777777777777777777", 0o7777777777777777777777),
    ("-0O1000000000000000000001", -0o1000000000000000000001),
    ("0x8000000000000000", 2**63),
    ("-0x8000000000000000", -(2**63)),
]


@pytest.mark.parametrize(("literal", "expected"), RADIX_LITERALS)
def test_hex_and_octal_overflow_are_int_in_both_loaders(literal, expected):
    from fast_yaml._core import parallel

    text = f"v: {literal}\nl: [{literal}]\n"
    loaded = fast_yaml.safe_load(text)
    assert loaded == {"v": expected, "l": [expected]}
    assert isinstance(loaded["v"], int)
    assert parallel.parse_parallel(text)[0] == loaded


def test_equal_big_integer_keys_collapse_in_both_loaders():
    from fast_yaml._core import parallel

    text = "+99999999999999999999: a\n99999999999999999999: b\n0x56BC75E2D630FFFFF: c\n"
    expected = {99999999999999999999: "c"}
    assert fast_yaml.safe_load(text) == expected
    assert parallel.parse_parallel(text)[0] == expected


# Largest accepted literals: 14284 significant bits, whose decimal form has 4300 digits.
MAX_LEN_LITERALS = [
    "0x" + "F" * 3571,
    "-0x" + "F" * 3571,
    "0o1" + "7" * 4761,
    "0o" + "0" * 50 + "1" + "7" * 4761,
]
OVER_CAP_LITERALS = ["0x1" + "0" * 3571, "0x" + "F" * 3572, "0o2" + "0" * 4761, "0o" + "7" * 4762]


@pytest.mark.parametrize("literal", MAX_LEN_LITERALS)
def test_max_length_radix_int_loads_in_both_loaders(literal):
    from fast_yaml._core import parallel

    text = f"v: {literal}\nl: [{literal}]\n"
    loaded = fast_yaml.safe_load(text)
    base = 16 if "x" in literal else 8
    assert loaded["v"] == int(literal, base)
    assert isinstance(loaded["v"], int)
    assert parallel.parse_parallel(text)[0] == loaded


@pytest.mark.parametrize("literal", OVER_CAP_LITERALS)
def test_radix_beyond_bit_cap_stays_str_in_both_loaders(literal):
    from fast_yaml._core import parallel

    text = f"v: {literal}\n"
    assert fast_yaml.safe_load(text) == {"v": literal}
    assert parallel.parse_parallel(text)[0] == {"v": literal}


def test_quoted_hex_overflow_stays_str():
    assert fast_yaml.safe_load('v: "0xFFFFFFFFFFFFFFFFFF"') == {"v": "0xFFFFFFFFFFFFFFFFFF"}
