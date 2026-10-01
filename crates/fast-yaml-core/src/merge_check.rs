//! Event-based `<<` merge and `!!set` validation shared by the loader and the streaming formatter.
//!
//! Validating on the event stream, before the loader collapses equal keys, makes every entry
//! point report the same error: the first invalid merge value, repeated `<<` key or set member
//! value in document order, positioned at its key.

use std::collections::HashMap;

use saphyr_parser::{Event, Span};

use crate::error::{ParseError, SourcePosition};
use crate::events::ScalarStyle;
use crate::merge::{MergeError, MergeKeyTracker, NodeRole, is_core_set_tag};
use crate::options::{DuplicateMergeKeys, LoadOptions, SetValues};
use crate::scalar::{ResolvedScalar, resolve_scalar_raw};

/// What a finished node is, as far as merge validation cares.
#[derive(Debug, Clone, Copy)]
enum NodeKind {
    Mapping,
    Set,
    Scalar,
    Null,
    /// A node whose anchor is defined but whose end has not been seen.
    Open,
    /// Sequence, with the verdict of its items taken as merge sources.
    Sequence(Result<(), MergeError>),
}

impl NodeKind {
    const fn as_merge_value(self) -> Result<(), MergeError> {
        match self {
            Self::Mapping => Ok(()),
            Self::Set => Err(MergeError::SetSource),
            Self::Scalar | Self::Null | Self::Open => Err(MergeError::NotMapping),
            Self::Sequence(items) => items,
        }
    }

    /// Whether the node can be the value of a `!!set` member: only null, and a still-open node,
    /// which is the loader's recursive alias to report.
    const fn is_set_value(self) -> bool {
        matches!(self, Self::Null | Self::Open)
    }

    const fn as_merge_item(self) -> Result<(), MergeError> {
        match self {
            Self::Sequence(_) => Err(MergeError::NotMapping),
            other => other.as_merge_value(),
        }
    }
}

#[derive(Debug)]
enum OpenKind {
    /// Mapping; `set` marks a `!!set`, `merge_key` the span of a just-seen `<<` key, `member`
    /// that of a just-seen set member, and `merge_seen` whether the mapping has a `<<` already.
    Mapping {
        set: bool,
        merge_key: Option<Span>,
        member: Option<Span>,
        merge_seen: bool,
    },
    Sequence(Result<(), MergeError>),
}

#[derive(Debug)]
struct Open {
    kind: OpenKind,
    role: NodeRole,
    anchor: usize,
    span: Span,
}

/// Rejects `<<` values that cannot be merged, repeated `<<` keys and `!!set` members that carry a
/// value, from the parser event stream.
///
/// Feed it every event of one stream, in order. It is the single implementation of this
/// validation: the core loader, the streaming formatter and the Python loader all use it, so each
/// reports the same first invalid node in document order. A repeated `<<` is an error unless the
/// validator is created with [`DuplicateMergeKeys::LastWins`].
///
/// Bindings reach it through [`EventStream`](crate::events::EventStream).
#[derive(Debug, Default)]
pub struct MergeKeyValidator {
    tracker: MergeKeyTracker,
    open: Vec<Open>,
    anchors: HashMap<usize, NodeKind>,
    documents: usize,
    duplicates: DuplicateMergeKeys,
    set_values: SetValues,
}

impl MergeKeyValidator {
    /// Creates a validator that treats a repeated `<<` and a `!!set` member value as `options`
    /// say.
    ///
    /// [`MergeKeyValidator::default`] rejects both.
    #[must_use]
    pub fn new(options: LoadOptions) -> Self {
        Self {
            duplicates: options.duplicate_merge_keys,
            set_values: options.set_values,
            ..Self::default()
        }
    }

    /// Feeds the next event with its span; returns the role of the node it starts, if any.
    ///
    /// Returns the role of the node the event starts, `None` for events that start no node.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Merge`] at the `<<` key of the first invalid merge value or repeated
    /// `<<` key, and [`ParseError::SetValue`] at the first `!!set` member that has a value.
    pub fn observe(
        &mut self,
        event: &Event<'_>,
        span: Span,
    ) -> Result<Option<NodeRole>, ParseError> {
        let role = self.tracker.observe(event);
        match role {
            Some(role) => self.observe_node(role, event, span)?,
            None => self.observe_structure(event)?,
        }
        Ok(role)
    }

