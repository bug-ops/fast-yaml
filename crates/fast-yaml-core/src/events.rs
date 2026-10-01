//! Guarded parser event stream for language bindings.
//!
//! [`EventStream`](crate::events::EventStream) yields the parser's events as crate-owned types ([`Event`](crate::events::Event), [`ScalarStyle`],
//! [`Tag`](crate::events::Tag), [`AnchorId`](crate::events::AnchorId)) after running them through the resource limits ([`ParseLimits`]) and
//! merge key validation, so a binding builds its own value tree without depending on the
//! underlying parser crate and cannot skip a safety check.

use std::borrow::Cow;
use std::fmt;
use std::num::NonZeroUsize;

use saphyr_parser::{Parser as SaphyrParser, ScanError, Span, StrInput};

use crate::error::{ParseError, ParseResult, SourcePosition};
use crate::input::NormalizedInput;
use crate::limits::{LimitGuard, ParseLimits};
use crate::merge::NodeRole;
use crate::merge_check::MergeKeyValidator;

/// How a scalar was written in the source.
///
/// The enum is exhaustive: a new style must be handled by every consumer at compile time.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{ResolvedScalar, ScalarStyle, resolve_scalar};
///
/// assert_eq!(resolve_scalar("7", ScalarStyle::Plain, None), ResolvedScalar::Int(7));
/// assert_eq!(resolve_scalar("7", ScalarStyle::SingleQuoted, None), ResolvedScalar::Str("7"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalarStyle {
    /// Unquoted scalar.
    Plain,
    /// `'single quoted'` scalar.
    SingleQuoted,
    /// `"double quoted"` scalar.
    DoubleQuoted,
    /// `|` block scalar.
    Literal,
    /// `>` block scalar.
    Folded,
}

impl ScalarStyle {
    pub(crate) const fn from_saphyr(style: saphyr_parser::ScalarStyle) -> Self {
        match style {
            saphyr_parser::ScalarStyle::Plain => Self::Plain,
            saphyr_parser::ScalarStyle::SingleQuoted => Self::SingleQuoted,
            saphyr_parser::ScalarStyle::DoubleQuoted => Self::DoubleQuoted,
            saphyr_parser::ScalarStyle::Literal => Self::Literal,
            saphyr_parser::ScalarStyle::Folded => Self::Folded,
        }
    }
}

/// A node tag as a resolved handle and suffix.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::events::Tag;
///
/// let tag = Tag::new("tag:yaml.org,2002:", "set");
/// assert_eq!(tag.handle(), "tag:yaml.org,2002:");
/// assert_eq!(tag.suffix(), "set");
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Tag<'a>(pub(crate) Cow<'a, saphyr_parser::Tag>);

impl fmt::Debug for Tag<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tag")
            .field("handle", &self.handle())
            .field("suffix", &self.suffix())
            .finish()
    }
}

impl Tag<'static> {
    /// Creates a tag from its handle and suffix.
    #[must_use]
    pub fn new(handle: impl Into<String>, suffix: impl Into<String>) -> Self {
        Self(Cow::Owned(saphyr_parser::Tag {
            handle: handle.into(),
            suffix: suffix.into(),
        }))
    }
}

impl Tag<'_> {
    /// Returns the tag handle: the resolved prefix, or empty for a verbatim `!<...>` tag.
    #[must_use]
    pub fn handle(&self) -> &str {
        &self.0.handle
    }

    /// Returns the tag suffix.
    #[must_use]
    pub fn suffix(&self) -> &str {
        &self.0.suffix
    }
}

/// Identifies an anchored node within one stream; an [`Event::Alias`] names the anchor it refers to.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{NormalizedInput, ParseLimits, events::{Event, EventItem, EventStream}};
///
/// let input = NormalizedInput::new("- &a 1\n- *a\n")?;
/// let mut anchor = None;
/// for item in EventStream::new(&input, ParseLimits::default()) {
///     match item?.event {
///         Event::Scalar { anchor: Some(id), .. } => anchor = Some(id),
///         Event::Alias(id) => assert_eq!(Some(id), anchor),
///         _ => {}
///     }
/// }
/// assert!(anchor.is_some());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnchorId(NonZeroUsize);

impl AnchorId {
    fn new(raw: usize) -> Option<Self> {
        NonZeroUsize::new(raw).map(Self)
    }
}

