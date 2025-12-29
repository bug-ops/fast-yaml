//! Common utilities for flow collection rules (braces, brackets).

use crate::{
    LintConfig, Location, Severity, Span,
    diagnostic::{Diagnostic, DiagnosticBuilder},
};

/// Checks if a flow collection is empty (contains only whitespace between delimiters).
///
/// # Arguments
///
/// * `source` - The full YAML source
/// * `start_offset` - Byte offset after opening delimiter
/// * `end_offset` - Byte offset of closing delimiter
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::rules::flow_common::is_empty_collection;
///
/// assert!(is_empty_collection("{}", 1, 1));
/// assert!(is_empty_collection("{  }", 1, 3));
/// assert!(!is_empty_collection("{a}", 1, 2));
/// ```
#[must_use]
pub fn is_empty_collection(source: &str, start_offset: usize, end_offset: usize) -> bool {
    if start_offset >= end_offset || end_offset > source.len() {
        return true;
    }

    source[start_offset..end_offset].trim().is_empty()
}

/// Checks spacing after an opening delimiter (brace or bracket).
///
/// # Arguments
///
/// * `source` - The full YAML source
/// * `start_offset` - Byte offset after opening delimiter
/// * `end_offset` - Byte offset of closing delimiter or next content
/// * `min_spaces` - Minimum required spaces (-1 to disable)
/// * `max_spaces` - Maximum allowed spaces (-1 to disable)
/// * `code` - Rule code for diagnostics
/// * `config` - Lint configuration
/// * `collection_name` - Name of collection type (e.g., "braces", "brackets")
///
/// Returns a diagnostic if spacing constraints are violated.
#[allow(clippy::too_many_arguments)]
pub fn check_spaces_after_opening(
    source: &str,
    start_offset: usize,
    end_offset: usize,
    min_spaces: i64,
    max_spaces: i64,
    code: &str,
    config: &LintConfig,
    collection_name: &str,
) -> Option<Diagnostic> {
    if start_offset >= source.len() {
        return None;
    }

    let content = &source[start_offset..end_offset.min(source.len())];
    let spaces = content.chars().take_while(|c| *c == ' ').count();

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_lossless
    )]
    let spaces_i64 = spaces as i64;

    if min_spaces >= 0 && spaces_i64 < min_spaces {
        let severity = config.get_effective_severity(code, Severity::Warning);
        let loc = Location::new(1, 1, start_offset);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too few spaces inside {collection_name} (expected at least {min_spaces}, found {spaces})"
                ),
                span,
            )
            .build(source),
        );
    }

    if max_spaces >= 0 && spaces_i64 > max_spaces {
        let severity = config.get_effective_severity(code, Severity::Warning);
        let loc = Location::new(1, 1, start_offset);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too many spaces inside {collection_name} (expected at most {max_spaces}, found {spaces})"
                ),
                span,
            )
            .build(source),
        );
    }

    None
}

/// Checks spacing before a closing delimiter (brace or bracket).
///
/// # Arguments
///
/// * `source` - The full YAML source
/// * `start_offset` - Byte offset after opening delimiter or last content
/// * `end_offset` - Byte offset of closing delimiter
/// * `min_spaces` - Minimum required spaces (-1 to disable)
/// * `max_spaces` - Maximum allowed spaces (-1 to disable)
/// * `code` - Rule code for diagnostics
/// * `config` - Lint configuration
/// * `collection_name` - Name of collection type (e.g., "braces", "brackets")
///
/// Returns a diagnostic if spacing constraints are violated.
#[allow(clippy::too_many_arguments)]
pub fn check_spaces_before_closing(
    source: &str,
    start_offset: usize,
    end_offset: usize,
    min_spaces: i64,
    max_spaces: i64,
    code: &str,
    config: &LintConfig,
    collection_name: &str,
) -> Option<Diagnostic> {
    if start_offset >= end_offset || end_offset > source.len() {
        return None;
    }

    let content = &source[start_offset..end_offset];
    let spaces = content.chars().rev().take_while(|c| *c == ' ').count();

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_lossless
    )]
    let spaces_i64 = spaces as i64;

    if min_spaces >= 0 && spaces_i64 < min_spaces {
        let severity = config.get_effective_severity(code, Severity::Warning);
        let loc = Location::new(1, 1, end_offset);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too few spaces inside {collection_name} (expected at least {min_spaces}, found {spaces})"
                ),
                span,
            )
            .build(source),
        );
    }

    if max_spaces >= 0 && spaces_i64 > max_spaces {
        let severity = config.get_effective_severity(code, Severity::Warning);
        let loc = Location::new(1, 1, end_offset);
        let span = Span::new(loc, loc);

        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too many spaces inside {collection_name} (expected at most {max_spaces}, found {spaces})"
                ),
                span,
            )
            .build(source),
        );
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_empty_collection() {
        assert!(is_empty_collection("{}", 1, 1));
        assert!(is_empty_collection("{  }", 1, 3));
        assert!(is_empty_collection("{\n}", 1, 2));
        assert!(!is_empty_collection("{a}", 1, 2));
        assert!(!is_empty_collection("{ key: value }", 1, 13));
    }

    #[test]
    fn test_check_spaces_after_opening() {
        let source = "{ key: value}";
        let config = LintConfig::default();

        // Should pass with 1 space
        let result = check_spaces_after_opening(source, 1, 13, 0, 1, "test", &config, "braces");
        assert!(result.is_none());

        // Should fail with too many spaces
        let source2 = "{  key: value}";
        let result2 = check_spaces_after_opening(source2, 1, 14, 0, 1, "test", &config, "braces");
        assert!(result2.is_some());
    }

    #[test]
    fn test_check_spaces_before_closing() {
        let source = "{key: value }";
        let config = LintConfig::default();

        // Should pass with 1 space
        let result = check_spaces_before_closing(source, 1, 12, 0, 1, "test", &config, "braces");
        assert!(result.is_none());

        // Should fail with too many spaces
        let source2 = "{key: value  }";
        let result2 = check_spaces_before_closing(source2, 1, 13, 0, 1, "test", &config, "braces");
        assert!(result2.is_some());
    }
}