    fn observe_node(
        &mut self,
        role: NodeRole,
        event: &Event<'_>,
        span: Span,
    ) -> Result<(), ParseError> {
        match event {
            Event::Scalar(text, style, anchor, tag) => {
                let kind = if *anchor > 0 || self.in_set() {
                    match resolve_scalar_raw(text, ScalarStyle::from_saphyr(*style), tag.as_deref())
                    {
                        ResolvedScalar::Null => NodeKind::Null,
                        _ => NodeKind::Scalar,
                    }
                } else {
                    NodeKind::Scalar
                };
                self.settle(role, kind, *anchor, span)
            }
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
                        member: None,
                        merge_seen: false,
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

    fn in_set(&self) -> bool {
        matches!(
            self.open.last(),
            Some(Open {
                kind: OpenKind::Mapping { set: true, .. },
                ..
            })
        )
    }

    /// An alias to a still-open node resolves to a non-mapping, like the loader does.
    fn reserve_anchor(&mut self, anchor: usize) {
        if anchor > 0 {
            self.anchors.insert(anchor, NodeKind::Open);
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
        let document = self.documents.saturating_sub(1);
        let merge_error = |error, at: Span| {
            let SourcePosition { line, column } = SourcePosition::from_span(at);
            ParseError::Merge {
                error,
                line,
                column,
                document,
            }
        };
        match (role, self.open.last_mut().map(|open| &mut open.kind)) {
            (NodeRole::Item, Some(OpenKind::Sequence(verdict))) => {
                if verdict.is_ok() {
                    *verdict = kind.as_merge_item();
                }
            }
            (
                NodeRole::MergeKey,
                Some(OpenKind::Mapping {
                    merge_key,
                    merge_seen,
                    ..
                }),
            ) => {
                if *merge_seen && self.duplicates == DuplicateMergeKeys::Reject {
                    return Err(merge_error(MergeError::DuplicateKey, span));
                }
                *merge_seen = true;
                *merge_key = Some(span);
            }
            (NodeRole::Key, Some(OpenKind::Mapping { member, .. })) => *member = Some(span),
            (
                NodeRole::Value,
                Some(OpenKind::Mapping {
                    set,
                    merge_key,
                    member,
                    ..
                }),
            ) => {
                if let Some(key) = merge_key.take() {
                    kind.as_merge_value()
                        .map_err(|error| merge_error(error, key))?;
                }
                if let Some(at) = member.take()
                    && *set
                    && self.set_values == SetValues::Reject
                    && !kind.is_set_value()
                {
                    let SourcePosition { line, column } = SourcePosition::from_span(at);
                    return Err(ParseError::SetValue {
                        line,
                        column,
                        document,
                    });
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
        "m: {<<: {x: 1}, <<: {y: 2}}\n",
        "k: &k <<\nm:\n  <<: {x: 1}\n  *k : {y: 2}\n",
        "s: !!set {a: 1}\n",
        "s: !!set {a: , b: ~}\n",
        "&a !!set {x: *a}\n",
        "n: &n ~\ns: !!set {a: *n}\n",
        "n: &n [1]\ns: !!set {a: *n}\n",
        "s: !!set {a: {b: 1}}\n",
        "s: !!set {a: [b]}\n",
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
    fn repeated_merge_key_is_rejected_at_the_second_key() {
        for (yaml, line, column) in [
            ("m: {<<: {x: 1}, <<: {y: 2}}", 1, 17),
            ("m:\n  <<: {x: 1}\n  !!merge a: {y: 2}\n", 3, 11),
            ("k: &k <<\nm:\n  <<: {x: 1}\n  *k : {y: 2}\n", 4, 3),
        ] {
            let (error, got_line, got_column, _) =
                rejected(yaml).unwrap_or_else(|| panic!("{yaml}"));
            assert_eq!(error, MergeError::DuplicateKey, "{yaml}");
            assert_eq!((got_line, got_column), (line, column), "{yaml}");
        }
    }

    #[test]
    fn last_wins_policy_accepts_a_repeated_merge_key() {
        let yaml = "m: {<<: {x: 1}, <<: {y: 2}}";
        let mut validator = MergeKeyValidator::new(
            LoadOptions::new().with_duplicate_merge_keys(DuplicateMergeKeys::LastWins),
        );
        for event in Parser::new_from_str(yaml) {
            let (event, span) = event.unwrap();
            validator.observe(&event, span).unwrap();
        }
        let mut validator = MergeKeyValidator::new(
            LoadOptions::new().with_duplicate_merge_keys(DuplicateMergeKeys::LastWins),
        );
        let invalid = Parser::new_from_str("m: {<<: 1, <<: {y: 2}}").try_for_each(|event| {
            let (event, span) = event.unwrap();
            validator.observe(&event, span).map(drop)
        });
        assert!(invalid.is_err());
    }

    #[test]
    fn ignore_policy_accepts_set_member_values_only() {
        let mut validator =
            MergeKeyValidator::new(LoadOptions::new().with_set_values(SetValues::Ignore));
        for event in Parser::new_from_str("s: !!set {a: 1, b: [c]}\nm: {<<: 1}") {
            let (event, span) = event.unwrap();
            if validator.observe(&event, span).is_err() {
                return;
            }
        }
        panic!("the invalid merge value must still be rejected");
    }

    #[test]
    fn set_member_values_must_be_null() {
        for yaml in [
            "!!set {a: 1}",
            "!!set {a: [b]}",
            "!!set {a: {b}}",
            "n: &n [1]\ns: !!set {a: *n}",
            "n: &n {b}\ns: !!set {a: *n}",
        ] {
            assert!(
                matches!(validate(yaml), Err(ParseError::SetValue { .. })),
                "{yaml}"
            );
        }
        for yaml in [
            "!!set {a, b: , c: ~}",
            "n: &n ~\ns: !!set {a: *n}",
            "&a !!set {x: *a}",
        ] {
            assert!(validate(yaml).is_ok(), "{yaml}");
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

    #[test]
    fn verbatim_set_tag_is_a_set_source_and_its_keys_are_ordinary() {
        let set = "s: &s !<tag:yaml.org,2002:set> {x}\nm:\n  <<: *s\n";
        assert!(matches!(
            validate(set),
            Err(ParseError::Merge {
                error: MergeError::SetSource,
                ..
            })
        ));
        assert!(validate("s: !<tag:yaml.org,2002:set> {k, <<}\n").is_ok());
    }

    #[test]
    fn merge_tag_keys_are_validated_like_plain_ones() {
        for key in [
            "!!merge <<",
            "!!merge '<<'",
            "!<tag:yaml.org,2002:merge> merge",
        ] {
            let bad = format!("m:\n  {key}: 1\n");
            assert!(
                matches!(
                    validate(&bad),
                    Err(ParseError::Merge {
                        error: MergeError::NotMapping,
                        line: 2,
                        ..
                    })
                ),
                "{key}"
            );
            assert!(
                validate(&format!("b: &b {{x: 1}}\nm:\n  {key}: *b\n")).is_ok(),
                "{key}"
            );
        }
        assert!(validate("s: !!set {!!merge <<}\n").is_ok());
    }
}