/// One parser event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event<'a> {
    /// Start of the stream.
    StreamStart,
    /// End of the stream; the last event.
    StreamEnd,
    /// Start of a document.
    DocumentStart {
        /// Whether the document opens with an explicit `---`.
        explicit: bool,
    },
    /// End of a document.
    DocumentEnd,
    /// Reference to an earlier anchored node.
    Alias(AnchorId),
    /// A scalar.
    Scalar {
        /// Decoded text.
        value: Cow<'a, str>,
        /// Source style.
        style: ScalarStyle,
        /// Anchor defined on the scalar.
        anchor: Option<AnchorId>,
        /// Explicit tag.
        tag: Option<Tag<'a>>,
    },
    /// Start of a sequence.
    SequenceStart {
        /// Anchor defined on the sequence.
        anchor: Option<AnchorId>,
        /// Explicit tag.
        tag: Option<Tag<'a>>,
    },
    /// End of a sequence.
    SequenceEnd,
    /// Start of a mapping.
    MappingStart {
        /// Anchor defined on the mapping.
        anchor: Option<AnchorId>,
        /// Explicit tag.
        tag: Option<Tag<'a>>,
    },
    /// End of a mapping.
    MappingEnd,
}

/// An event with the position of its start and, for an event that starts a node, its role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventItem<'a> {
    /// The event.
    pub event: Event<'a>,
    /// Where the event starts in the source.
    pub at: SourcePosition,
    /// Role of the node the event starts, `None` for events that start no node.
    pub role: Option<NodeRole>,
}

/// Parser events of one input with limits and merge key validation applied.
///
/// Each item carries the event, the position of its start and, for an event that starts a node,
/// the node's [`NodeRole`]. The stream ends after [`Event::StreamEnd`] or after the first error.
///
/// Limits are enforced as events are produced, so a loader that builds values from the stream
/// never recurses deeper than [`ParseLimits::max_depth`] or expands more than the alias budget.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{NormalizedInput, ParseLimits, events::{Event, EventItem, EventStream}};
///
/// let input = NormalizedInput::new("a: 1")?;
/// let scalars = EventStream::new(&input, ParseLimits::default())
///     .filter(|item| matches!(item, Ok(EventItem { event: Event::Scalar { .. }, .. })))
///     .count();
/// assert_eq!(scalars, 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct EventStream<'a> {
    parser: SaphyrParser<'a, StrInput<'a>>,
    guard: LimitGuard,
    merge_keys: MergeKeyValidator,
    done: bool,
}

impl<'a> EventStream<'a> {
    /// Creates a stream over `input` enforcing `limits`.
    #[must_use]
    pub fn new(input: &'a NormalizedInput, limits: ParseLimits) -> Self {
        Self {
            parser: SaphyrParser::new_from_str(input.as_str()),
            guard: LimitGuard::new(limits),
            merge_keys: MergeKeyValidator::default(),
            done: false,
        }
    }

    /// Declares that the consumer shares anchored nodes instead of copying them, which lifts the
    /// alias-copy limit that only bounds copies.
    #[must_use]
    pub fn sharing_anchors(mut self) -> Self {
        self.guard = self.guard.sharing_anchors();
        self
    }

    /// Zero-based index of the document the next event belongs to.
    ///
    /// Between two documents this is the index of the document that follows.
    #[must_use]
    pub const fn document(&self) -> usize {
        self.guard.document()
    }

    fn convert(event: saphyr_parser::Event<'a>) -> Option<Event<'a>> {
        use saphyr_parser::Event as Raw;
        Some(match event {
            Raw::Nothing => return None,
            Raw::Alias(id) => Event::Alias(AnchorId::new(id)?),
            Raw::StreamStart => Event::StreamStart,
            Raw::StreamEnd => Event::StreamEnd,
            Raw::DocumentStart(explicit) => Event::DocumentStart { explicit },
            Raw::DocumentEnd => Event::DocumentEnd,
            Raw::Scalar(value, style, anchor, tag) => Event::Scalar {
                value,
                style: ScalarStyle::from_saphyr(style),
                anchor: AnchorId::new(anchor),
                tag: tag.map(Tag),
            },
            Raw::SequenceStart(anchor, tag) => Event::SequenceStart {
                anchor: AnchorId::new(anchor),
                tag: tag.map(Tag),
            },
            Raw::SequenceEnd => Event::SequenceEnd,
            Raw::MappingStart(anchor, tag) => Event::MappingStart {
                anchor: AnchorId::new(anchor),
                tag: tag.map(Tag),
            },
            Raw::MappingEnd => Event::MappingEnd,
        })
    }

    fn unknown_anchor(&self, span: Span) -> ParseError {
        let error = ScanError::new_str(span.start, "while parsing node, found unknown anchor");
        ParseError::scanner(&error, self.guard.document())
    }

    fn advance(&mut self) -> ParseResult<Option<EventItem<'a>>> {
        loop {
            let Some(next) = self.parser.next_event() else {
                return Ok(None);
            };
            let (raw, span) =
                next.map_err(|error| ParseError::scanner(&error, self.guard.document()))?;
            self.guard.observe(&raw, span)?;
            let role = self.merge_keys.observe(&raw, span)?;
            if matches!(raw, saphyr_parser::Event::Alias(0)) {
                return Err(self.unknown_anchor(span));
            }
            if let Some(event) = Self::convert(raw) {
                return Ok(Some(EventItem {
                    event,
                    at: SourcePosition::from_span(span),
                    role,
                }));
            }
        }
    }
}

