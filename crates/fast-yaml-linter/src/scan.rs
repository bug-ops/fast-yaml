//! Single-pass scan of the parser events: comments and document markers.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ops::RangeInclusive;

use fast_yaml_core::events::{AnchorId, Event, EventItem, ScalarStyle};
use fast_yaml_core::limits::{ParseLimits, StreamBudget};
use fast_yaml_core::{
    CommentScanner, DuplicateMergeKeys, LoadOptions, NodeRole, NormalizedInput, ParseError, Parser,
    SetValues, Value, resolve_scalar,
};

use crate::source::offset::{ByteOffset, ByteRange};
use crate::{Location, SourceContext, Span, comments::Comment};

/// How the linter loads a document: a repeated `<<` and a `!!set` member with a value load, so
/// the `duplicate-key` and `set-values` rules can report them instead of the load failing.
pub const fn lint_load_options() -> LoadOptions {
    LoadOptions::new()
        .with_duplicate_merge_keys(DuplicateMergeKeys::LastWins)
        .with_set_values(SetValues::Ignore)
}

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
    /// A `<<` merge key, written out or through an alias.
    Merge,
    /// An anchored collection used as a key, or an alias to one; collections are only equal
    /// through their anchor.
    Alias,
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
    /// Whether the parser pass completed; when not, only `key_repeats` is filled.
    pub complete: bool,
}

/// What a mapping key stands for when two keys of one mapping are compared.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum KeyIdentity {
    /// A scalar, or an alias to one, by its resolved value.
    Value(Value),
    /// An anchored collection, or an alias to it.
    Collection(AnchorId),
}

/// An anchored node as far as key comparison is concerned.
struct AnchoredKey {
    identity: KeyIdentity,
    /// The scalar text, empty for a collection.
    text: String,
}

/// Keys seen so far in one open mapping, with the 1-indexed line of their first occurrence.
#[derive(Default)]
struct MappingKeys {
    values: HashMap<KeyIdentity, usize>,
    merge_first_line: Option<usize>,
}

impl MappingKeys {
    /// Records `key` and returns the first line it was seen on if it repeats.
    fn record(&mut self, kind: RepeatedKey, key: KeyIdentity, line: usize) -> Option<usize> {
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
    /// Scans `source` with a loader pass that discards the values, under `limits`.
    ///
    /// A repeated `<<` is treated like `Linter::lint` does. A BOM-prefixed `source` is parsed in
    /// its normalized form and the positions are mapped back, so the scan agrees with the other
    /// rules on the text of `context`. When the pass fails the scan is incomplete (see
    /// [`SourceScan::complete`]), holds only the key repeats found before the error, and the
    /// error is returned.
    pub fn scan(
        source: &'a str,
        context: &SourceContext<'_>,
        limits: ParseLimits,
    ) -> (Self, Option<ParseError>) {
        let input = match NormalizedInput::new(source) {
            Ok(input) => input,
            Err(error) => return (Self::default(), Some(error)),
        };
        let mut collector = ScanCollector::new(&input, source, context);
        let loaded = Parser::parse_normalized_observed(
            &input,
            &StreamBudget::new(limits),
            lint_load_options(),
            |item| collector.observe(item),
        );
        match loaded {
            Ok(_) => (collector.finish(), None),
            Err(error) => (collector.finish_failed(), Some(error)),
        }
    }

    /// Lazy form of [`scan`](Self::scan) with the default limits for a context nobody scanned.
    ///
    /// The result is incomplete when the source does not parse.
    pub fn of_source(source: &'a str, context: &SourceContext<'_>) -> Self {
        Self::scan(source, context, ParseLimits::default()).0
    }
}

/// Positions of a parsed text that differs from the context's source (document-prefix BOMs
/// were removed), with the way back to the original bytes.
struct Remap<'n> {
    input: &'n NormalizedInput<'n>,
    context: SourceContext<'n>,
}

