"""Merge-key errors report the source position of the offending `<<` key (#492)."""

import pytest

import fast_yaml
from fast_yaml import parallel

POSITION_CASES = [
    pytest.param("m:\n  <<: 1\n  k: 0\n", 2, 3, id="first-key"),
    pytest.param("m:\n  k: 0\n  <<: 1\n", 3, 3, id="last-key"),
    pytest.param("m: {k: 0, <<: 1}\n", 1, 11, id="flow-mapping"),
    pytest.param("a:\n  b:\n    <<: [{x: 1}, 5]\n", 3, 5, id="sequence-item"),
    pytest.param("m:\n  <<: 1\n  <<: {x: 1}\n", 2, 3, id="invalid-earlier-repeat"),
    pytest.param("m:\n  <<: {x: 1}\n  <<: 1\n", 3, 3, id="invalid-later-repeat"),
    pytest.param("s: &s !!set {x}\nm:\n  <<: *s\n", 3, 3, id="set-source"),
    pytest.param('m: {"\u00e9\u00e9": 1, <<: 1}\n', 1, 14, id="non-ascii-before-key"),
    pytest.param("a: 1\r\nm:\r\n  <<: 1\r\n", 3, 3, id="crlf"),
]


@pytest.mark.parametrize(("doc", "line", "column"), POSITION_CASES)
def test_safe_load_reports_key_position(doc, line, column):
    with pytest.raises(ValueError, match=f"at line {line}, column {column}"):
        fast_yaml.safe_load(doc)


@pytest.mark.parametrize(("doc", "line", "column"), POSITION_CASES)
def test_parse_parallel_reports_key_position(doc, line, column):
    with pytest.raises(ValueError, match=f"at line {line}, column {column}"):
        parallel.parse_parallel(doc)


def test_position_in_later_document_is_absolute():
    doc = "a: 1\n---\nb: 2\n---\nm:\n  <<: 1\n"
    with pytest.raises(ValueError, match="at line 6, column 3"):
        fast_yaml.safe_load_all(doc)
    with pytest.raises(ValueError, match="at line 6, column 3"):
        parallel.parse_parallel(doc)
