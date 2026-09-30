"""NUL is rejected instead of truncating the document (#417)."""

import pytest

import fast_yaml

INPUTS = ["a: 1\0\nb: 2\n", "\0a: 1\n", "# c\0\na: 1\n", 'a: "x\0y"\n', "a: 1\n---\nb: 2\0\n"]


@pytest.mark.parametrize("text", INPUTS)
def test_load_rejects_nul(text):
    with pytest.raises(ValueError, match="NUL"):
        fast_yaml.safe_load(text)
    with pytest.raises(ValueError, match="NUL"):
        list(fast_yaml.safe_load_all(text))
