"""Tests for PyYAML-compatible exception hierarchy."""

import pytest

import fast_yaml


class TestYAMLError:
    """Tests for YAMLError base exception."""

    def test_yaml_error_exists(self):
        """YAMLError exception class exists."""
        assert hasattr(fast_yaml, "YAMLError")

    def test_yaml_error_is_exception(self):
        """YAMLError is a subclass of Exception."""
        assert issubclass(fast_yaml.YAMLError, Exception)

    def test_yaml_error_can_be_raised(self):
        """YAMLError can be raised and caught."""
        with pytest.raises(fast_yaml.YAMLError):
            raise fast_yaml.YAMLError("test error")

    def test_yaml_error_message(self):
        """YAMLError preserves error message."""
        msg = "test error message"
        try:
            raise fast_yaml.YAMLError(msg)
        except fast_yaml.YAMLError as e:
            assert str(e) == msg


class TestMarkedYAMLError:
    """Tests for MarkedYAMLError exception."""

    def test_marked_yaml_error_exists(self):
        """MarkedYAMLError exception class exists."""
        assert hasattr(fast_yaml, "MarkedYAMLError")

    def test_marked_yaml_error_inherits_from_yaml_error(self):
        """MarkedYAMLError inherits from YAMLError."""
        assert issubclass(fast_yaml.MarkedYAMLError, fast_yaml.YAMLError)

    def test_marked_yaml_error_can_be_raised(self):
        """MarkedYAMLError can be raised and caught."""
        with pytest.raises(fast_yaml.MarkedYAMLError):
            raise fast_yaml.MarkedYAMLError("parse error")

    def test_marked_yaml_error_caught_as_yaml_error(self):
        """MarkedYAMLError can be caught as YAMLError."""
        with pytest.raises(fast_yaml.YAMLError):
            raise fast_yaml.MarkedYAMLError("parse error")


class TestScannerError:
    """Tests for ScannerError exception."""

    def test_scanner_error_exists(self):
        """ScannerError exception class exists."""
        assert hasattr(fast_yaml, "ScannerError")

    def test_scanner_error_inherits_from_marked_yaml_error(self):
        """ScannerError inherits from MarkedYAMLError."""
        assert issubclass(fast_yaml.ScannerError, fast_yaml.MarkedYAMLError)

    def test_scanner_error_inherits_from_yaml_error(self):
        """ScannerError inherits from YAMLError (transitive)."""
        assert issubclass(fast_yaml.ScannerError, fast_yaml.YAMLError)

    def test_scanner_error_can_be_raised(self):
        """ScannerError can be raised and caught."""
        with pytest.raises(fast_yaml.ScannerError):
            raise fast_yaml.ScannerError("scanning error")


class TestParserError:
    """Tests for ParserError exception."""

    def test_parser_error_exists(self):
        """ParserError exception class exists."""
        assert hasattr(fast_yaml, "ParserError")

    def test_parser_error_inherits_from_marked_yaml_error(self):
        """ParserError inherits from MarkedYAMLError."""
        assert issubclass(fast_yaml.ParserError, fast_yaml.MarkedYAMLError)

    def test_parser_error_can_be_raised(self):
        """ParserError can be raised and caught."""
        with pytest.raises(fast_yaml.ParserError):
            raise fast_yaml.ParserError("parsing error")


class TestComposerError:
    """Tests for ComposerError exception."""

    def test_composer_error_exists(self):
        """ComposerError exception class exists."""
        assert hasattr(fast_yaml, "ComposerError")

    def test_composer_error_inherits_from_marked_yaml_error(self):
        """ComposerError inherits from MarkedYAMLError."""
        assert issubclass(fast_yaml.ComposerError, fast_yaml.MarkedYAMLError)

    def test_composer_error_can_be_raised(self):
        """ComposerError can be raised and caught."""
        with pytest.raises(fast_yaml.ComposerError):
            raise fast_yaml.ComposerError("composition error")


class TestConstructorError:
    """Tests for ConstructorError exception."""

    def test_constructor_error_exists(self):
        """ConstructorError exception class exists."""
        assert hasattr(fast_yaml, "ConstructorError")

    def test_constructor_error_inherits_from_marked_yaml_error(self):
        """ConstructorError inherits from MarkedYAMLError."""
        assert issubclass(fast_yaml.ConstructorError, fast_yaml.MarkedYAMLError)

    def test_constructor_error_can_be_raised(self):
        """ConstructorError can be raised and caught."""
        with pytest.raises(fast_yaml.ConstructorError):
            raise fast_yaml.ConstructorError("construction error")


class TestEmitterError:
    """Tests for EmitterError exception."""

    def test_emitter_error_exists(self):
        """EmitterError exception class exists."""
        assert hasattr(fast_yaml, "EmitterError")

    def test_emitter_error_inherits_from_yaml_error(self):
        """EmitterError inherits directly from YAMLError (not MarkedYAMLError)."""
        assert issubclass(fast_yaml.EmitterError, fast_yaml.YAMLError)

    def test_emitter_error_not_marked(self):
        """EmitterError does not inherit from MarkedYAMLError."""
        # EmitterError should inherit from YAMLError but not MarkedYAMLError
        assert not issubclass(fast_yaml.EmitterError, fast_yaml.MarkedYAMLError)

    def test_emitter_error_can_be_raised(self):
        """EmitterError can be raised and caught."""
        with pytest.raises(fast_yaml.EmitterError):
            raise fast_yaml.EmitterError("emission error")


