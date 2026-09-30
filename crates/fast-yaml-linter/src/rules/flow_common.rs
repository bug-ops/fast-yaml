//! Common utilities for flow collection rules (braces, brackets).

use crate::{
    LintConfig, Severity, SourceContext, Span,
    diagnostic::{Diagnostic, DiagnosticBuilder},
    tokenizer::Token,
};

/// Pairs opening and closing delimiter tokens by nesting depth.
///
/// Both slices must be sorted by offset (as returned by `FlowTokenizer::find_all`).
/// Unmatched openers and closers are skipped. Pairs are returned in opener order.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{
///     rules::flow_common::pair_delimiters,
///     tokenizer::{FlowTokenizer, TokenType},
///     SourceContext,
/// };
///
/// let yaml = "{a: {b: c}}";
/// let ctx = SourceContext::new(yaml);
/// let tokenizer = FlowTokenizer::new(yaml, &ctx);
/// let opens = tokenizer.find_all(TokenType::BraceOpen);
/// let closes = tokenizer.find_all(TokenType::BraceClose);
///
/// let pairs = pair_delimiters(&opens, &closes);
/// assert_eq!(pairs.len(), 2);
/// assert_eq!(pairs[0].1.span.start.offset, 10);
/// ```
#[must_use]
pub fn pair_delimiters<'t>(opens: &'t [Token], closes: &'t [Token]) -> Vec<(&'t Token, &'t Token)> {
    let mut stack: Vec<usize> = Vec::new();
    let mut matched: Vec<(usize, &'t Token)> = Vec::new();
    let mut next_open = 0;

    for close in closes {
        while let Some(open) = opens.get(next_open)
            && open.span.start.offset < close.span.start.offset
        {
            stack.push(next_open);
            next_open += 1;
        }
        if let Some(open_idx) = stack.pop() {
            matched.push((open_idx, close));
        }
    }

    matched.sort_unstable_by_key(|&(open_idx, _)| open_idx);
    matched
        .into_iter()
        .map(|(open_idx, close)| (&opens[open_idx], close))
        .collect()
}

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
    let end = end_offset.min(source.len());
    if start_offset >= end {
        return true;
    }

    source
        .get(start_offset..end)
        .is_none_or(|s| s.trim().is_empty())
}

/// Checks spacing after an opening delimiter (brace or bracket).
///
/// # Arguments
///
/// * `source` - The full YAML source
/// * `source_ctx` - Pre-built source context for diagnostic extraction
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
    source_ctx: &SourceContext<'_>,
    start_offset: usize,
    end_offset: usize,
    min_spaces: i64,
    max_spaces: i64,
    code: &str,
    config: &LintConfig,
    collection_name: &str,
    opening_span: Span,
) -> Option<Diagnostic> {
    let end = end_offset.min(source.len());
    if start_offset > end {
        return None;
    }

    let content = source.get(start_offset..end)?;
    let spaces = content.chars().take_while(|c| *c == ' ').count();

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_lossless
    )]
    let spaces_i64 = spaces as i64;

    if min_spaces >= 0 && spaces_i64 < min_spaces {
        let severity = config.get_effective_severity(code, Severity::Warning);
        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too few spaces inside {collection_name} (expected at least {min_spaces}, found {spaces})"
                ),
                opening_span,
            )
            .build_with_context(source_ctx),
        );
    }

    if max_spaces >= 0 && spaces_i64 > max_spaces {
        let severity = config.get_effective_severity(code, Severity::Warning);
        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too many spaces inside {collection_name} (expected at most {max_spaces}, found {spaces})"
                ),
                opening_span,
            )
            .build_with_context(source_ctx),
        );
    }

    None
}

