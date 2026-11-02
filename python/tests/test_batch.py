"""Tests for the batch file processing module."""

from __future__ import annotations

import os
import sys
import tempfile
from pathlib import Path

import pytest

from fast_yaml._core import batch


@pytest.fixture
def temp_yaml_files():
    """Create temporary YAML files for testing."""
    with tempfile.TemporaryDirectory() as tmpdir:
        files = []
        for i in range(5):
            path = Path(tmpdir) / f"file{i}.yaml"
            path.write_text(f"key{i}: value{i}\n")
            files.append(str(path))
        yield files


@pytest.fixture
def temp_invalid_yaml():
    """Create temporary invalid YAML file."""
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "invalid.yaml"
        path.write_text("invalid: [\n")
        yield str(path)


@pytest.fixture
def temp_unformatted_yaml():
    """Create temporary unformatted YAML file."""
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "unformatted.yaml"
        path.write_text("key:     value\nnested:    {a: 1, b: 2}\n")
        yield str(path)


class TestFileError:
    """Tests for FileErrorKind and FileError."""

    def test_kinds_exist_and_compare(self):
        kinds = [
            batch.FileErrorKind.Io,
            batch.FileErrorKind.Decode,
            batch.FileErrorKind.Parse,
            batch.FileErrorKind.Limit,
            batch.FileErrorKind.Format,
            batch.FileErrorKind.EmptyDocument,
            batch.FileErrorKind.Comments,
            batch.FileErrorKind.Write,
            batch.FileErrorKind.ThreadPool,
        ]
        assert len(set(kinds)) == len(kinds)
        assert batch.FileErrorKind.Io == batch.FileErrorKind.Io

    def test_removed_file_result_types_are_gone(self):
        assert not hasattr(batch, "FileResult")
        assert not hasattr(batch, "FileOutcome")


class TestBatchConfig:
    """Tests for BatchConfig class."""

    def test_default_config(self):
        """Test default configuration."""
        config = batch.BatchConfig()
        assert config is not None

    def test_custom_workers(self):
        """Test custom worker count."""
        config = batch.BatchConfig(workers=4)
        assert config is not None

    def test_auto_workers(self):
        """Test auto worker count (None)."""
        config = batch.BatchConfig(workers=None)
        assert config is not None

    def test_custom_limits(self):
        """Test custom limits."""
        config = batch.BatchConfig(
            max_input_bytes=50 * 1024 * 1024,
        )
        assert config is not None

    def test_formatting_options(self):
        """Test formatting options."""
        config = batch.BatchConfig(
            indent=4,
        )
        assert config is not None

    def test_builder_chaining(self):
        """Test chaining builder methods."""
        config = batch.BatchConfig().with_workers(4).with_indent(4)
        assert config is not None

    def test_config_repr(self):
        """Test config repr."""
        config = batch.BatchConfig()
        assert "BatchConfig" in repr(config)

    def test_workers_limit(self):
        """Test workers limit enforcement."""
        with pytest.raises(ValueError, match="workers must be between 0 and 128, got 1000"):
            batch.BatchConfig(workers=1000)
        with pytest.raises(ValueError, match="workers must be between 0 and 128, got 129"):
            batch.BatchConfig().with_workers(129)
        assert batch.BatchConfig(workers=128) is not None

    def test_repr_shows_the_workers_setting(self):
        """Auto prints None, sequential 0, a fixed pool its size."""
        assert "workers=None" in repr(batch.BatchConfig())
        assert "workers=0" in repr(batch.BatchConfig(workers=0))
        assert "workers=4" in repr(batch.BatchConfig().with_workers(4))

    def test_max_input_bytes_limit(self):
        """Test max input bytes limit."""
        with pytest.raises(ValueError, match="max_input_bytes must be between 1 and 1073741824"):
            batch.BatchConfig(max_input_bytes=2 * 1024 * 1024 * 1024)

    def test_max_input_bytes_rejects_zero(self):
        """Zero is not "unlimited"."""
        with pytest.raises(ValueError, match="max_input_bytes must be between 1 and"):
            batch.BatchConfig(max_input_bytes=0)