impl fmt::Debug for EventStream<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventStream")
            .field("document", &self.document())
            .field("done", &self.done)
            .finish_non_exhaustive()
    }
}

impl<'a> Iterator for EventStream<'a> {
    type Item = ParseResult<EventItem<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let item = self.advance().transpose();
        self.done = match &item {
            Some(Ok(item)) => matches!(item.event, Event::StreamEnd),
            Some(Err(_)) | None => true,
        };
        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::{LimitKind, MaxAliasBytes};
    use std::fmt::Write;

    fn stream<'a>(input: &'a NormalizedInput<'_>) -> EventStream<'a> {
        EventStream::new(input, ParseLimits::default())
    }

    fn events<'a>(
        input: &'a NormalizedInput<'_>,
        limits: ParseLimits,
    ) -> ParseResult<Vec<Event<'a>>> {
        EventStream::new(input, limits)
            .map(|item| item.map(|item| item.event))
            .collect()
    }

    fn default_events(text: &str) -> ParseResult<Vec<Event<'static>>> {
        let input = NormalizedInput::new(text).unwrap();
        events(&input, ParseLimits::default()).map(|all| all.into_iter().map(into_static).collect())
    }

    fn into_static(event: Event<'_>) -> Event<'static> {
        match event {
            Event::Scalar {
                value,
                style,
                anchor,
                tag,
            } => Event::Scalar {
                value: Cow::Owned(value.into_owned()),
                style,
                anchor,
                tag: tag.map(|tag| Tag(Cow::Owned(tag.0.into_owned()))),
            },
            Event::SequenceStart { anchor, tag } => Event::SequenceStart {
                anchor,
                tag: tag.map(|tag| Tag(Cow::Owned(tag.0.into_owned()))),
            },
            Event::MappingStart { anchor, tag } => Event::MappingStart {
                anchor,
                tag: tag.map(|tag| Tag(Cow::Owned(tag.0.into_owned()))),
            },
            Event::StreamStart => Event::StreamStart,
            Event::StreamEnd => Event::StreamEnd,
            Event::DocumentStart { explicit } => Event::DocumentStart { explicit },
            Event::DocumentEnd => Event::DocumentEnd,
            Event::Alias(id) => Event::Alias(id),
            Event::SequenceEnd => Event::SequenceEnd,
            Event::MappingEnd => Event::MappingEnd,
        }
    }

    #[test]
    fn yields_events_through_stream_end() {
        let all = default_events("a: 1").unwrap();
        assert_eq!(all.first(), Some(&Event::StreamStart));
        assert_eq!(all.last(), Some(&Event::StreamEnd));
        assert!(matches!(all[1], Event::DocumentStart { explicit: false }));
    }

    #[test]
    fn maps_style_anchor_and_tag() {
        let all = default_events("- &x !!set 'v'\n- *x\n").unwrap();
        let Event::Scalar {
            style, anchor, tag, ..
        } = &all[3]
        else {
            panic!("scalar expected, got {:?}", all[3]);
        };
        assert_eq!(*style, ScalarStyle::SingleQuoted);
        assert_eq!(tag.as_ref().map(Tag::suffix), Some("set"));
        assert_eq!(all[4], Event::Alias(anchor.unwrap()));
    }

    #[test]
    fn collection_starts_carry_anchor_and_tag() {
        let all = default_events("&s !!set {a}\n").unwrap();
        let Event::MappingStart { anchor, tag } = &all[2] else {
            panic!("mapping start expected, got {:?}", all[2]);
        };
        assert!(anchor.is_some());
        assert_eq!(tag.as_ref().map(Tag::suffix), Some("set"));

        let all = default_events("&q !!seq [1]\n").unwrap();
        let Event::SequenceStart { anchor, tag } = &all[2] else {
            panic!("sequence start expected, got {:?}", all[2]);
        };
        assert!(anchor.is_some());
        assert_eq!(tag.as_ref().map(Tag::suffix), Some("seq"));
    }

    #[test]
    fn verbatim_tag_has_empty_handle() {
        let all = default_events("!<tag:yaml.org,2002:int> 7\n").unwrap();
        let Event::Scalar { tag: Some(tag), .. } = &all[2] else {
            panic!("tagged scalar expected, got {:?}", all[2]);
        };
        assert_eq!(tag.handle(), "");
        assert_eq!(tag.suffix(), "tag:yaml.org,2002:int");
    }

    #[test]
    fn anchors_are_distinct_and_aliases_name_them() {
        let all = default_events("- &a 1\n- &b 2\n- *b\n- *a\n").unwrap();
        let ids: Vec<_> = all
            .iter()
            .filter_map(|event| match event {
                Event::Scalar { anchor, .. } => *anchor,
                _ => None,
            })
            .collect();
        let aliases: Vec<_> = all
            .iter()
            .filter_map(|event| match event {
                Event::Alias(id) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert_eq!(aliases, [ids[1], ids[0]]);
    }

    #[test]
    fn tag_equality_and_debug_hide_parser_types() {
        let tag = Tag::new("!", "foo");
        assert_eq!(tag, Tag::new(String::from("!"), String::from("foo")));
        assert_ne!(tag, Tag::new("!", "bar"));
        assert_eq!(format!("{tag:?}"), r#"Tag { handle: "!", suffix: "foo" }"#);
    }

    #[test]
    fn stream_debug_reports_position_only() {
        let input = NormalizedInput::new("a").unwrap();
        let text = format!("{:?}", stream(&input));
        assert!(
            text.starts_with("EventStream { document: 0, done: false"),
            "{text}"
        );
    }

    #[test]
    fn stops_after_first_error() {
        let input = NormalizedInput::new("a: [").unwrap();
        let mut events = stream(&input);
        assert!(events.any(|item| item.is_err()));
        assert!(events.next().is_none());
    }

    #[test]
    fn unknown_alias_is_an_error() {
        assert!(default_events("[*a]").is_err());
        assert!(default_events("&a 1\n---\n*a\n").is_err());
    }

    #[test]
    fn enforces_limits() {
        let limits = ParseLimits {
            max_depth: crate::MaxDepth::new(1).unwrap(),
            ..ParseLimits::default()
        };
        let input = NormalizedInput::new("[[1]]").unwrap();
        assert!(matches!(
            events(&input, limits),
            Err(ParseError::LimitExceeded {
                kind: LimitKind::Depth(_),
                ..
            })
        ));
    }

    #[test]
    fn sharing_anchors_lifts_only_the_copy_limit() {
        let wrappers: String = (0..100).fold(String::new(), |mut acc, i| {
            write!(acc, "&a{i} [").unwrap();
            acc
        });
        let text = format!("{wrappers}{}0{}", "1, ".repeat(2000), "]".repeat(100));
        let input = NormalizedInput::new(&text).unwrap();
        let limits = ParseLimits {
            max_alias_bytes: MaxAliasBytes::new(1 << 20).unwrap(),
            ..ParseLimits::default()
        };
        assert!(matches!(
            events(&input, limits),
            Err(ParseError::LimitExceeded {
                kind: LimitKind::AnchorCopies(_),
                ..
            })
        ));
        let shared = EventStream::new(&input, limits).sharing_anchors();
        assert!(shared.collect::<Result<Vec<_>, _>>().is_ok());

        let cross = NormalizedInput::new("&a 1\n---\n*a\n").unwrap();
        let shared = EventStream::new(&cross, ParseLimits::default()).sharing_anchors();
        assert!(shared.collect::<Result<Vec<_>, _>>().is_err());
    }

    #[test]
    fn rejects_invalid_merge_value() {
        let input = NormalizedInput::new("{<<: 1}").unwrap();
        assert!(matches!(
            events(&input, ParseLimits::default()),
            Err(ParseError::Merge { .. })
        ));
    }

    #[test]
    fn reports_document_index() {
        let input = NormalizedInput::new("a\n---\nb\n").unwrap();
        let mut events = stream(&input);
        let mut seen = Vec::new();
        while let Some(item) = events.next() {
            if let Event::Scalar { .. } = item.unwrap().event {
                seen.push(events.document());
            }
        }
        assert_eq!(seen, [0, 1]);
        assert_eq!(events.document(), 2);
    }

    #[test]
    fn document_index_after_error_is_the_failing_document() {
        let input = NormalizedInput::new("a\n---\n[\n").unwrap();
        let mut events = stream(&input);
        assert!(events.any(|item| item.is_err()));
        assert_eq!(events.document(), 1);
    }
}