/// Checks spacing before a closing delimiter (brace or bracket).
///
/// # Arguments
///
/// * `source` - The full YAML source
/// * `source_ctx` - Pre-built source context for diagnostic extraction
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
    source_ctx: &SourceContext<'_>,
    start_offset: usize,
    end_offset: usize,
    min_spaces: i64,
    max_spaces: i64,
    code: &str,
    config: &LintConfig,
    collection_name: &str,
    closing_span: Span,
) -> Option<Diagnostic> {
    let end = end_offset.min(source.len());
    if start_offset >= end {
        return None;
    }

    let content = source.get(start_offset..end)?;
    let spaces = content.chars().rev().take_while(|c| *c == ' ').count();

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_lossless
    )]
    let spaces_i64 = spaces as i64;

    if min_spaces >= 0 && spaces_i64 < min_spaces {
        let severity = config.get_effective_severity(code, Severity::Warning);
        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too few spaces inside {collection_name} (expected at least {min_spaces}, found {spaces})"
                ),
                closing_span,
            )
            .build_with_context(source_ctx),
        );
    }

    if max_spaces >= 0 && spaces_i64 > max_spaces {
        let severity = config.get_effective_severity(code, Severity::Warning);
        return Some(
            DiagnosticBuilder::new(
                code,
                severity,
                format!(
                    "too many spaces inside {collection_name} (expected at most {max_spaces}, found {spaces})"
                ),
                closing_span,
            )
            .build_with_context(source_ctx),
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
        use crate::{Location, SourceContext, Span};
        let dummy_span = Span::new(Location::new(1, 1, 0), Location::new(1, 2, 1));
        let source = "{ key: value}";
        let source_ctx = SourceContext::new(source);
        let config = LintConfig::default();

        // Should pass with 1 space
        let result = check_spaces_after_opening(
            source,
            &source_ctx,
            1,
            13,
            0,
            1,
            "test",
            &config,
            "braces",
            dummy_span,
        );
        assert!(result.is_none());

        // Should fail with too many spaces
        let source2 = "{  key: value}";
        let source_ctx2 = SourceContext::new(source2);
        let result2 = check_spaces_after_opening(
            source2,
            &source_ctx2,
            1,
            14,
            0,
            1,
            "test",
            &config,
            "braces",
            dummy_span,
        );
        assert!(result2.is_some());
    }

    #[test]
    fn test_check_spaces_before_closing() {
        use crate::{Location, SourceContext, Span};
        let dummy_span = Span::new(Location::new(1, 13, 12), Location::new(1, 14, 13));
        let source = "{key: value }";
        let source_ctx = SourceContext::new(source);
        let config = LintConfig::default();

        // Should pass with 1 space
        let result = check_spaces_before_closing(
            source,
            &source_ctx,
            1,
            12,
            0,
            1,
            "test",
            &config,
            "braces",
            dummy_span,
        );
        assert!(result.is_none());

        // Should fail with too many spaces
        let source2 = "{key: value  }";
        let source_ctx2 = SourceContext::new(source2);
        let result2 = check_spaces_before_closing(
            source2,
            &source_ctx2,
            1,
            13,
            0,
            1,
            "test",
            &config,
            "braces",
            dummy_span,
        );
        assert!(result2.is_some());
    }

    #[test]
    fn test_reversed_or_invalid_ranges_do_not_panic() {
        use crate::{Location, SourceContext, Span};
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 2, 1));
        let source = "{ é }";
        let ctx = SourceContext::new(source);
        let config = LintConfig::default();

        assert!(is_empty_collection(source, 5, 2));
        for (start, end) in [(5, 2), (3, 4), (4, 3)] {
            assert!(
                check_spaces_after_opening(source, &ctx, start, end, 0, 0, "t", &config, "b", span)
                    .is_none()
            );
            assert!(
                check_spaces_before_closing(
                    source, &ctx, start, end, 0, 0, "t", &config, "b", span
                )
                .is_none()
            );
        }
    }

    #[test]
    fn test_empty_range_reports_min_spaces_after_opening() {
        use crate::{Location, SourceContext, Span};
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 2, 1));
        let source = "{}";
        let ctx = SourceContext::new(source);
        let config = LintConfig::default();

        let diag =
            check_spaces_after_opening(source, &ctx, 1, 1, 1, -1, "t", &config, "braces", span);
        assert!(diag.is_some());
        assert!(
            check_spaces_before_closing(source, &ctx, 1, 1, 1, -1, "t", &config, "braces", span)
                .is_none()
        );
    }

    #[test]
    fn test_pair_delimiters_nested_and_unmatched() {
        use crate::{
            SourceContext,
            tokenizer::{FlowTokenizer, TokenType},
        };
        let yaml = "{a: {b: c}}\n{d: e}";
        let ctx = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &ctx);
        let opens = tokenizer.find_all(TokenType::BraceOpen);
        let closes = tokenizer.find_all(TokenType::BraceClose);
        let offsets: Vec<_> = pair_delimiters(&opens, &closes)
            .iter()
            .map(|(o, c)| (o.span.start.offset, c.span.start.offset))
            .collect();
        assert_eq!(offsets, [(0, 10), (4, 9), (12, 17)]);

        assert!(pair_delimiters(&opens, &[]).is_empty());
        assert!(pair_delimiters(&[], &closes).is_empty());
        assert!(pair_delimiters(&opens[2..], &closes[..1]).is_empty());
    }
}