class TestProcessFiles:
    """Tests for process_files function."""

    def test_process_valid_files(self, temp_yaml_files):
        """Test processing valid files."""
        result = batch.process_files(temp_yaml_files)
        assert isinstance(result, batch.BatchResult)
        assert result.total == 5
        assert result.success == 5
        assert result.failed == 0
        assert result.is_success()

    def test_process_with_invalid_file(self, temp_yaml_files, temp_invalid_yaml):
        """Test processing with one invalid file."""
        paths = temp_yaml_files + [temp_invalid_yaml]
        result = batch.process_files(paths)
        assert result.total == 6
        assert result.success == 5
        assert result.failed == 1
        assert not result.is_success()
        [(path, error)] = result.errors()
        assert path == temp_invalid_yaml
        assert error.kind == batch.FileErrorKind.Parse

    def test_process_empty_list(self):
        """Test processing empty list."""
        result = batch.process_files([])
        assert result.total == 0
        assert result.is_success()

    def test_process_with_config(self, temp_yaml_files):
        """Test processing with custom config."""
        config = batch.BatchConfig(workers=2)
        result = batch.process_files(temp_yaml_files, config)
        assert result.is_success()

    def test_process_nonexistent_file(self):
        """Test processing nonexistent file."""
        result = batch.process_files(["/nonexistent/file.yaml"])
        assert result.total == 1
        assert result.failed == 1

    def test_process_files_per_second(self, temp_yaml_files):
        """Test files_per_second calculation."""
        result = batch.process_files(temp_yaml_files)
        fps = result.files_per_second()
        assert fps >= 0


class TestFormatFiles:
    """Tests for format_files function."""

    def test_format_valid_files(self, temp_yaml_files):
        """Test formatting valid files."""
        results = batch.format_files(temp_yaml_files)
        assert len(results) == 5
        for result in results:
            assert isinstance(result, batch.FormatResult.Formatted)
            assert result.content

    def test_format_preserves_explicit_tags(self):
        """Explicit tags must survive format_files."""
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "tagged.yaml"
            path.write_text("a: !!str 1\nb: !custom x\n")
            [result] = batch.format_files([str(path)])
        assert isinstance(result, batch.FormatResult.Formatted)
        assert "a: !!str 1" in result.content
        assert "b: !custom x" in result.content

    def test_format_with_invalid_file(self, temp_invalid_yaml):
        """Test formatting invalid file."""
        results = batch.format_files([temp_invalid_yaml])
        assert len(results) == 1
        [result] = results
        assert isinstance(result, batch.FormatResult.Failed)
        assert result.error.kind == batch.FileErrorKind.Parse
        assert not hasattr(result, "content")
        match result:
            case batch.FormatResult.Failed(path=path, error=error):
                assert path == temp_invalid_yaml
                assert error.message
            case _:
                pytest.fail("expected Failed")

    def test_format_accepts_pathlike(self, temp_yaml_files):
        results = batch.format_files([Path(p) for p in temp_yaml_files])
        assert [r.path for r in results] == temp_yaml_files

    def test_format_empty_list(self):
        """Test formatting empty list."""
        results = batch.format_files([])
        assert len(results) == 0

    def test_format_with_config(self, temp_yaml_files):
        """Test formatting with custom config."""
        config = batch.BatchConfig(indent=4)
        results = batch.format_files(temp_yaml_files, config)
        assert len(results) == 5

    def test_format_does_not_modify_files(self, temp_unformatted_yaml):
        """Test that format_files does not modify files."""
        original = Path(temp_unformatted_yaml).read_text()
        batch.format_files([temp_unformatted_yaml])
        assert Path(temp_unformatted_yaml).read_text() == original


