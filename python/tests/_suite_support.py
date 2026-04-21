"""Discovery and equality helpers for yaml-test-suite parametrized tests."""

import json
import math
from pathlib import Path, PurePosixPath


def parse_ndjson(text):
    """Parse one or more concatenated JSON values from text; return list."""
    decoder = json.JSONDecoder()
    docs = []
    pos = 0
    text = text.strip()
    while pos < len(text):
        remainder = text[pos:].lstrip()
        if not remainder:
            break
        offset = len(text[pos:]) - len(remainder)
        value, end = decoder.raw_decode(remainder)
        docs.append(value)
        pos += offset + end
    return docs


def collect_cases(suite_root):
    root = Path(suite_root)
    cases = []
    for in_yaml in sorted(root.rglob("in.yaml")):
        case_dir = in_yaml.parent
        case_id = PurePosixPath(case_dir.relative_to(root)).as_posix()
        in_json = case_dir / "in.json"
        error_file = case_dir / "error"
        event_file = case_dir / "test.event"

        has_error = error_file.exists()
        has_json = in_json.exists()
        skip_reason = None
        json_docs = None
        multi_doc = False

        if has_json:
            raw = in_json.read_text(encoding="utf-8")
            json_docs = parse_ndjson(raw)
            multi_doc = len(json_docs) > 1

        if not has_json and not has_error and event_file.exists():
            skip_reason = "event-stream only"

        cases.append({
            "id": case_id,
            "path": case_dir,
            "has_json": has_json,
            "has_error": has_error,
            "multi_doc": multi_doc,
            "json_docs": json_docs,
            "skip_reason": skip_reason,
        })
    return cases


def yaml_json_equal(actual, expected):
    if expected is None:
        if actual is not None:
            return False, f"expected None, got {type(actual).__name__}"
        return True, ""

    if type(expected) is bool:
        if type(actual) is not bool or actual != expected:
            return False, f"expected bool {expected}, got {type(actual).__name__} {actual!r}"
        return True, ""

    if type(expected) is int:
        if type(actual) is bool:
            return False, f"expected int, got bool {actual!r}"
        if type(actual) is int:
            if actual != expected:
                return False, f"expected {expected}, got {actual}"
            return True, ""
        # accept float when value is integer-exact (e.g. YAML 1e2 vs JSON 100)
        if type(actual) is float and not math.isnan(actual) and actual == expected:
            return True, ""
        return False, f"expected int, got {type(actual).__name__} {actual!r}"

    if type(expected) is float:
        if type(actual) is not float and type(actual) is not int:
            return False, f"expected float, got {type(actual).__name__} {actual!r}"
        if math.isnan(expected):
            if not (type(actual) is float and math.isnan(actual)):
                return False, f"expected NaN, got {actual!r}"
            return True, ""
        if actual != expected:
            return False, f"expected {expected}, got {actual}"
        return True, ""

    if type(expected) is str:
        if type(actual) is not str:
            return False, f"expected str, got {type(actual).__name__} {actual!r}"
        if actual != expected:
            return False, f"expected {expected!r}, got {actual!r}"
        return True, ""

    if type(expected) is list:
        if type(actual) is not list:
            return False, f"expected list, got {type(actual).__name__}"
        if len(actual) != len(expected):
            return False, f"list length mismatch: {len(actual)} != {len(expected)}"
        for i, (a, e) in enumerate(zip(actual, expected)):
            ok, reason = yaml_json_equal(a, e)
            if not ok:
                return False, f"[{i}]: {reason}"
        return True, ""

    if type(expected) is dict:
        if type(actual) is not dict:
            return False, f"expected dict, got {type(actual).__name__}"
        if set(expected.keys()) != set(actual.keys()):
            return False, f"key mismatch: {set(expected.keys())} != {set(actual.keys())}"
        for k in expected:
            ok, reason = yaml_json_equal(actual[k], expected[k])
            if not ok:
                return False, f"[{k!r}]: {reason}"
        return True, ""

    return False, f"unhandled expected type {type(expected).__name__}"