/// Feeds parser events into the comment scanner and the document marker list.
pub struct ScanCollector<'a, 'c, 'n> {
    source: &'a str,
    context: &'c SourceContext<'c>,
    remap: Option<Remap<'n>>,
    scanner: CommentScanner,
    documents: Vec<DocumentMarkers>,
    open: Option<(DocumentStart, usize)>,
    mappings: Vec<MappingKeys>,
    anchors: HashMap<AnchorId, AnchoredKey>,
    key_repeats: Vec<KeyRepeat>,
    block_scalars: Vec<RangeInclusive<usize>>,
}

impl<'a, 'c, 'n> ScanCollector<'a, 'c, 'n> {
    /// `input` must be the normalized form of `source`, whose events will be observed;
    /// `context` is the line table of `source`.
    pub fn new(
        input: &'n NormalizedInput<'n>,
        source: &'a str,
        context: &'c SourceContext<'c>,
    ) -> Self {
        let remap = (input.as_str().len() != source.len()).then(|| Remap {
            input,
            context: SourceContext::new(input.as_str()),
        });
        Self {
            source,
            context,
            remap,
            scanner: CommentScanner::new(input),
            documents: Vec::new(),
            open: None,
            mappings: Vec::new(),
            anchors: HashMap::new(),
            key_repeats: Vec::new(),
            block_scalars: Vec::new(),
        }
    }

    /// The byte range of an event in `source`.
    fn byte_range(&self, item: &EventItem<'_>) -> ByteRange {
        self.remap.as_ref().map_or_else(
            || self.context.byte_range_between(item.at, item.end),
            |remap| {
                let parsed = remap.context.byte_range_between(item.at, item.end);
                ByteRange::new(
                    ByteOffset::new(remap.input.original_offset(parsed.start().get())),
                    ByteOffset::new(remap.input.original_offset(parsed.end().get())),
                )
            },
        )
    }

    fn span(&self, item: &EventItem<'_>) -> Span {
        self.context.span_of_bytes(self.byte_range(item))
    }

    pub fn observe(&mut self, item: &EventItem<'_>) {
        let _ = self.scanner.observe(item);
        match &item.event {
            Event::DocumentStart { explicit } => {
                self.anchors.clear();
                let span = self.span(item);
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
                let range = self.byte_range(item);
                let explicit = self
                    .source
                    .get(range.start().get()..range.end().get())
                    .is_some_and(|text| text == "...");
                let Some((start, first_line)) = self.open.take() else {
                    return;
                };
                self.documents.push(DocumentMarkers {
                    start,
                    end: explicit.then(|| self.span(item)),
                    first_line,
                });
            }
            _ => self.observe_node(item),
        }
    }

    /// Tracks mappings, anchors and keys.
    fn observe_node(&mut self, item: &EventItem<'_>) {
        match &item.event {
            Event::MappingStart { anchor, .. } | Event::SequenceStart { anchor, .. } => {
                if let Some(id) = anchor {
                    self.anchors.insert(
                        *id,
                        AnchoredKey {
                            identity: KeyIdentity::Collection(*id),
                            text: String::new(),
                        },
                    );
                    if item.role == Some(NodeRole::Key) {
                        self.record_key(item, RepeatedKey::Alias, KeyIdentity::Collection(*id), "");
                    }
                }
                if matches!(item.event, Event::MappingStart { .. }) {
                    self.mappings.push(MappingKeys::default());
                }
            }
            Event::MappingEnd => {
                self.mappings.pop();
            }
            Event::Alias(id) => match item.role {
                Some(NodeRole::MergeKey) => {
                    self.record_key(item, RepeatedKey::Merge, KeyIdentity::Collection(*id), "<<");
                }
                Some(NodeRole::Key) => {
                    let Some(anchored) = self.anchors.get(id) else {
                        return;
                    };
                    let kind = match anchored.identity {
                        KeyIdentity::Collection(_) => RepeatedKey::Alias,
                        KeyIdentity::Value(_) => RepeatedKey::Ordinary,
                    };
                    let (identity, text) = (anchored.identity.clone(), anchored.text.clone());
                    self.record_key(item, kind, identity, &text);
                }
                _ => {}
            },
            Event::Scalar {
                value,
                style,
                anchor,
                tag,
            } => {
                if matches!(style, ScalarStyle::Literal | ScalarStyle::Folded) {
                    // The token starts at its content and ends on the line that stopped it
                    let last = item.end.line.saturating_sub(1);
                    if last >= item.at.line {
                        self.block_scalars.push(item.at.line..=last);
                    }
                }
                let key_kind = match item.role {
                    Some(NodeRole::Key) => Some(RepeatedKey::Ordinary),
                    Some(NodeRole::MergeKey) => Some(RepeatedKey::Merge),
                    _ => None,
                };
                if anchor.is_none() && key_kind.is_none() {
                    return;
                }
                let identity =
                    KeyIdentity::Value(Value::from(resolve_scalar(value, *style, tag.as_ref())));
                if let Some(id) = anchor {
                    self.anchors.insert(
                        *id,
                        AnchoredKey {
                            identity: identity.clone(),
                            text: value.as_ref().to_owned(),
                        },
                    );
                }
                if let Some(kind) = key_kind {
                    self.record_key(item, kind, identity, value);
                }
            }
            _ => {}
        }
    }

