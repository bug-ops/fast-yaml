//! Common utilities for flow collection rules (braces, brackets).

use serde::{Deserialize, Serialize};

use crate::{
    LintContext, Severity, SourceContext, Span,
    config::{
        BoolOrName, EmptyInsideLimit, Limit, RuleOptions, RuleSettings, deserialize_bool_or_name,
    },
    diagnostic::{Diagnostic, DiagnosticBuilder},
    tokenizer::{FlowTokenizer, Token, TokenType},
};

/// The two flow collection kinds checked by the braces and brackets rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlowCollection {
    /// Flow mapping `{}`.
    Mapping,
    /// Flow sequence `[]`.
    Sequence,
}

impl FlowCollection {
    const fn open(self) -> TokenType {
        match self {
            Self::Mapping => TokenType::BraceOpen,
            Self::Sequence => TokenType::BracketOpen,
        }
    }

    const fn close(self) -> TokenType {
        match self {
            Self::Mapping => TokenType::BraceClose,
            Self::Sequence => TokenType::BracketClose,
        }
    }

    const fn noun(self) -> &'static str {
        match self {
            Self::Mapping => "flow mapping",
            Self::Sequence => "flow sequence",
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Mapping => "braces",
            Self::Sequence => "brackets",
        }
    }
}

/// Value of the `forbid` option of the braces and brackets rules.
///
/// Accepts `false` or `no`, `true` (same as `all`), `non-empty` and `all`; `No` serializes as
/// `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Forbid {
    /// Flow collections are allowed.
    #[default]
    No,
    /// Only non-empty flow collections are forbidden.
    NonEmpty,
    /// All flow collections are forbidden.
    All,
}

impl BoolOrName for Forbid {
    const EXPECTING: &'static str = "a boolean, 'no', 'non-empty' or 'all'";

    fn from_bool(value: bool) -> Result<Self, &'static str> {
        Ok(if value { Self::All } else { Self::No })
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "no" => Some(Self::No),
            "non-empty" => Some(Self::NonEmpty),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

impl Serialize for Forbid {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::No => serializer.serialize_bool(false),
            Self::NonEmpty => serializer.serialize_str("non-empty"),
            Self::All => serializer.serialize_str("all"),
        }
    }
}

impl<'de> Deserialize<'de> for Forbid {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_bool_or_name(deserializer)
    }
}

/// Options shared by the braces and brackets rules.
///
/// A set `min-spaces-inside-empty` or `max-spaces-inside-empty` overrides the matching
/// non-empty limit for empty collections independently of the other one. A minimum above
/// the maximum is not rejected: as in yamllint it flags every collection.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::{EmptyInsideLimit, Limit};
/// use fast_yaml_linter::rules::FlowCollectionOptions;
///
/// let options = FlowCollectionOptions::default();
/// assert_eq!(options.max_spaces_inside, Limit::Max(0));
/// assert_eq!(options.max_spaces_inside_empty, EmptyInsideLimit::Inherit);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct FlowCollectionOptions {
    /// Which flow collections are forbidden.
    pub forbid: Forbid,
    /// Minimum spaces inside non-empty collections.
    pub min_spaces_inside: Limit,
    /// Maximum spaces inside non-empty collections.
    pub max_spaces_inside: Limit,
    /// Minimum spaces inside empty collections.
    pub min_spaces_inside_empty: EmptyInsideLimit,
    /// Maximum spaces inside empty collections.
    pub max_spaces_inside_empty: EmptyInsideLimit,
}

impl Default for FlowCollectionOptions {
    fn default() -> Self {
        Self {
            forbid: Forbid::No,
            min_spaces_inside: Limit::Max(0),
            max_spaces_inside: Limit::Max(0),
            min_spaces_inside_empty: EmptyInsideLimit::Inherit,
            max_spaces_inside_empty: EmptyInsideLimit::Inherit,
        }
    }
}

impl RuleOptions for FlowCollectionOptions {}

/// Runs the shared braces/brackets check for one kind of flow collection.
pub(crate) fn check_flow_collection(
    context: &LintContext,
    settings: &RuleSettings<FlowCollectionOptions>,
    code: &str,
    default_severity: Severity,
    kind: FlowCollection,
) -> Vec<Diagnostic> {
    let source = context.source();
    let source_context = context.source_context();
    let tokenizer = FlowTokenizer::new(source, source_context);
    let options = &settings.options;

    let opens = tokenizer.find_all(kind.open());
    let closes = tokenizer.find_all(kind.close());
    let pairs = pair_delimiters(&opens, &closes);
    let severity = settings.severity_or(default_severity);
    let mut diagnostics = Vec::new();

    match options.forbid {
        Forbid::All => {
            let message = format!("{} forbidden (forbid: all)", kind.noun());
            for token in &opens {
                diagnostics.push(
                    DiagnosticBuilder::new(code, severity, message.as_str(), token.span)
                        .build_with_context(source_context),
                );
            }
            return diagnostics;
        }
        Forbid::NonEmpty => {
            let message = format!("non-empty {} forbidden (forbid: non-empty)", kind.noun());
            for (open, close) in &pairs {
                if !is_empty_collection(source, open.span.end.offset, close.span.start.offset) {
                    diagnostics.push(
                        DiagnosticBuilder::new(code, severity, message.as_str(), open.span)
                            .build_with_context(source_context),
                    );
                }
            }
            return diagnostics;
        }
        Forbid::No => {}
    }

    for (open, close) in &pairs {
        let is_empty = is_empty_collection(source, open.span.end.offset, close.span.start.offset);

        let (min_spaces, max_spaces) = if is_empty {
            (
                options
                    .min_spaces_inside_empty
                    .resolve(options.min_spaces_inside),
                options
                    .max_spaces_inside_empty
                    .resolve(options.max_spaces_inside),
            )
        } else {
            (options.min_spaces_inside, options.max_spaces_inside)
        };

        diagnostics.extend(check_spaces_after_opening(
            source,
            source_context,
            open.span.end.offset,
            close.span.start.offset,
            min_spaces,
            max_spaces,
            code,
            severity,
            kind.name(),
            open.span,
        ));
        diagnostics.extend(check_spaces_before_closing(
            source,
            source_context,
            open.span.end.offset,
            close.span.start.offset,
            min_spaces,
            max_spaces,
            code,
            severity,
            kind.name(),
            close.span,
        ));
    }

    diagnostics
}

