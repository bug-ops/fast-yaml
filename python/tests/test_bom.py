"""Regression tests for UTF-8 BOM handling (#310)."""

import fast_yaml

BOM = "\ufeff"


def test_safe_load_bom_before_comment():
    assert fast_yaml.safe_load(f"{BOM}# c\na: 1") == {"a": 1}


def test_safe_load_bom_key_has_no_bom():
    assert fast_yaml.safe_load(f"{BOM}a: 1") == {"a": 1}


def test_safe_load_all_bom_multi_document():
    assert list(fast_yaml.safe_load_all(f"{BOM}---\na: 1\n---\nb: 2")) == [
        {"a": 1},
        {"b": 2},
    ]


def test_safe_load_bom_only_is_none():
    assert fast_yaml.safe_load(BOM) is None


def test_safe_load_mid_text_bom_is_data():
    assert fast_yaml.safe_load(f"b: {BOM}x") == {"b": f"{BOM}x"}
