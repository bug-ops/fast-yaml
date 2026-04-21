"""Parametrized tests against yaml-test-suite cases.

Set YAML_TEST_SUITE_PATH to the cloned yaml-test-suite repository root.
Without it, this module is skipped entirely.
"""

import os
from pathlib import Path

import pytest

try:
    import fast_yaml
except ImportError:
    pytest.skip("fast_yaml not installed; run `uv run maturin develop`", allow_module_level=True)

from _suite_support import collect_cases, yaml_json_equal

_SUITE_PATH = os.environ.get("YAML_TEST_SUITE_PATH")
if not _SUITE_PATH:
    pytest.skip("YAML_TEST_SUITE_PATH not set", allow_module_level=True)

_XFAIL_PATH = Path(__file__).parent / "data" / "yaml_test_suite_xfail.txt"
_COLLECT_MODE = os.environ.get("YAML_TEST_SUITE_XFAIL_COLLECT") == "1"


def _load_xfail(path):
    if not path.exists():
        return {}
    result = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        parts = line.split()
        case_id = parts[0]
        strict = len(parts) < 2 or parts[1] != "?"
        result[case_id] = strict
    return result


XFAIL = {} if _COLLECT_MODE else _load_xfail(_XFAIL_PATH)

_CASES = collect_cases(_SUITE_PATH)


def _make_param(case):
    marks = []
    mode = XFAIL.get(case["id"])
    if mode is True:
        marks.append(pytest.mark.xfail(strict=True, reason="known failure; see xfail list"))
    elif mode is False:
        marks.append(
            pytest.mark.xfail(strict=False, reason="known failure (non-strict); see xfail list")
        )
    return pytest.param(case, id=case["id"], marks=marks)


@pytest.mark.parametrize("case", [_make_param(c) for c in _CASES])
def test_yaml_test_suite(case):
    if case["skip_reason"]:
        pytest.skip(case["skip_reason"])

    source = (case["path"] / "in.yaml").read_bytes().decode("utf-8")

    if case["has_error"]:
        with pytest.raises(ValueError):
            list(fast_yaml.safe_load_all(source))
        return

    if case["has_json"]:
        expected_docs = case["json_docs"]

        if case["multi_doc"]:
            actual_docs = list(fast_yaml.safe_load_all(source))
            assert len(actual_docs) == len(expected_docs), (
                f"multi-doc count mismatch: {len(actual_docs)} != {len(expected_docs)}"
            )
            for i, (actual, expected) in enumerate(zip(actual_docs, expected_docs)):
                ok, reason = yaml_json_equal(actual, expected)
                assert ok, f"doc[{i}]: {reason}"
        else:
            actual = fast_yaml.safe_load(source)
            ok, reason = yaml_json_equal(actual, expected_docs[0])
            assert ok, reason
        return

    pytest.skip("no comparable output")
