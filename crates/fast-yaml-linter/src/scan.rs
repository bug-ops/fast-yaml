//! Single-pass scan of the parser events: comments and document markers.

use fast_yaml_core::events::{Event, EventItem};
use fast_yaml_core::limits::{ParseLimits, StreamBudget};
use fast_yaml_core::{CommentScanner, DuplicateMergeKeys, LoadOptions, NormalizedInput, Parser};

use crate::{SourceContext, Span, comments::Comment};

/// Explicit markers and first line of one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentMarkers {
    /// Span of the explicit `---`, `None` when the document start is implicit.
    pub start: Option<Span>,
    /// Span of the explicit `...`, `None` when the document end is implicit.
    pub end: Option<Span>,
    /// 1-based line where a forward key search for the document begins.
    pub first_line: usize,
}

/// Everything the rules need from the parser events of one source.
#[derive(Debug, Default)]
pub struct SourceScan<'a> {
    pub comments: Vec<Comment<'a>>,
    pub documents: Vec<DocumentMarkers>,
}

impl<'a> SourceScan<'a> {
    /// Scans `source` with a loader pass that discards the values.
    ///
    /// It applies the default parse limits, and treats a repeated `<<` like `Linter::lint` does.
    /// Empty when `source` is not valid YAML, or is not its own normalized form (a BOM prefix),
    /// because the parser then sees a different text than the context.
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
            return Self::default();
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
    open: Option<(Option<Span>, usize)>,
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
                self.open = Some((explicit.then_some(span), first_line));
            }
            Event::DocumentEnd => {
                let range = self.context.byte_range_between(item.at, item.end);
                let explicit = self
                    .source
                    .get(range.start().get()..range.end().get())
                    .is_some_and(|text| text == "...");
                let (start, first_line) = self.open.take().unwrap_or((None, 1));
                self.documents.push(DocumentMarkers {
                    start,
                    end: explicit.then(|| self.context.span_between(item.at, item.end)),
                    first_line,
                });
            }
            _ => {}
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
                    d.start.map(|s| s.start.line),
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
