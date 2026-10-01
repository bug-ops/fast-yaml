"""Regression tests for UTF-8 BOM handling (#310)."""

import pytest

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


def test_bom_in_a_later_document_prefix_is_not_data():
    doc = f"a: 1\n...\n{BOM}b: 2\n...\n{BOM}%YAML 1.2\n---\nc: 3\n"
    expected = [{"a": 1}, {"b": 2}, {"c": 3}]
    assert list(fast_yaml.safe_load_all(doc)) == expected
    assert fast_yaml.parallel.parse_parallel(doc) == expected


def test_bom_before_a_marker_line_is_not_data():
    doc = f"a: 1\n{BOM}---\nb: 2\n"
    assert list(fast_yaml.safe_load_all(doc)) == [{"a": 1}, {"b": 2}]


def test_non_printable_characters_are_rejected():
    for char in ("\x00", "\x7f", "\x86", "\ufffe", "\uffff"):
        with pytest.raises(ValueError, match="not allowed in YAML"):
            fast_yaml.safe_load(f"a: x{char}y\n")
        with pytest.raises(ValueError, match="not allowed in YAML"):
            fast_yaml.parallel.parse_parallel(f"a: x{char}y\n")


def _write(tmp_path, name, data):
    path = tmp_path / name
    path.write_bytes(data)
    return str(path)


def test_format_files_keeps_a_leading_bom(tmp_path):
    from fast_yaml._core import batch

    path = _write(tmp_path, "bom.yaml", f"{BOM}# c\na:   1\n".encode())
    [(_, content, error)] = batch.format_files([path])
    assert error is None
    assert content == f"{BOM}a: 1\n"


def test_format_files_in_place_leaves_a_formatted_bom_file_unchanged(tmp_path):
    from fast_yaml._core import batch

    data = f"{BOM}a: 1\n".encode()
    path = _write(tmp_path, "bom.yaml", data)
    result = batch.format_files_in_place([path])
    assert result.changed == 0
    assert (tmp_path / "bom.yaml").read_bytes() == data


@pytest.mark.parametrize("encoding", ["utf-16-le", "utf-16-be", "utf-32-le", "utf-32-be"])
def test_bom_less_utf16_and_utf32_are_reported_as_unsupported(tmp_path, encoding):
    from fast_yaml._core import batch

    path = _write(tmp_path, "wide.yaml", "a: 1\n".encode(encoding))
    result = batch.process_files([path])
    assert result.failed == 1
    [(_, message)] = result.errors()
    assert "unsupported encoding" in message.lower()
    [(_, content, error)] = batch.format_files([path])
    assert content is None
    assert "unsupported encoding" in error.lower()


@pytest.mark.parametrize(
    ("text", "marker"),
    [
        ("a: \x01\n", None),
        ("a: 1\n---\nb: \x01\n", "(document 2)"),
        ("a: 1\n---\nb: 2\n---\nc: \x01", "(document 3)"),
        (f"{BOM}a: 1\n...\n{BOM}b: \x7f\n", "(document 2)"),
    ],
)
def test_rejected_character_reports_its_document(text, marker):
    for load in (fast_yaml.safe_load_all, fast_yaml.parallel.parse_parallel):
        with pytest.raises(ValueError, match="not allowed in YAML") as info:
            list(load(text))
        message = str(info.value)
        assert (marker in message) if marker else ("(document" not in message)
