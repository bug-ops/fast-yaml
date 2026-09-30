//! Event-based `<<` merge value validation shared by the loader and the streaming formatter.
//!
//! Validating on the event stream, before the loader collapses equal keys, makes every entry
//! point report the same error: the first invalid merge value in document order, positioned at
//! its `<<` key.

use std::collections::HashMap;

use saphyr_parser::{Event, Span};

use crate::error::{ParseError, SourcePosition};
use crate::merge::{MergeError, MergeKeyTracker, NodeRole, is_core_set_tag};

/// What a finished node is, as far as merge validation cares.
#[derive(Debug, Clone, Copy)]
enum NodeKind {
    Mapping,
    Set,
    Scalar,
    /// Sequence, with the verdict of its items taken as merge sources.
    Sequence(Result<(), MergeError>),
}

impl NodeKind {
    const fn as_merge_value(self) -> Result<(), MergeError> {
        match self {
            Self::Mapping => Ok(()),
            Self::Set => Err(MergeError::SetSource),
            Self::Scalar => Err(MergeError::NotMapping),
            Self::Sequence(items) => items,
        }
    }

    const fn as_merge_item(self) -> Result<(), MergeError> {
        match self {
            Self::Sequence(_) => Err(MergeError::NotMapping),
            other => other.as_merge_value(),
        }
    }
}

enum OpenKind {
    /// Mapping; `set` marks a `!!set`, `merge_key` the span of a just-seen `<<` key.
    Mapping {
        set: bool,
        merge_key: Option<Span>,
    },
    Sequence(Result<(), MergeError>),
}

struct Open {
    kind: OpenKind,
    role: NodeRole,
    anchor: usize,
    span: Span,
}

/// Rejects `<<` values that cannot be merged, from the parser event stream.
#[derive(Default)]
pub struct MergeKeyValidator {
    tracker: MergeKeyTracker,
    open: Vec<Open>,
    anchors: HashMap<usize, NodeKind>,
    documents: usize,
}

