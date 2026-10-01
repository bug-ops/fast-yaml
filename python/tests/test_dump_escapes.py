"""Dump escapes, flow quoting and empty loads shared with the CLI and Node (#557)."""

import fast_yaml


def test_unicode_line_separators_are_escaped():
    assert fast_yaml.safe_dump("a\u0085b") == '"a\\x85b"\n'
    assert fast_yaml.safe_dump({"k": "a b c"}) == 'k: "a\\u2028b\\u2029c"\n'
    assert fast_yaml.safe_dump(["a\u0085b"], default_flow_style=True) == '["a\\x85b"]\n'


def test_comment_only_and_empty_sources_load():
    assert fast_yaml.safe_load("# only a comment\n") is None
    assert list(fast_yaml.safe_load_all("# only a comment\n")) == [None]
    assert fast_yaml.safe_load("") is None
    assert list(fast_yaml.safe_load_all("")) == []


def test_flow_dump_quotes_question_mark_and_marks_long_keys():
    assert (
        fast_yaml.safe_dump({"a": ["x?y", "?"]}, default_flow_style=True) == '{a: ["x?y", "?"]}\n'
    )
    long_key = "k" * 1100
    flow = fast_yaml.safe_dump({long_key: 1}, default_flow_style=True)
    assert flow.startswith("{? kkk")
    assert fast_yaml.safe_load(flow) == {long_key: 1}


def test_negative_zero_round_trips():
    assert fast_yaml.safe_dump(-0.0) == "-0.0\n"
    assert str(fast_yaml.safe_load("-0.0")) == "-0.0"
