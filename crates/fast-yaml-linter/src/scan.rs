//! Single-pass scan of the parser events: comments and document markers.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ops::RangeInclusive;

use fast_yaml_core::events::{Event, EventItem, ScalarStyle};
use fast_yaml_core::limits::{ParseLimits, StreamBudget};
use fast_yaml_core::{
    CommentScanner, DuplicateMergeKeys, LoadOptions, NodeRole, NormalizedInput, Parser, Value,
    resolve_scalar,
};

use crate::{Location, SourceContext, Span, comments::Comment};

/// How a document starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentStart {
    /// With a `---` marker at this span.
    Explicit(Span),
    /// Without a marker; the span is the first token of the document.
    Implicit(Span),
}

impl DocumentStart {
    /// The span of the explicit `---`, if any.
    pub const fn marker(self) -> Option<Span> {
        match self {
            Self::Explicit(span) => Some(span),
            Self::Implicit(_) => None,
        }
    }
}

/// Explicit markers and first line of one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentMarkers {
    /// How the document starts.
    pub start: DocumentStart,
    /// Span of the explicit `...`, `None` when the document end is implicit.
    pub end: Option<Span>,
    /// 1-based line where a forward key search for the document begins.
    pub first_line: usize,
}

/// Stand-in for the document of a source the parser reports no document for.
pub const IMPLICIT_DOCUMENT: DocumentMarkers = DocumentMarkers {
    start: DocumentStart::Implicit(Span::new(Location::new(1, 1, 0), Location::new(1, 1, 0))),
    end: None,
    first_line: 1,
};

/// Which kind of key a [`KeyRepeat`] repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatedKey {
    /// An ordinary key; keys are equal when their resolved values are.
    Ordinary,
    /// A `<<` merge key.
    Merge,
}

/// A mapping key that repeats an earlier key of the same mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyRepeat {
    /// The key as written.
    pub key: String,
    /// 1-indexed line of the first occurrence.
    pub first_line: usize,
    /// Span of the repeating key.
    pub span: Span,
    /// Whether it is an ordinary or a merge key.
    pub kind: RepeatedKey,
}

/// Everything the rules need from the parser events of one source.
#[derive(Debug, Default)]
pub struct SourceScan<'a> {
    pub comments: Vec<Comment<'a>>,
    pub documents: Vec<DocumentMarkers>,
    pub key_repeats: Vec<KeyRepeat>,
    /// Content lines of every literal and folded scalar, in source order.
    pub block_scalars: Vec<RangeInclusive<usize>>,
}

/// Keys seen so far in one open mapping, with the 1-indexed line of their first occurrence.
#[derive(Default)]
struct MappingKeys {
    values: HashMap<Value, usize>,
    merge_first_line: Option<usize>,
}

impl MappingKeys {
    /// Records `key` and returns the first line it was seen on if it repeats.
    fn record(&mut self, kind: RepeatedKey, key: Value, line: usize) -> Option<usize> {
        if kind == RepeatedKey::Merge {
            let first = self.merge_first_line;
            self.merge_first_line.get_or_insert(line);
            return first;
        }
        match self.values.entry(key) {
            Entry::Occupied(first) => Some(*first.get()),
            Entry::Vacant(slot) => {
                slot.insert(line);
                None
            }
        }
    }
}

impl<'a> SourceScan<'a> {
    /// Scans `source` with a loader pass that discards the values.
    ///
    /// It applies the default parse limits, and treats a repeated `<<` like `Linter::lint` does.
    /// Empty when `source` is not its own normalized form (a BOM prefix), because the parser then
    /// sees a different text than the context. When the parse fails only the key repeats found
    /// before the error remain.
    pub fn of_source(source: &'a str, context: &SourceContext<'_>) -> Self {
        let Ok(input) = NormalizedInput::new(source) else {
            return Self::default();
        };
        if input.as_str().len() != source.len() {
            return Self::default();
        }
        let mut collector = ScanCollector::new(&input, source, context);
        let options = LoadOptions::new().with_duplicate_merge_keys(DuplicateMergeKeys::LastWins);
        let loaded = Parser::parse_normalized_observed(
            &input,
            &StreamBudget::new(ParseLimits::default()),
            options,
            |item| collector.observe(item),
        );
        if loaded.is_err() {
            return collector.finish_failed();
        }
        collector.finish()
    }
}

/// Feeds parser events into the comment scanner and the document marker list.
pub struct ScanCollector<'a, 'c> {
    source: &'a str,
    context: &'c SourceContext<'c>,
    scanner: CommentScanner,
    documents: Vec<DocumentMarkers>,
    open: Option<(DocumentStart, usize)>,
    mappings: Vec<MappingKeys>,
    key_repeats: Vec<KeyRepeat>,
    block_scalars: Vec<RangeInclusive<usize>>,
}

impl<'a, 'c> ScanCollector<'a, 'c> {
    /// `input` must be the normalized text whose events will be observed, and `source` its text.
    pub fn new(
        input: &NormalizedInput<'_>,
        source: &'a str,
        context: &'c SourceContext<'c>,
    ) -> Self {
        Self {
            source,
            context,
            scanner: CommentScanner::new(input),
            documents: Vec::new(),
            open: None,
            mappings: Vec::new(),
            key_repeats: Vec::new(),
            block_scalars: Vec::new(),
        }
    }

