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

    doc = parallel.parse_parallel(f'a: "{BIG_POS}"\nb: \'{BIG_NEG}\'\n')[0]
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

    text = (
        f"a: &x {BIG_POS}\nb: *x\nc: !!str {BIG_POS}\nd: !!int {BIG_POS}\ne: !!float {BIG_POS}\n"
    )
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