impl MergeKeyValidator {
    /// Feeds the next event with its span.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Merge`] at the `<<` key of the first invalid merge value.
    pub fn observe(&mut self, event: &Event<'_>, span: Span) -> Result<(), ParseError> {
        let role = self.tracker.observe(event);
        let Some(role) = role else {
            return self.observe_structure(event);
        };
        match event {
            Event::Scalar(_, _, anchor, _) => self.settle(role, NodeKind::Scalar, *anchor, span),
            Event::Alias(id) => {
                // An alias to an undefined anchor is the parser's error to report
                let kind = self.anchors.get(id).copied().unwrap_or(NodeKind::Mapping);
                self.settle(role, kind, 0, span)
            }
            Event::MappingStart(anchor, tag) => {
                self.reserve_anchor(*anchor);
                let set = tag.as_ref().is_some_and(|t| is_core_set_tag(t));
                self.open.push(Open {
                    kind: OpenKind::Mapping {
                        set,
                        merge_key: None,
                    },
                    role,
                    anchor: *anchor,
                    span,
                });
                Ok(())
            }
            Event::SequenceStart(anchor, _) => {
                self.reserve_anchor(*anchor);
                self.open.push(Open {
                    kind: OpenKind::Sequence(Ok(())),
                    role,
                    anchor: *anchor,
                    span,
                });
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// An alias to a still-open node resolves to a non-mapping, like the loader does.
    fn reserve_anchor(&mut self, anchor: usize) {
        if anchor > 0 {
            self.anchors.insert(anchor, NodeKind::Scalar);
        }
    }

    fn observe_structure(&mut self, event: &Event<'_>) -> Result<(), ParseError> {
        match event {
            Event::DocumentStart(_) => {
                self.documents += 1;
                self.open.clear();
                self.anchors.clear();
                Ok(())
            }
            Event::MappingEnd | Event::SequenceEnd => {
                let Some(Open {
                    kind,
                    role,
                    anchor,
                    span,
                }) = self.open.pop()
                else {
                    return Ok(());
                };
                let kind = match kind {
                    OpenKind::Mapping { set: true, .. } => NodeKind::Set,
                    OpenKind::Mapping { .. } => NodeKind::Mapping,
                    OpenKind::Sequence(items) => NodeKind::Sequence(items),
                };
                self.settle(role, kind, anchor, span)
            }
            _ => Ok(()),
        }
    }

    /// Hands a finished node to its parent, checking it when it is a merge value.
    fn settle(
        &mut self,
        role: NodeRole,
        kind: NodeKind,
        anchor: usize,
        span: Span,
    ) -> Result<(), ParseError> {
        if anchor > 0 {
            self.anchors.insert(anchor, kind);
        }
        match (role, self.open.last_mut().map(|open| &mut open.kind)) {
            (NodeRole::Item, Some(OpenKind::Sequence(verdict))) => {
                if verdict.is_ok() {
                    *verdict = kind.as_merge_item();
                }
            }
            (
                NodeRole::MergeKey,
                Some(OpenKind::Mapping {
                    set: false,
                    merge_key,
                }),
            ) => {
                *merge_key = Some(span);
            }
            (NodeRole::Value, Some(OpenKind::Mapping { merge_key, .. })) => {
                if let Some(key) = merge_key.take() {
                    let SourcePosition { line, column } = key.into();
                    kind.as_merge_value().map_err(|error| ParseError::Merge {
                        error,
                        line,
                        column,
                        document: self.documents.saturating_sub(1),
                    })?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saphyr_parser::Parser;

    fn validate(yaml: &str) -> Result<(), ParseError> {
        let mut validator = MergeKeyValidator::default();
        for event in Parser::new_from_str(yaml) {
            let (event, span) = event.unwrap();
            validator.observe(&event, span)?;
        }
        Ok(())
    }

    fn rejected(yaml: &str) -> Option<(MergeError, usize, usize, usize)> {
        match validate(yaml) {
            Err(ParseError::Merge {
                error,
                line,
                column,
                document,
            }) => Some((error, line, column, document)),
            _ => None,
        }
    }

    #[test]
    fn accepts_valid_merge_values() {
        for yaml in [
            "m: {<<: {a: 1}}",
            "a: &a {x: 1}\nm:\n  <<: *a",
            "a: &a {x: 1}\nb: &b {y: 2}\nm:\n  <<: [*a, *b]",
            "m:\n  <<: [{a: 1}, {b: 2}]",
            "m:\n  <<: !!map {a: 1}",
            "m: {'<<': 1}",
            "m: {!!str <<: 1}",
            "m: !!set {<<: , a: }",
            "a: &a [{x: 1}]\nm:\n  <<: *a",
        ] {
            assert!(validate(yaml).is_ok(), "{yaml}");
        }
    }

    #[test]
    fn rejects_invalid_merge_values_at_the_key() {
        for (yaml, kind, line, column) in [
            ("m:\n  <<: 1\n", MergeError::NotMapping, 2, 3),
            ("m: {<<: }", MergeError::NotMapping, 1, 5),
            ("m:\n  <<: !!set {a}", MergeError::SetSource, 2, 3),
            ("a: &a [1]\nm:\n  <<: *a", MergeError::NotMapping, 3, 3),
            ("a: &a 1\nm:\n  <<: *a", MergeError::NotMapping, 3, 3),
            ("s: &s !!set {a}\nm:\n  <<: *s", MergeError::SetSource, 3, 3),
            (
                "s: &s !!set {a}\nm:\n  <<: [*s]",
                MergeError::SetSource,
                3,
                3,
            ),
            ("m:\n  <<: [[{a: 1}]]", MergeError::NotMapping, 2, 3),
            ("m:\n  <<: [{a: 1}, 2]", MergeError::NotMapping, 2, 3),
        ] {
            let (got, got_line, got_column, _) = rejected(yaml).unwrap_or_else(|| panic!("{yaml}"));
            assert_eq!(got, kind, "{yaml}");
            assert_eq!((got_line, got_column), (line, column), "{yaml}");
        }
    }

    const PARITY_CORPUS: &[&str] = &[
        "<<: 1\n",
        "m: &a {<<: *a}\n",
        "m: &a [{<<: *a}]\n",
        "m:\n  <<: null\n",
        "m:\n  <<: ~\n",
        "m:\n  <<: !!str x\n",
        "m:\n  <<: !!seq [{a: 1}]\n",
        "m:\n  <<: !custom {a: 1}\n",
        "m:\n  <<: !custom 5\n",
        "m:\n  <<: |\n    text\n",
        "? <<\n: 1\n",
        "m:\n  <<: *undefined\n",
        "a: &a {x: 1}\nb: &b [[*a]]\nm:\n  <<: *b\n",
        "a: &a {x: 1}\nb: &b [*a]\nc: &c [*b]\nm:\n  <<: *c\n",
        "a: &a {x: 1}\nb: &b [*a]\nm:\n  <<: *b\n",
        "m:\n  <<: {x: 1}\n  <<: 2\n",
        "m:\n  <<: 2\n  <<: {x: 1}\n",
        "- <<: 1\n",
        "- <<: {x: 1}\n",
        "x: 1\n---\na: 2\n---\nm: {<<: 1}\n",
        "a: &a [1]\nm:\n  <<: *a\n",
        "a: &a 1\nm:\n  <<: *a\n",
        "a: &a {x: 1}\nm:\n  <<: *a\n",
        "a: &a {x: 1}\nb: &b {y: 2}\nm:\n  <<: [*a, *b]\n",
        "a: &a [{x: 1}]\nm:\n  <<: [*a]\n",
        "a: &a [{x: 1}]\nm:\n  <<: *a\n",
        "s: &s !!set {a}\nm:\n  <<: *s\n",
        "s: &s !!set {a}\nm:\n  <<: [{x: 1}, *s]\n",
        "m:\n  <<: [{x: 1}, 5]\n",
        "m:\n  <<: [[{x: 1}]]\n",
        "m:\n  <<: !!map {x: 1}\n",
        "m:\n  '<<': 1\n  \"<<\": 2\n  !!str <<: 3\n",
        "s: !!set {<<: , a: }\n",
        "m:\n  <<:\n  k: 1\n",
        "m: {&k <<: {x: 1}, *k : 1}\n",
        "m: {&k <<: {x: 1}, *k : {y: 2}}\n",
        "- <<: {x: 1}\n- <<: [ ]\n",
    ];

    #[test]
    fn formatter_accepts_exactly_what_parse_all_accepts() {
        use crate::streaming::format_streaming;
        use crate::{EmitterConfig, Parser};

        let config = EmitterConfig::default();
        for yaml in PARITY_CORPUS {
            let parsed = Parser::parse_all(yaml);
            let formatted = format_streaming(yaml, &config);
            assert_eq!(parsed.is_ok(), formatted.is_ok(), "{yaml}: {formatted:?}");
            #[cfg(feature = "arena")]
            assert_eq!(
                parsed.is_ok(),
                crate::streaming::format_streaming_arena(yaml, &config).is_ok(),
                "{yaml}"
            );
            if let (Err(parse), Err(crate::EmitError::Parse(emit))) = (parsed, formatted) {
                assert_eq!(parse.to_string(), emit.to_string(), "{yaml}");
            }
        }
    }

    #[test]
    fn reports_the_document_of_the_error() {
        let (.., document) = rejected("a: 1\n---\nb: 2\n---\nm: {<<: 1}\n").unwrap();
        assert_eq!(document, 2);
    }

    #[test]
    fn anchored_merge_key_alias_is_a_merge_key() {
        assert!(rejected("a: {&k <<: {x: 1}, *k : 1}").is_some());
    }
}
