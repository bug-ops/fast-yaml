//! Common utilities for flow collection rules (braces, brackets).

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::{
    Finding, LintContext, Span,
    config::{
        BoolOrName, EmptyInsideLimit, Limit, RuleOptions, RuleSettings, deserialize_bool_or_name,
    },
    tokenizer::{Token, TokenType},
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
    kind: FlowCollection,
) -> Vec<Finding> {
    let source = context.source();
    let tokenizer = context.flow_tokenizer();
    let options = &settings.options;

    let opens = tokenizer.find_all(kind.open());
    let closes = tokenizer.find_all(kind.close());
    let pairs = pair_delimiters(&opens, &closes);
    let mut diagnostics = Vec::new();

    match options.forbid {
        Forbid::All => {
            let message = format!("{} forbidden (forbid: all)", kind.noun());
            for token in &opens {
                diagnostics.push(Finding::new(message.clone(), token.span));
            }
            return diagnostics;
        }
        Forbid::NonEmpty => {
            let message = format!("non-empty {} forbidden (forbid: non-empty)", kind.noun());
            for (open, close) in &pairs {
                if !is_empty_collection(source, open.span.end.offset, close.span.start.offset) {
                    diagnostics.push(Finding::new(message.clone(), open.span));
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

        let spacing = FlowSpacing {
            source,
            inner: open.span.end.offset..close.span.start.offset,
            min: min_spaces,
            max: max_spaces,
            kind,
        };
        diagnostics.extend(spacing.after_opening(open.span));
        diagnostics.extend(spacing.before_closing(close.span));
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

/// Spacing limits for the interior of one flow collection.
pub(crate) struct FlowSpacing<'a> {
    pub(crate) source: &'a str,
    /// Byte range between the opening and the closing delimiter.
    pub(crate) inner: Range<usize>,
    pub(crate) min: Limit,
    pub(crate) max: Limit,
    pub(crate) kind: FlowCollection,
}

impl FlowSpacing<'_> {
    /// Checks the spaces right after the opening delimiter.
    pub(crate) fn after_opening(&self, opening_span: Span) -> Option<Finding> {
        let end = self.inner.end.min(self.source.len());
        if self.inner.start > end {
            return None;
        }

        let content = self.source.get(self.inner.start..end)?;
        let rest = content.trim_start_matches(' ');
        if rest.starts_with(['\n', '\r', '#']) {
            return None;
        }
        let spaces = content.len() - rest.len();
        self.violation(spaces, opening_span)
    }

    /// Checks the spaces right before the closing delimiter.
    pub(crate) fn before_closing(&self, closing_span: Span) -> Option<Finding> {
        let end = self.inner.end.min(self.source.len());
        if self.inner.start >= end {
            return None;
        }

        let content = self.source.get(self.inner.start..end)?;
        let trimmed = content.trim_end_matches(' ');
        if trimmed.ends_with('\n') {
            return None;
        }
        let spaces = content.len() - trimmed.len();
        self.violation(spaces, closing_span)
    }

    fn violation(&self, spaces: usize, span: Span) -> Option<Finding> {
        let (problem, bound, limit) = if self.min.unmet_by(spaces) {
            ("few", "least", self.min)
        } else if self.max.exceeded_by(spaces) {
            ("many", "most", self.max)
        } else {
            return None;
        };
        let name = self.kind.name();
        Some(Finding::new(
            format!(
                "too {problem} spaces inside {name} (expected at {bound} {limit}, found {spaces})"
            ),
            span,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Location;

    #[test]
    fn test_is_empty_collection() {
        assert!(is_empty_collection("{}", 1, 1));
        assert!(is_empty_collection("{  }", 1, 3));
        assert!(is_empty_collection("{\n}", 1, 2));
        assert!(!is_empty_collection("{a}", 1, 2));
        assert!(!is_empty_collection("{ key: value }", 1, 13));
    }

    fn spacing(source: &str, inner: Range<usize>, min: Limit, max: Limit) -> FlowSpacing<'_> {
        FlowSpacing {
            source,
            inner,
            min,
            max,
            kind: FlowCollection::Mapping,
        }
    }

    fn dummy_span() -> Span {
        Span::new(Location::new(1, 1, 0), Location::new(1, 2, 1))
    }

    #[test]
    fn test_pair_delimiters_pairs_nested_braces_in_opener_order() {
        use crate::tokenizer::TokenType;
        let yaml = "{a: {b: c}}";
        let ctx = LintContext::new(yaml);
        let tokenizer = ctx.flow_tokenizer();
        let opens = tokenizer.find_all(TokenType::BraceOpen);
        let closes = tokenizer.find_all(TokenType::BraceClose);

        let pairs = pair_delimiters(&opens, &closes);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].1.span.start.offset, 10);
    }

    #[test]
    fn test_check_spaces_after_opening() {
        let source = "{ key: value}";
        let ok = spacing(source, 1..13, Limit::Max(0), Limit::Max(1));
        assert!(ok.after_opening(dummy_span()).is_none());

        let source2 = "{  key: value}";
        let bad = spacing(source2, 1..14, Limit::Max(0), Limit::Max(1));
        let diag = bad.after_opening(dummy_span()).unwrap();
        assert_eq!(
            diag.message(),
            "too many spaces inside braces (expected at most 1, found 2)"
        );
    }

    #[test]
    fn test_check_spaces_before_closing() {
        let source = "{key: value }";
        let ok = spacing(source, 1..12, Limit::Max(0), Limit::Max(1));
        assert!(ok.before_closing(dummy_span()).is_none());

        let source2 = "{key: value  }";
        let bad = spacing(source2, 1..13, Limit::Max(0), Limit::Max(1));
        assert!(bad.before_closing(dummy_span()).is_some());
    }

    #[test]
    fn test_spacing_ignores_indentation_across_lines() {
        let source = "{\n  a: 1\n  }";
        let s = spacing(source, 1..source.len() - 1, Limit::Max(0), Limit::Max(0));
        assert!(s.before_closing(dummy_span()).is_none());

        let source = "{ \n a }";
        let s = spacing(source, 1..source.len() - 1, Limit::Max(0), Limit::Max(0));
        assert!(s.after_opening(dummy_span()).is_none());

        let source = "{ # c\n a }";
        let s = spacing(source, 1..source.len() - 1, Limit::Max(0), Limit::Max(0));
        assert!(s.after_opening(dummy_span()).is_none());
    }

    #[test]
    fn test_reversed_or_invalid_ranges_do_not_panic() {
        let source = "{ é }";

        assert!(is_empty_collection(source, 5, 2));
        for (start, end) in [(5, 2), (3, 4), (4, 3)] {
            let s = spacing(source, start..end, Limit::Max(0), Limit::Max(0));
            assert!(s.after_opening(dummy_span()).is_none());
            assert!(s.before_closing(dummy_span()).is_none());
        }
    }

    #[test]
    fn test_empty_range_reports_min_spaces_after_opening() {
        let source = "{}";
        let s = spacing(source, 1..1, Limit::Max(1), Limit::Disabled);

        let diag = s.after_opening(dummy_span()).unwrap();
        assert_eq!(
            diag.message(),
            "too few spaces inside braces (expected at least 1, found 0)"
        );
        assert!(s.before_closing(dummy_span()).is_none());
    }

    #[test]
    fn test_pair_delimiters_nested_and_unmatched() {
        use crate::{LintContext, tokenizer::TokenType};
        let ctx = LintContext::new("{a: {b: c}}\n{d: e}");
        let tokenizer = ctx.flow_tokenizer();
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
