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