    pub fn observe(&mut self, item: &EventItem<'_>) {
        let _ = self.scanner.observe(item);
        match &item.event {
            Event::DocumentStart { explicit } => {
                let span = self.context.span_between(item.at, item.end);
                let first_line = match (self.documents.is_empty(), explicit) {
                    (true, _) => 1,
                    (false, true) => span.start.line + 1,
                    (false, false) => span.start.line,
                };
                let start = if *explicit {
                    DocumentStart::Explicit(span)
                } else {
                    DocumentStart::Implicit(span)
                };
                self.open = Some((start, first_line));
            }
            Event::DocumentEnd => {
                let range = self.context.byte_range_between(item.at, item.end);
                let explicit = self
                    .source
                    .get(range.start().get()..range.end().get())
                    .is_some_and(|text| text == "...");
                let Some((start, first_line)) = self.open.take() else {
                    return;
                };
                self.documents.push(DocumentMarkers {
                    start,
                    end: explicit.then(|| self.context.span_between(item.at, item.end)),
                    first_line,
                });
            }
            Event::MappingStart { .. } => self.mappings.push(MappingKeys::default()),
            Event::MappingEnd => {
                self.mappings.pop();
            }
            Event::Scalar {
                style: ScalarStyle::Literal | ScalarStyle::Folded,
                ..
            } => {
                // The token starts at its content and ends on the line that stopped it
                let last = item.end.line.saturating_sub(1);
                if last >= item.at.line {
                    self.block_scalars.push(item.at.line..=last);
                }
            }
            Event::Scalar {
                value, style, tag, ..
            } => {
                let kind = match item.role {
                    Some(NodeRole::Key) => RepeatedKey::Ordinary,
                    Some(NodeRole::MergeKey) => RepeatedKey::Merge,
                    _ => return,
                };
                let Some(keys) = self.mappings.last_mut() else {
                    return;
                };
                let resolved = Value::from(resolve_scalar(value, *style, tag.as_ref()));
                if let Some(first_line) = keys.record(kind, resolved, item.at.line) {
                    self.key_repeats.push(KeyRepeat {
                        key: value.as_ref().to_owned(),
                        first_line,
                        span: self.context.span_between(item.at, item.end),
                        kind,
                    });
                }
            }
            _ => {}
        }
    }

    /// The scan of a source whose parse failed: only the key repeats seen before the error.
    pub fn finish_failed(self) -> SourceScan<'a> {
        SourceScan {
            key_repeats: self.key_repeats,
            ..SourceScan::default()
        }
    }

    pub fn finish(self) -> SourceScan<'a> {
        let comments = self
            .scanner
            .finish()
            .into_iter()
            .filter_map(|range| Comment::from_range(self.source, self.context, range))
            .collect();
        SourceScan {
            comments,
            documents: self.documents,
            key_repeats: self.key_repeats,
            block_scalars: self.block_scalars,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{CommentKind, LintContext};

    fn markers(source: &str) -> Vec<(Option<usize>, Option<usize>, usize)> {
        LintContext::new(source)
            .documents()
            .iter()
            .map(|d| {
                (
                    d.start.marker().map(|s| s.start.line),
                    d.end.map(|s| s.start.line),
                    d.first_line,
                )
            })
            .collect()
    }

    #[test]
    fn implicit_end_before_next_marker_is_not_explicit() {
        assert_eq!(
            markers("a: 1\n---\nb: 2\n"),
            [(None, None, 1), (Some(2), None, 3)]
        );
    }

    #[test]
    fn explicit_end_marker_is_detected() {
        assert_eq!(
            markers("a: 1\n...\n---\nb: 2\n...\n"),
            [(None, Some(2), 1), (Some(3), Some(5), 4)]
        );
    }

    #[test]
    fn marker_with_comment_or_tag_on_its_line() {
        assert_eq!(
            markers("--- # c\na\n... # e\n--- !t x\n"),
            [(Some(1), Some(3), 1), (Some(4), None, 5)]
        );
    }

    #[test]
    fn directive_before_marker() {
        assert_eq!(markers("%YAML 1.2\n---\na\n...\n"), [(Some(2), Some(4), 1)]);
    }

    #[test]
    fn implicit_document_after_end_marker_starts_at_its_content() {
        assert_eq!(
            markers("a: 1\n...\nb: 2\n"),
            [(None, Some(2), 1), (None, None, 3)]
        );
    }

    #[test]
    fn comment_only_source_has_no_documents() {
        assert!(markers("# only a comment\n").is_empty());
        assert!(markers("").is_empty());
    }

    #[test]
    fn comment_kinds() {
        let context = LintContext::new("#!shebang\n# top\nkey: 1  # inline\n  # indented\n");
        let kinds: Vec<_> = context.comments().iter().map(|c| c.kind).collect();
        assert_eq!(
            kinds,
            [
                CommentKind::Shebang,
                CommentKind::FullLine,
                CommentKind::Inline,
                CommentKind::FullLine
            ]
        );
    }

    #[test]
    fn hash_inside_scalars_is_not_a_comment() {
        let context =
            LintContext::new("a: \"one\n  # two\n  three\"\nb: |\n  # four\n  five\nc: 1 # six\n");
        let texts: Vec<_> = context.comments().iter().map(|c| c.text).collect();
        assert_eq!(texts, [" six"]);
    }

    #[test]
    fn invalid_or_unnormalized_source_has_no_comments() {
        assert!(LintContext::new("a: [\n# c\n").comments().is_empty());
        assert!(
            LintContext::new("\u{FEFF}# c\na: 1\n")
                .comments()
                .is_empty()
        );
    }
}