class TestFormatFilesInPlace:
    """Tests for format_files_in_place function."""

    def test_format_in_place_changes_file(self, temp_unformatted_yaml):
        """Test that format_files_in_place modifies files."""
        result = batch.format_files_in_place([temp_unformatted_yaml])

        assert result.total == 1
        assert result.is_success()

    def test_format_in_place_tracking(self, temp_yaml_files):
        """Test changed/unchanged tracking."""
        result = batch.format_files_in_place(temp_yaml_files)
        assert result.total == 5
        assert result.success == 5

    def test_format_in_place_empty_list(self):
        """Test formatting empty list."""
        result = batch.format_files_in_place([])
        assert result.total == 0
        assert result.is_success()

    def test_format_in_place_with_config(self, temp_yaml_files):
        """Test formatting with custom config."""
        config = batch.BatchConfig(indent=4)
        result = batch.format_files_in_place(temp_yaml_files, config)
        assert result.is_success()

    def test_format_in_place_nonexistent_file(self):
        """Test formatting nonexistent file."""
        result = batch.format_files_in_place(["/nonexistent/file.yaml"])
        assert result.total == 1
        assert result.failed == 1

    def test_two_names_of_one_file(self, tmp_path):
        """Hard-link and symlink names of one file among many targets stay consistent."""
        distinct = []
        for i in range(10):
            path = tmp_path / f"file{i}.yaml"
            path.write_text(f"key{i}:     value{i}\nnested:    {{a: {i}, b: 2}}\n")
            distinct.append(path)
        hardlink = tmp_path / "hardlink.yaml"
        symlink = tmp_path / "symlink.yaml"
        try:
            os.link(distinct[0], hardlink)
            symlink.symlink_to(distinct[0])
        except (OSError, NotImplementedError) as exc:
            if os.environ.get("CI"):
                pytest.fail(f"links unavailable in CI: {exc}")
            pytest.skip(f"hard links or symlinks unavailable: {exc}")

        targets = [*distinct, hardlink, symlink]
        result = batch.format_files_in_place([str(p) for p in targets])

        assert result.total == len(targets)
        assert result.success == len(targets)
        assert result.failed == 0
        assert result.errors() == []
        for i, path in enumerate(distinct):
            assert path.read_text() == f"key{i}: value{i}\nnested:\n  a: {i}\n  b: 2\n"
        assert hardlink.read_text() == distinct[0].read_text()
        assert symlink.read_text() == distinct[0].read_text()
        assert symlink.is_symlink()
        if sys.platform != "win32":
            assert os.path.samefile(hardlink, distinct[0])


class TestBatchResult:
    """Tests for BatchResult class."""

    def test_batch_result_attributes(self, temp_yaml_files):
        """Test BatchResult attributes."""
        result = batch.process_files(temp_yaml_files)
        assert hasattr(result, "total")
        assert hasattr(result, "success")
        assert hasattr(result, "changed")
        assert hasattr(result, "failed")
        assert hasattr(result, "duration_ms")

    def test_batch_result_repr(self, temp_yaml_files):
        """Test BatchResult repr."""
        result = batch.process_files(temp_yaml_files)
        assert "BatchResult" in repr(result)

    def test_batch_result_errors_method(self, temp_invalid_yaml):
        """Test errors() method."""
        result = batch.process_files([temp_invalid_yaml])
        errors = result.errors()
        assert isinstance(errors, list)
        assert len(errors) == 1
        path, error = errors[0]
        assert isinstance(path, str)
        assert isinstance(error, batch.FileError)
        assert isinstance(error.message, str)


class TestEdgeCases:
    """Edge case tests for batch processing."""

    def test_unicode_file_content(self):
        """Test processing files with unicode content."""
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "unicode.yaml"
            path.write_text("chinese: 中文\n", encoding="utf-8")
            result = batch.process_files([str(path)])
            assert result.is_success()

    def test_large_file(self):
        """Test processing large file."""
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "large.yaml"
            content = "key: value\n" * 100_000
            path.write_text(content)
            result = batch.process_files([str(path)])
            assert result.is_success()

    def test_many_files(self):
        """Test processing many files."""
        with tempfile.TemporaryDirectory() as tmpdir:
            paths = []
            for i in range(100):
                path = Path(tmpdir) / f"file{i}.yaml"
                path.write_text(f"id: {i}\n")
                paths.append(str(path))
            result = batch.process_files(paths)
            assert result.total == 100
            assert result.is_success()

    def test_empty_file(self):
        """Test processing empty file."""
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "empty.yaml"
            path.write_text("")
            result = batch.process_files([str(path)])
            # Empty file behavior depends on parser
            assert result.total == 1

    def test_special_characters_in_path(self):
        """Test processing file with special characters in path."""
        with tempfile.TemporaryDirectory() as tmpdir:
            path = Path(tmpdir) / "file with spaces.yaml"
            path.write_text("key: value\n")
            result = batch.process_files([str(path)])
            assert result.is_success()


def test_mmap_threshold_removed():
    with pytest.raises(TypeError):
        batch.BatchConfig(mmap_threshold=1024)