class TestMark:
    """Tests for Mark class."""

    def test_mark_exists(self):
        """Mark class exists."""
        assert hasattr(fast_yaml, "Mark")

    def test_mark_instantiate(self):
        """Mark can be instantiated with name, line, column."""
        mark = fast_yaml.Mark("<string>", 5, 10)
        assert mark is not None

    def test_mark_attributes(self):
        """Mark has name, line, and column attributes."""
        mark = fast_yaml.Mark("<string>", 5, 10)
        assert mark.name == "<string>"
        assert mark.line == 5
        assert mark.column == 10

    def test_mark_repr(self):
        """Mark has a proper __repr__ method."""
        mark = fast_yaml.Mark("<string>", 5, 10)
        # Rust uses double quotes in {:?} format
        assert repr(mark) == 'Mark(name="<string>", line=5, column=10)'

    def test_mark_str(self):
        """Mark has a proper __str__ method."""
        mark = fast_yaml.Mark("<string>", 5, 10)
        assert str(mark) == "<string>:5:10"

    def test_mark_with_filename(self):
        """Mark works with filename instead of <string>."""
        mark = fast_yaml.Mark("config.yaml", 42, 15)
        assert mark.name == "config.yaml"
        assert mark.line == 42
        assert mark.column == 15
        assert str(mark) == "config.yaml:42:15"

    def test_mark_zero_indexed(self):
        """Mark uses 0-indexed line and column numbers."""
        mark = fast_yaml.Mark("test.yaml", 0, 0)
        assert mark.line == 0
        assert mark.column == 0


class TestExceptionHierarchy:
    """Tests for exception hierarchy relationships."""

    def test_hierarchy_structure(self):
        """Verify the complete exception hierarchy."""
        # Base exception
        assert issubclass(fast_yaml.YAMLError, Exception)

        # MarkedYAMLError inherits from YAMLError
        assert issubclass(fast_yaml.MarkedYAMLError, fast_yaml.YAMLError)

        # Scanner, Parser, Composer, Constructor all inherit from MarkedYAMLError
        marked_errors = [
            fast_yaml.ScannerError,
            fast_yaml.ParserError,
            fast_yaml.ComposerError,
            fast_yaml.ConstructorError,
        ]
        for error_class in marked_errors:
            assert issubclass(error_class, fast_yaml.MarkedYAMLError)
            assert issubclass(error_class, fast_yaml.YAMLError)

        # EmitterError inherits directly from YAMLError (not MarkedYAMLError)
        assert issubclass(fast_yaml.EmitterError, fast_yaml.YAMLError)
        assert not issubclass(fast_yaml.EmitterError, fast_yaml.MarkedYAMLError)

    def test_catch_all_yaml_errors(self):
        """All YAML errors can be caught with YAMLError."""
        error_classes = [
            fast_yaml.YAMLError,
            fast_yaml.MarkedYAMLError,
            fast_yaml.ScannerError,
            fast_yaml.ParserError,
            fast_yaml.ComposerError,
            fast_yaml.ConstructorError,
            fast_yaml.EmitterError,
        ]

        for error_class in error_classes:
            with pytest.raises(fast_yaml.YAMLError):
                raise error_class("test error")

    def test_catch_all_marked_errors(self):
        """All marked errors can be caught with MarkedYAMLError."""
        marked_error_classes = [
            fast_yaml.MarkedYAMLError,
            fast_yaml.ScannerError,
            fast_yaml.ParserError,
            fast_yaml.ComposerError,
            fast_yaml.ConstructorError,
        ]

        for error_class in marked_error_classes:
            with pytest.raises(fast_yaml.MarkedYAMLError):
                raise error_class("test error")

    def test_emitter_error_not_marked(self):
        """EmitterError cannot be caught as MarkedYAMLError."""
        # This should NOT be caught by MarkedYAMLError
        with pytest.raises(fast_yaml.YAMLError):
            # Verify it's only YAMLError, not MarkedYAMLError
            try:
                raise fast_yaml.EmitterError("emission error")
            except fast_yaml.MarkedYAMLError:
                pytest.fail("EmitterError should not be MarkedYAMLError")


class TestPyYAMLCompatibility:
    """Tests for PyYAML API compatibility."""

    def test_all_exceptions_available_at_module_level(self):
        """All exception classes are available at module level."""
        exceptions = [
            "YAMLError",
            "MarkedYAMLError",
            "ScannerError",
            "ParserError",
            "ComposerError",
            "ConstructorError",
            "EmitterError",
        ]

        for exc_name in exceptions:
            assert hasattr(fast_yaml, exc_name), f"Missing: {exc_name}"

    def test_mark_available_at_module_level(self):
        """Mark class is available at module level."""
        assert hasattr(fast_yaml, "Mark")

    def test_exception_naming_matches_pyyaml(self):
        """Exception class names match PyYAML convention."""
        # Verify naming matches PyYAML expectations
        assert fast_yaml.YAMLError.__name__ == "YAMLError"
        assert fast_yaml.MarkedYAMLError.__name__ == "MarkedYAMLError"
        assert fast_yaml.ScannerError.__name__ == "ScannerError"
        assert fast_yaml.ParserError.__name__ == "ParserError"
        assert fast_yaml.ComposerError.__name__ == "ComposerError"
        assert fast_yaml.ConstructorError.__name__ == "ConstructorError"
        assert fast_yaml.EmitterError.__name__ == "EmitterError"