    /// Records a key of the innermost mapping and notes it when it repeats an earlier one.
    fn record_key(
        &mut self,
        item: &EventItem<'_>,
        kind: RepeatedKey,
        identity: KeyIdentity,
        text: &str,
    ) {
        let Some(keys) = self.mappings.last_mut() else {
            return;
        };
        if let Some(first_line) = keys.record(kind, identity, item.at.line) {
            let span = self.span(item);
            self.key_repeats.push(KeyRepeat {
                key: text.to_owned(),
                first_line,
                span,
                kind,
            });
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
            .filter_map(|range| {
                let range = match &self.remap {
                    None => range,
                    Some(remap) => {
                        remap.input.original_offset(range.start)
                            ..remap.input.original_offset(range.end)
                    }
                };
                Comment::from_range(self.source, self.context, range)
            })
            .collect();
        SourceScan {
            comments,
            documents: self.documents,
            key_repeats: self.key_repeats,
            block_scalars: self.block_scalars,
            complete: true,
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
        assert_eq!(markers("# only a comment\n"), []);
        assert_eq!(markers(""), []);
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
    fn invalid_source_has_no_comments_and_is_incomplete() {
        let context = LintContext::new("a: [\n# c\n");
        assert_eq!(context.comments(), []);
        assert!(!context.scan_is_complete());
    }

    #[test]
    fn bom_prefixed_context_keeps_comments_markers_and_repeats() {
        let source = "\u{FEFF}# c\n---\na: 1\na: 2\n";
        let context = LintContext::new(source);

        let comments = context.comments();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].text, " c");
        // Coordinates refer to the text of the context, the BOM included
        assert_eq!(comments[0].span.start.offset, 3);
        assert_eq!(
            &source[comments[0].span.start.offset..comments[0].span.end.offset],
            "# c"
        );

        let marker = context.documents()[0].start.marker().unwrap();
        assert_eq!((marker.start.line, marker.start.offset), (2, 7));

        let repeats = context.key_repeats();
        assert_eq!(repeats.len(), 1);
        assert_eq!(
            &source[repeats[0].span.start.offset..repeats[0].span.end.offset],
            "a"
        );
        assert_eq!(repeats[0].span.start.line, 4);
    }

    #[test]
    fn bom_in_a_later_document_prefix_is_mapped_back() {
        let source = "a: 1\n...\n\u{FEFF}# c\nb: 1\nb: 2\n";
        let context = LintContext::new(source);
        let comment = context.comments()[0];
        assert_eq!(
            &source[comment.span.start.offset..comment.span.end.offset],
            "# c"
        );
        let repeat = &context.key_repeats()[0];
        assert_eq!(
            &source[repeat.span.start.offset..repeat.span.end.offset],
            "b"
        );
    }
}
