"""Tests for typed rule configuration passed through LintConfig(rules=...) (#324, #327)."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path

import pytest

from fast_yaml._core import lint

REPO_ROOT = Path(__file__).resolve().parents[2]


def codes(diagnostics: list[lint.Diagnostic]) -> list[str]:
    return [d.code for d in diagnostics]


class TestRuleOptions:
    def test_document_start_present_bool(self):
        config = lint.LintConfig(rules={"document-start": {"present": True}})
        assert "document-start" in codes(lint.lint("a: 1\n", config))
        assert "document-start" not in codes(lint.lint("---\na: 1\n", config))

    def test_quoted_strings_quote_type(self):
        config = lint.LintConfig(
            rules={"quoted-strings": {"quote-type": "double", "required": True}}
        )
        assert "quoted-strings" in codes(lint.lint("a: 'x'\n", config))
        assert "quoted-strings" not in codes(lint.lint('a: "x"\n', config))

    def test_line_length_max(self):
        config = lint.LintConfig(rules={"line-length": {"max": 10}})
        assert config.max_line_length == 10
        assert "line-length" in codes(lint.lint("key: " + "x" * 20 + "\n", config))

    def test_line_length_max_null_means_no_limit(self):
        config = lint.LintConfig(rules={"line-length": {"max": None}})
        assert config.max_line_length is None
        assert "line-length" not in codes(lint.lint("key: " + "x" * 200 + "\n", config))

    def test_indentation_indent_size_getter(self):
        config = lint.LintConfig(rules={"indentation": {"indent-size": 4}})
        assert config.indent_size == 4

    def test_with_rule_config_options(self):
        config = lint.LintConfig().with_rule_config(
            "line-length", severity="error", options={"max": 10}
        )
        assert config.max_line_length == 10
        diagnostics = lint.lint("key: " + "x" * 20 + "\n", config)
        diag = next(d for d in diagnostics if d.code == "line-length")
        assert str(diag.severity) == "error"

    def test_with_rule_config_options_must_be_mapping(self):
        with pytest.raises(ValueError, match="options must be a mapping"):
            lint.LintConfig().with_rule_config("line-length", options=[1])

    def test_severity_patch_keeps_kwarg_max(self):
        config = lint.LintConfig(max_line_length=120, rules={"line-length": "error"})
        assert config.max_line_length == 120


class TestSeverityStrings:
    @pytest.mark.parametrize("value", ["error", "ERROR", "Warning", "INFO", "hint"])
    def test_case_insensitive(self, value: str):
        lint.LintConfig(rules={"duplicate-key": value})

    def test_invalid_severity_names_rule(self):
        with pytest.raises(ValueError, match="rule 'duplicate-key'.*critical"):
            lint.LintConfig(rules={"duplicate-key": "critical"})


class TestConfigErrors:
    def test_unknown_rule(self):
        with pytest.raises(ValueError, match="unknown rule 'no-such-rule'"):
            lint.LintConfig(rules={"no-such-rule": "error"})

    @pytest.mark.parametrize(
        ("yamllint", "ours"),
        [
            ("key-duplicates", "duplicate-key"),
            ("trailing-spaces", "trailing-whitespace"),
            ("anchors", "invalid-anchor"),
        ],
    )
    def test_renamed_yamllint_rule_gets_hint(self, yamllint: str, ours: str):
        with pytest.raises(
            ValueError, match=f"unknown rule '{yamllint}'; yamllint's '{yamllint}' is '{ours}'"
        ):
            lint.LintConfig(rules={yamllint: "enable"})

    def test_unknown_rule_has_no_yamllint_hint(self):
        with pytest.raises(ValueError) as info:
            lint.LintConfig(rules={"no-such-rule": "enable"})
        assert "yamllint" not in str(info.value)

    def test_quote_type_typo_names_rule_and_key(self):
        with pytest.raises(ValueError, match="rule 'quoted-strings', option 'quote-type'.*singel"):
            lint.LintConfig(rules={"quoted-strings": {"quote-type": "singel"}})

    def test_unknown_option_key(self):
        with pytest.raises(ValueError, match="rule 'line-length'.*maxx"):
            lint.LintConfig(rules={"line-length": {"maxx": 10}})

    def test_wrong_type(self):
        with pytest.raises(ValueError, match="rule 'line-length', option 'max'"):
            lint.LintConfig(rules={"line-length": {"max": "wide"}})

    def test_null_option_is_error(self):
        with pytest.raises(ValueError, match="rule 'indentation', option 'indent-size'"):
            lint.LintConfig(rules={"indentation": {"indent-size": None}})

    def test_rules_must_be_mapping(self):
        with pytest.raises(ValueError, match="mapping"):
            lint.LintConfig(rules="error")

    def test_unsupported_yamllint_option(self):
        with pytest.raises(ValueError, match="supported by yamllint but not implemented"):
            lint.LintConfig(rules={"indentation": {"spaces": 2}})

    def test_document_end_present_false_forbids_marker(self):
        config = lint.LintConfig(rules={"document-end": {"present": False}})
        diagnostics = [
            d for d in lint.lint("a: 1\n...\n", config) if d.code == "document-end"
        ]
        assert [d.message for d in diagnostics] == ["document end marker '...' is forbidden"]
        assert "document-end" not in codes(lint.lint("a: 1\n", config))

    def test_extra_required_regex_flags_plain_scalar(self):
        config = lint.LintConfig(
            rules={"quoted-strings": {"extra-required": ["^http://", r"\.md$"]}}
        )
        found = [d for d in lint.lint("a: http://x\nb: README.md\nc: plain\n", config)]
        assert [d.message for d in found if d.code == "quoted-strings"] == [
            "string should be quoted",
            "string should be quoted",
        ]

    def test_extra_allowed_regex_keeps_plain_scalar(self):
        config = lint.LintConfig(rules={"quoted-strings": {"extra-allowed": ["^ftp://"]}})
        messages = [
            d.message
            for d in lint.lint('a: ftp://x\nb: "ftp://x"\nc: "plain"\n', config)
            if d.code == "quoted-strings"
        ]
        assert messages == ["string does not need quotes"]

    def test_invalid_regex_names_rule_option_and_index(self):
        with pytest.raises(
            ValueError, match=r"rule 'quoted-strings', option 'extra-required'.*pattern 1.*look-around"
        ):
            lint.LintConfig(rules={"quoted-strings": {"extra-required": ["ok", "(?=x)"]}})

    def test_extra_allowed_with_always_is_rejected(self):
        with pytest.raises(ValueError, match="extra-allowed.*only-when-needed"):
            lint.LintConfig(
                rules={"quoted-strings": {"required": "always", "extra-allowed": ["a"]}}
            )

    @pytest.mark.parametrize(
        "build",
        [
            lambda: lint.LintConfig(disabled_rules=["nope"]),
            lambda: lint.LintConfig().with_disabled_rule("nope"),
        ],
        ids=["kwarg", "builder"],
    )
    def test_unknown_disabled_rule(self, build):
        with pytest.raises(ValueError, match="unknown rule 'nope'"):
            build()

    @pytest.mark.parametrize("key", ["enabled", "severity"])
    def test_meta_key_in_options_is_rejected(self, key: str):
        with pytest.raises(ValueError, match=f"pass it as the '{key}' argument"):
            lint.LintConfig().with_rule_config("line-length", options={key: False})

    def test_unquoted_bool_in_truthy_allowed_values(self):
        with pytest.raises(ValueError, match="unquoted boolean.*quote the spelling"):
            lint.LintConfig(rules={"truthy": {"allowed-values": [True]}})


class TestYamllintForms:
    def test_quoted_strings_required_false(self):
        config = lint.LintConfig(rules={"quoted-strings": {"required": False}})
        assert "quoted-strings" not in codes(lint.lint("a: plain\n", config))

    def test_braces_forbid_true(self):
        config = lint.LintConfig(rules={"braces": {"forbid": True}})
        assert "braces" in codes(lint.lint("a: {b: 1}\n", config))

    def test_enable_disable_shorthands(self):
        config = lint.LintConfig(rules={"duplicate-key": "disable"})
        assert "duplicate-key" not in codes(lint.lint("a: 1\na: 2\n", config))
        config = lint.LintConfig(allow_duplicate_keys=True, rules={"duplicate-key": "enable"})
        assert "duplicate-key" in codes(lint.lint("a: 1\na: 2\n", config))

    def test_with_rule_config_unknown_rule(self):
        with pytest.raises(ValueError, match="unknown rule 'nope'"):
            lint.LintConfig().with_rule_config("nope", severity="error")


class TestTruthinessIsNotCoerced:
    """pythonize reads bool by truthiness; the Value buffer must prevent that."""

    @pytest.mark.parametrize("value", ["false", "", "no", 0, 1, [], [False]])
    def test_non_bool_enabled_is_rejected(self, value: object):
        with pytest.raises(ValueError, match="rule 'duplicate-key'"):
            lint.LintConfig(rules={"duplicate-key": {"enabled": value}})

    def test_string_false_does_not_enable_or_disable(self):
        with pytest.raises(ValueError, match="expected a boolean"):
            lint.LintConfig(rules={"duplicate-key": {"enabled": "false"}})

    def test_non_bool_document_start_present_is_rejected(self):
        with pytest.raises(ValueError, match="document-start"):
            lint.LintConfig(rules={"document-start": {"present": "yes please"}})


class TestKwargSemantics:
    def test_max_line_length_has_no_upper_cap(self):
        assert lint.LintConfig(max_line_length=5000).max_line_length == 5000
        assert lint.LintConfig().with_max_line_length(5000).max_line_length == 5000

    @pytest.mark.parametrize("value", [0, -1])
    def test_max_line_length_non_positive_rejected(self, value: int):
        with pytest.raises(ValueError, match="max_line_length must be a positive integer"):
            lint.LintConfig(max_line_length=value)
        with pytest.raises(ValueError, match="max_line_length must be a positive integer"):
            lint.LintConfig().with_max_line_length(value)

    def test_none_max_line_length_disables_limit(self):
        config = lint.LintConfig(max_line_length=None)
        assert "line-length" not in codes(lint.lint("key: " + "x" * 200 + "\n", config))

    @pytest.mark.parametrize("size", [0, 17, -1])
    def test_indent_size_out_of_range(self, size: int):
        with pytest.raises(ValueError, match="between 1 and 16"):
            lint.LintConfig(indent_size=size)
        with pytest.raises(ValueError, match="between 1 and 16"):
            lint.LintConfig().with_indent_size(size)

    def test_rules_can_reenable_allow_duplicate_keys(self):
        config = lint.LintConfig(
            allow_duplicate_keys=True, rules={"duplicate-key": {"enabled": True}}
        )
        assert "duplicate-key" in codes(lint.lint("a: 1\na: 2\n", config))

    def test_disabled_rules_wins_over_rules(self):
        config = lint.LintConfig(
            disabled_rules=["duplicate-key"],
            rules={"duplicate-key": {"enabled": True}},
        )
        assert "duplicate-key" not in codes(lint.lint("a: 1\na: 2\n", config))

    def test_require_document_end(self):
        config = lint.LintConfig(require_document_end=True)
        assert "document-end" in codes(lint.lint("a: 1\n", config))

    def test_repr(self):
        assert (
            repr(lint.LintConfig())
            == "LintConfig(max_line_length=80, indent_size=2, max_input_bytes=104857600)"
        )
        assert (
            repr(lint.LintConfig(max_line_length=None, indent_size=4))
            == "LintConfig(max_line_length=None, indent_size=4, max_input_bytes=104857600)"
        )


def _fy_binary() -> str:
    if env := os.environ.get("FY_BIN"):
        return env
    built = REPO_ROOT / "target" / "debug" / "fy"
    if built.exists():
        return str(built)
    found = shutil.which("fy")
    if found is None:
        pytest.skip("fy binary not built; set FY_BIN or run cargo build --bin fy, see #422")
    return found


def _cli_diagnostics(config_yaml: str, source: str, tmp_path: Path) -> list[tuple]:
    config_path = tmp_path / "config.yaml"
    config_path.write_text(config_yaml)
    result = subprocess.run(
        [_fy_binary(), "lint", "--format", "json", "--config", str(config_path)],
        input=source,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode in (0, 2), result.stderr
    return [
        (d["code"], d["severity"], d["span"]["start"]["line"], d["message"])
        for d in json.loads(result.stdout)
    ]


def _py_diagnostics(rules: dict, source: str) -> list[tuple]:
    config = lint.LintConfig(rules=rules)
    return [
        (d.code, str(d.severity), d.span.start.line, d.message) for d in lint.lint(source, config)
    ]


CROSS_CASES = [
    pytest.param(
        "rules:\n  document-start: {present: true, severity: error}\n",
        {"document-start": {"present": True, "severity": "error"}},
        "a: 1\n",
        id="document-start-required",
    ),
    pytest.param(
        (
            "rules:\n  line-length: {max: 12}\n"
            "  quoted-strings: {quote-type: double, required: true}\n"
        ),
        {"line-length": {"max": 12}, "quoted-strings": {"quote-type": "double", "required": True}},
        "key: " + "x" * 20 + "\nb: 'single'\n",
        id="line-length-and-quoted-strings",
    ),
    pytest.param(
        "rules:\n  line-length: {max: ~}\n  duplicate-key: warning\n",
        {"line-length": {"max": None}, "duplicate-key": "warning"},
        "a: " + "x" * 200 + "\na: 2\n",
        id="no-limit-and-severity-shorthand",
    ),
]


class TestCliParity:
    @pytest.mark.parametrize(("config_yaml", "rules", "source"), CROSS_CASES)
    def test_same_config_same_diagnostics(self, config_yaml, rules, source, tmp_path):
        python = _py_diagnostics(rules, source)
        cli = _cli_diagnostics(config_yaml, source, tmp_path)
        assert python
        assert python == cli

    def test_same_error_text(self, tmp_path):
        config_path = tmp_path / "config.yaml"
        config_path.write_text("rules:\n  quoted-strings: {quote-type: singel}\n")
        result = subprocess.run(
            [_fy_binary(), "lint", "--config", str(config_path)],
            input="a: 1\n",
            capture_output=True,
            text=True,
            check=False,
        )
        with pytest.raises(ValueError) as exc:
            lint.LintConfig(rules={"quoted-strings": {"quote-type": "singel"}})
        assert str(exc.value) in result.stderr
