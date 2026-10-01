"""Locates the `fy` binary for tests that compare the bindings with the CLI."""

from __future__ import annotations

import os
import shutil
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[2]


def fy_binary() -> str:
    """Return the `fy` path from FY_BIN, target/debug or PATH; fail in CI, skip locally."""
    if env := os.environ.get("FY_BIN"):
        return env
    built = REPO_ROOT / "target" / "debug" / "fy"
    if built.exists():
        return str(built)
    if found := shutil.which("fy"):
        return found
    message = "fy binary not built; set FY_BIN or run cargo build --bin fy"
    if os.environ.get("CI"):
        pytest.fail(message)
    pytest.skip(message)