/// Pairs opening and closing delimiter tokens by nesting depth.
///
/// Both slices must be sorted by offset (as returned by `FlowTokenizer::find_all`).
/// Unmatched openers and closers are skipped. Pairs are returned in opener order.
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
        .filter_map(|(open_idx, close)| Some((opens.get(open_idx)?, close)))
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
/// * `min_spaces` - Minimum required spaces
/// * `max_spaces` - Maximum allowed spaces
/// * `code` - Rule code for diagnostics
/// * `severity` - Severity of the returned diagnostic
/// * `collection_name` - Name of collection type (e.g., "braces", "brackets")
///
/// Returns a diagnostic if spacing constraints are violated.
#[allow(clippy::too_many_arguments)]
pub fn check_spaces_after_opening(
    source: &str,
    source_ctx: &SourceContext<'_>,
    start_offset: usize,
    end_offset: usize,
    min_spaces: Limit,
    max_spaces: Limit,
    code: &str,
    severity: Severity,
    collection_name: &str,
    opening_span: Span,
) -> Option<Diagnostic> {
    let end = end_offset.min(source.len());
    if start_offset > end {
        return None;
    }

    let content = source.get(start_offset..end)?;
    let spaces = content.chars().take_while(|c| *c == ' ').count();

    if min_spaces.unmet_by(spaces) {
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

    if max_spaces.exceeded_by(spaces) {
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
/// * `min_spaces` - Minimum required spaces
/// * `max_spaces` - Maximum allowed spaces
/// * `code` - Rule code for diagnostics
/// * `severity` - Severity of the returned diagnostic
/// * `collection_name` - Name of collection type (e.g., "braces", "brackets")
///
/// Returns a diagnostic if spacing constraints are violated.
#[allow(clippy::too_many_arguments)]
pub fn check_spaces_before_closing(
    source: &str,
    source_ctx: &SourceContext<'_>,
    start_offset: usize,
    end_offset: usize,
    min_spaces: Limit,
    max_spaces: Limit,
    code: &str,
    severity: Severity,
    collection_name: &str,
    closing_span: Span,
) -> Option<Diagnostic> {
    let end = end_offset.min(source.len());
    if start_offset >= end {
        return None;
    }

    let content = source.get(start_offset..end)?;
    let spaces = content.chars().rev().take_while(|c| *c == ' ').count();

    if min_spaces.unmet_by(spaces) {
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

    if max_spaces.exceeded_by(spaces) {
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
    fn test_pair_delimiters_pairs_nested_braces_in_opener_order() {
        use crate::SourceContext;
        use crate::tokenizer::{FlowTokenizer, TokenType};
        let yaml = "{a: {b: c}}";
        let ctx = SourceContext::new(yaml);
        let tokenizer = FlowTokenizer::new(yaml, &ctx);
        let opens = tokenizer.find_all(TokenType::BraceOpen);
        let closes = tokenizer.find_all(TokenType::BraceClose);

        let pairs = pair_delimiters(&opens, &closes);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].1.span.start.offset, 10);
    }

    #[test]
    fn test_check_spaces_after_opening() {
        use crate::{Location, SourceContext, Span};
        let dummy_span = Span::new(Location::new(1, 1, 0), Location::new(1, 2, 1));
        let source = "{ key: value}";
        let source_ctx = SourceContext::new(source);

        // Should pass with 1 space
        let result = check_spaces_after_opening(
            source,
            &source_ctx,
            1,
            13,
            Limit::Max(0),
            Limit::Max(1),
            "test",
            Severity::Warning,
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
            Limit::Max(0),
            Limit::Max(1),
            "test",
            Severity::Warning,
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

        // Should pass with 1 space
        let result = check_spaces_before_closing(
            source,
            &source_ctx,
            1,
            12,
            Limit::Max(0),
            Limit::Max(1),
            "test",
            Severity::Warning,
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
            Limit::Max(0),
            Limit::Max(1),
            "test",
            Severity::Warning,
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

        assert!(is_empty_collection(source, 5, 2));
        for (start, end) in [(5, 2), (3, 4), (4, 3)] {
            assert!(
                check_spaces_after_opening(
                    source,
                    &ctx,
                    start,
                    end,
                    Limit::Max(0),
                    Limit::Max(0),
                    "t",
                    Severity::Warning,
                    "b",
                    span
                )
                .is_none()
            );
            assert!(
                check_spaces_before_closing(
                    source,
                    &ctx,
                    start,
                    end,
                    Limit::Max(0),
                    Limit::Max(0),
                    "t",
                    Severity::Warning,
                    "b",
                    span
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

        let diag = check_spaces_after_opening(
            source,
            &ctx,
            1,
            1,
            Limit::Max(1),
            Limit::Disabled,
            "t",
            Severity::Warning,
            "braces",
            span,
        );
        assert!(diag.is_some());
        assert!(
            check_spaces_before_closing(
                source,
                &ctx,
                1,
                1,
                Limit::Max(1),
                Limit::Disabled,
                "t",
                Severity::Warning,
                "braces",
                span
            )
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
