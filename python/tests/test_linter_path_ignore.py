"""Per-rule ``ignore`` with a ``path`` argument, and yamllint rule names (#585, #589)."""

from __future__ import annotations

import os
from pathlib import Path

import pytest

from fast_yaml.lint import LintConfig, Linter, lint

SOURCE = "a: 1 \nb: 2\nb: 3\n"


def codes(diagnostics) -> set[str]:
    return {d.code for d in diagnostics}


@pytest.fixture
def workdir(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    (tmp_path / "src").mkdir()
    (tmp_path / "generated").mkdir()
    monkeypatch.chdir(tmp_path)
    return tmp_path


def test_rule_ignore_skips_the_rule_for_a_matching_path(workdir: Path) -> None:
    config = LintConfig(rules={"trailing-whitespace": {"ignore": "generated/"}})

    kept = codes(lint(SOURCE, config, path=workdir / "src" / "a.yaml"))
    skipped = codes(lint(SOURCE, config, path=workdir / "generated" / "a.yaml"))

    assert "trailing-whitespace" in kept
    assert "trailing-whitespace" not in skipped
    assert "duplicate-key" in skipped


def test_a_source_without_a_path_is_never_ignored(workdir: Path) -> None:
    config = LintConfig(rules={"trailing-whitespace": {"ignore": ["*"]}})
    assert "trailing-whitespace" in codes(lint(SOURCE, config))


def test_path_accepts_str_and_pathlike_and_relative_spellings(workdir: Path) -> None:
    config = LintConfig(rules={"trailing-whitespace": {"ignore": ["generated/"]}})
    for path in (
        "generated/a.yaml",
        os.fspath(workdir / "generated" / "a.yaml"),
        Path("src") / ".." / "generated" / "a.yaml",
    ):
        assert "trailing-whitespace" not in codes(lint(SOURCE, config, path=path))


def test_linter_method_takes_the_path_too(workdir: Path) -> None:
    config = LintConfig(rules={"trailing-whitespace": {"ignore": ["generated/"]}})
    linter = Linter(config)
    assert "trailing-whitespace" not in codes(linter.lint(SOURCE, "generated/a.yaml"))
    assert "trailing-whitespace" in codes(linter.lint(SOURCE, "src/a.yaml"))
    assert "trailing-whitespace" in codes(linter.lint(SOURCE))


def test_ignore_from_file_reads_relative_to_the_working_directory(workdir: Path) -> None:
    (workdir / "ignores").write_text("generated/\n")
    config = LintConfig(rules={"trailing-whitespace": {"ignore-from-file": "ignores"}})
    assert "trailing-whitespace" not in codes(lint(SOURCE, config, path="generated/a.yaml"))


def test_ignore_and_ignore_from_file_conflict(workdir: Path) -> None:
    with pytest.raises(ValueError, match="cannot be used together"):
        LintConfig(rules={"braces": {"ignore": ["x/"], "ignore-from-file": "ignores"}})


def test_a_path_in_a_missing_directory_is_a_value_error_only_with_ignore(workdir: Path) -> None:
    config = LintConfig(rules={"trailing-whitespace": {"ignore": ["generated/"]}})
    with pytest.raises(ValueError, match="cannot resolve path"):
        lint(SOURCE, config, path="no-such-dir/a.yaml")


def test_path_does_not_touch_the_file_system_without_ignore(workdir: Path) -> None:
    for path in ("no-such-dir/a.yaml", "/nonexistent/x.yaml", ""):
        assert "duplicate-key" in codes(lint(SOURCE, path=path))
        assert "duplicate-key" in codes(Linter(LintConfig()).lint(SOURCE, path))


def test_with_rule_config_honors_ignore_like_the_constructor(workdir: Path) -> None:
    config = LintConfig().with_rule_config("trailing-whitespace", options={"ignore": "generated/"})
    assert "trailing-whitespace" not in codes(lint(SOURCE, config, path="generated/a.yaml"))
    assert "trailing-whitespace" in codes(lint(SOURCE, config, path="src/a.yaml"))


def test_a_path_to_a_file_that_does_not_exist_yet_is_accepted(workdir: Path) -> None:
    config = LintConfig(rules={"trailing-whitespace": {"ignore": ["generated/"]}})
    result = lint(SOURCE, config, path="generated/new.yaml")
    assert "trailing-whitespace" not in codes(result)


def test_yamllint_rule_names_are_accepted() -> None:
    config = LintConfig(
        rules={"trailing-spaces": "disable", "key-duplicates": "warning"},
        disabled_rules=["anchors"],
    )
    found = lint(SOURCE, config)
    assert "trailing-whitespace" not in codes(found)
    assert {d.severity.as_str() for d in found if d.code == "duplicate-key"} == {"warning"}


def test_enable_resets_to_yamllint_defaults() -> None:
    config = LintConfig(rules={"document-start": "enable"})
    assert "document-start" in codes(lint("a: 1\n", config))
