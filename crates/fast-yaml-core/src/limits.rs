//! Resource limits for parsing untrusted YAML.
//!
//! [`LimitGuard`] observes the parser event stream before any tree is built, so
//! pathological input (deep nesting, alias amplification) is rejected while memory
//! and stack usage are still bounded.

use crate::error::{ParseError, ParseResult};
use saphyr_parser::{Event, ScanError, Span, Tag};
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

/// Maximum nesting depth of sequences and mappings.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxDepth;
///
/// assert_eq!(MaxDepth::default(), MaxDepth::DEFAULT);
/// assert_eq!(MaxDepth::new(8).get(), 8);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxDepth(usize);

impl MaxDepth {
    /// Default depth: matches saphyr's flow-nesting cap and keeps recursive
    /// consumers well inside small thread stacks.
    pub const DEFAULT: Self = Self(256);

    /// Creates a depth limit of `depth` nested collections.
    #[must_use]
    pub const fn new(depth: usize) -> Self {
        Self(depth)
    }

    /// Returns the limit as a plain number.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    /// Enters one more container level below `depth` enclosing ones.
    ///
    /// # Errors
    ///
    /// Returns [`LimitKind::Depth`] when `depth` already equals the limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::limits::MaxDepth;
    ///
    /// let max = MaxDepth::new(2);
    /// assert_eq!(max.descend(1), Ok(2));
    /// assert!(max.descend(2).is_err());
    /// ```
    pub const fn descend(self, depth: usize) -> Result<usize, LimitKind> {
        if depth >= self.0 {
            Err(LimitKind::Depth(self))
        } else {
            Ok(depth + 1)
        }
    }
}

impl Default for MaxDepth {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for MaxDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Maximum estimated memory, in bytes, materialized by alias expansion over a whole stream.
///
/// Each expanded node costs [`NODE_BYTES`] plus the length of its scalar and tag text, so both wide
/// and long-scalar amplification are bounded. The budget is shared by all documents of a
/// stream, not reset per document.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxAliasBytes;
///
/// assert_eq!(MaxAliasBytes::default(), MaxAliasBytes::DEFAULT);
/// assert_eq!(MaxAliasBytes::new(10).get(), 10);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxAliasBytes(usize);

impl MaxAliasBytes {
    /// Default budget: 64 MiB of expanded data per stream.
    pub const DEFAULT: Self = Self(64 * 1024 * 1024);

    /// Creates a budget of `bytes` expanded bytes.
    #[must_use]
    pub const fn new(bytes: usize) -> Self {
        Self(bytes)
    }

    /// Returns the budget as a plain number.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl Default for MaxAliasBytes {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for MaxAliasBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Maximum bytes of resolved `%TAG` prefix text the parser may materialize over a whole stream.
///
/// The parser copies the full prefix into every tagged node, so a long prefix reused by many
/// tags amplifies memory without any alias. Each tag is charged the length of its prefix above
/// [`TAG_PREFIX_ALLOWANCE`]; the budget is shared by all documents of a stream.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxTagBytes;
///
/// assert_eq!(MaxTagBytes::default(), MaxTagBytes::DEFAULT);
/// assert_eq!(MaxTagBytes::new(10).get(), 10);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxTagBytes(usize);

impl MaxTagBytes {
    /// Default budget: 64 MiB of expanded tag prefixes per stream.
    pub const DEFAULT: Self = Self(64 * 1024 * 1024);

    /// Creates a budget of `bytes` expanded prefix bytes.
    #[must_use]
    pub const fn new(bytes: usize) -> Self {
        Self(bytes)
    }

    /// Returns the budget as a plain number.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl Default for MaxTagBytes {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for MaxTagBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The set of limits enforced while parsing.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::Parser;
/// use fast_yaml_core::limits::{MaxDepth, ParseLimits};
///
/// let limits = ParseLimits { max_depth: MaxDepth::new(2), ..ParseLimits::default() };
/// assert!(Parser::parse_str_with_limits("[[1]]", &limits).is_ok());
/// assert!(Parser::parse_str_with_limits("[[[1]]]", &limits).is_err());
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseLimits {
    /// Maximum nesting depth of collections.
    pub max_depth: MaxDepth,
    /// Maximum estimated bytes produced by alias expansion, per stream.
    pub max_alias_bytes: MaxAliasBytes,
    /// Maximum bytes of expanded `%TAG` prefixes, per stream.
    pub max_tag_bytes: MaxTagBytes,
}

/// Identifies which limit was exceeded, carrying its configured value.
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    /// Collections are nested deeper than the limit.
    #[error("nesting depth exceeds {0}")]
    Depth(MaxDepth),
    /// Alias expansion would produce more data than the budget.
    #[error("alias expansion exceeds {0} bytes")]
    AliasBytes(MaxAliasBytes),
    /// Tag prefix expansion would materialize more data than the budget.
    #[error("tag prefix expansion exceeds {0} bytes")]
    TagBytes(MaxTagBytes),
}

/// Estimated fixed cost of one expanded node, in bytes, charged on top of scalar and tag text.
pub const NODE_BYTES: usize = 64;

/// Length of a resolved tag prefix, in bytes, that is not charged against [`MaxTagBytes`].
pub const TAG_PREFIX_ALLOWANCE: usize = 64;

/// Size of a completed subtree: expanded byte weight and collection height.
#[derive(Debug, Clone, Copy, Default)]
struct Subtree {
    bytes: usize,
    height: usize,
}

fn tag_bytes(tag: Option<&Tag>) -> usize {
    tag.map_or(0, |t| t.handle.len().saturating_add(t.suffix.len()))
}

#[derive(Debug)]
struct Frame {
    anchor: usize,
    tag_bytes: usize,
    /// Accumulated children: `bytes` is their sum, `height` their maximum.
    children: Subtree,
}

/// Streaming enforcer of [`ParseLimits`] over parser events.
///
/// Feed every event, in order, to [`observe`](Self::observe) before handing it to a
/// loader. It tracks open collections and the expanded size of every anchored node, so
/// an alias is rejected before the loader clones it.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::{LimitGuard, ParseLimits};
/// use saphyr_parser::{Event, Marker, Span};
///
/// let mut guard = LimitGuard::new(ParseLimits::default());
/// let span = Span::empty(Marker::new(0, 1, 0));
/// assert!(guard.observe(&Event::SequenceStart(0, None), span).is_ok());
/// ```
#[derive(Debug)]
pub struct LimitGuard {
    limits: ParseLimits,
    stack: Vec<Frame>,
    completed: HashMap<usize, Subtree>,
    alias_bytes: usize,
    tag_prefix_bytes: usize,
    max_anchor_seen: usize,
    // Anchors below this id belong to earlier documents and are out of scope.
    doc_anchor_floor: usize,
}

impl LimitGuard {
    /// Creates a guard enforcing `limits` from the start of a stream.
    #[must_use]
    pub fn new(limits: ParseLimits) -> Self {
        Self {
            limits,
            stack: Vec::new(),
            completed: HashMap::new(),
            alias_bytes: 0,
            tag_prefix_bytes: 0,
            max_anchor_seen: 0,
            doc_anchor_floor: 0,
        }
    }

    /// Accounts for one parser event.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::LimitExceeded`] when the event breaks a limit, and
    /// [`ParseError::Scanner`] for an alias that refers to an anchor from a previous
    /// document (anchors do not cross document boundaries).
    pub fn observe(&mut self, event: &Event<'_>, span: Span) -> ParseResult<()> {
        match event {
            Event::DocumentStart(_) => {
                self.completed.clear();
                self.doc_anchor_floor = self.max_anchor_seen + 1;
            }
            Event::SequenceStart(anchor, tag) | Event::MappingStart(anchor, tag) => {
                self.charge_tag_prefix(tag.as_deref(), span)?;
                if self.stack.len() >= self.limits.max_depth.get() {
                    return Err(Self::exceeded(
                        LimitKind::Depth(self.limits.max_depth),
                        span,
                    ));
                }
                self.max_anchor_seen = self.max_anchor_seen.max(*anchor);
                self.stack.push(Frame {
                    anchor: *anchor,
                    tag_bytes: tag_bytes(tag.as_deref()),
                    children: Subtree::default(),
                });
            }
            Event::Scalar(text, _, anchor, tag) => {
                self.charge_tag_prefix(tag.as_deref(), span)?;
                self.max_anchor_seen = self.max_anchor_seen.max(*anchor);
                self.complete(
                    *anchor,
                    Subtree {
                        bytes: NODE_BYTES
                            .saturating_add(text.len())
                            .saturating_add(tag_bytes(tag.as_deref())),
                        height: 0,
                    },
                );
            }
            Event::SequenceEnd | Event::MappingEnd => {
                if let Some(frame) = self.stack.pop() {
                    self.complete(
                        frame.anchor,
                        Subtree {
                            bytes: frame
                                .children
                                .bytes
                                .saturating_add(NODE_BYTES)
                                .saturating_add(frame.tag_bytes),
                            height: frame.children.height + 1,
                        },
                    );
                }
            }
            Event::Alias(id) => self.observe_alias(*id, span)?,
            _ => {}
        }
        Ok(())
    }

    fn charge_tag_prefix(&mut self, tag: Option<&Tag>, span: Span) -> ParseResult<()> {
        let Some(tag) = tag else { return Ok(()) };
        self.tag_prefix_bytes = self
            .tag_prefix_bytes
            .saturating_add(tag.handle.len().saturating_sub(TAG_PREFIX_ALLOWANCE));
        if self.tag_prefix_bytes > self.limits.max_tag_bytes.get() {
            return Err(Self::exceeded(
                LimitKind::TagBytes(self.limits.max_tag_bytes),
                span,
            ));
        }
        Ok(())
    }

    fn observe_alias(&mut self, id: usize, span: Span) -> ParseResult<()> {
        if id < self.doc_anchor_floor {
            return Err(
                ScanError::new_str(span.start, "while parsing node, found unknown anchor").into(),
            );
        }
        // Absent means the anchor's collection is still open (`&a [*a]`); the loader yields one node.
        let subtree = self.completed.get(&id).copied().unwrap_or(Subtree {
            bytes: NODE_BYTES,
            height: 0,
        });
        self.alias_bytes = self.alias_bytes.saturating_add(subtree.bytes);
        if self.alias_bytes > self.limits.max_alias_bytes.get() {
            return Err(Self::exceeded(
                LimitKind::AliasBytes(self.limits.max_alias_bytes),
                span,
            ));
        }
        if self.stack.len().saturating_add(subtree.height) > self.limits.max_depth.get() {
            return Err(Self::exceeded(
                LimitKind::Depth(self.limits.max_depth),
                span,
            ));
        }
        self.complete(0, subtree);
        Ok(())
    }

    fn complete(&mut self, anchor: usize, subtree: Subtree) {
        if anchor > 0 {
            self.completed.insert(anchor, subtree);
        }
        if let Some(parent) = self.stack.last_mut() {
            parent.children.bytes = parent.children.bytes.saturating_add(subtree.bytes);
            parent.children.height = parent.children.height.max(subtree.height);
        }
    }

    // Char-based column, shifted to 1-indexed like saphyr's own errors; no source text to convert from here.
    #[allow(clippy::disallowed_methods)]
    fn exceeded(kind: LimitKind, span: Span) -> ParseError {
        ParseError::LimitExceeded {
            kind,
            line: span.start.line(),
            column: span.start.col() + 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saphyr_parser::{Marker, ScalarStyle};
    use std::borrow::Cow;

    fn span() -> Span {
        Span::empty(Marker::new(0, 1, 0))
    }

    #[test]
    fn doubling_alias_chain_saturates_without_panic() {
        let limits = ParseLimits {
            max_alias_bytes: MaxAliasBytes::new(usize::MAX),
            max_depth: MaxDepth::new(1_000),
            ..ParseLimits::default()
        };
        let mut guard = LimitGuard::new(limits);
        let scalar = Event::Scalar(Cow::Borrowed("x"), ScalarStyle::Plain, 1, None);
        guard.observe(&Event::DocumentStart(false), span()).unwrap();
        guard.observe(&scalar, span()).unwrap();
        for id in 2..200 {
            guard
                .observe(&Event::SequenceStart(id, None), span())
                .unwrap();
            guard.observe(&Event::Alias(id - 1), span()).unwrap();
            guard.observe(&Event::Alias(id - 1), span()).unwrap();
            guard.observe(&Event::SequenceEnd, span()).unwrap();
        }
    }

    fn tagged_scalar(prefix_len: usize) -> Event<'static> {
        let tag = Tag {
            handle: "p".repeat(prefix_len),
            suffix: "x".to_owned(),
        };
        Event::Scalar(
            Cow::Borrowed("v"),
            ScalarStyle::Plain,
            0,
            Some(Cow::Owned(tag)),
        )
    }

    fn tag_limits(bytes: usize) -> ParseLimits {
        ParseLimits {
            max_tag_bytes: MaxTagBytes::new(bytes),
            ..ParseLimits::default()
        }
    }

    #[test]
    fn prefix_within_allowance_is_free() {
        let mut guard = LimitGuard::new(tag_limits(0));
        for _ in 0..1_000 {
            guard
                .observe(&tagged_scalar(TAG_PREFIX_ALLOWANCE), span())
                .unwrap();
        }
    }

    #[test]
    fn prefix_charge_exceeding_budget_is_rejected() {
        let mut guard = LimitGuard::new(tag_limits(20));
        let event = tagged_scalar(TAG_PREFIX_ALLOWANCE + 10);
        guard.observe(&event, span()).unwrap();
        guard.observe(&event, span()).unwrap();
        let err = guard.observe(&event, span()).unwrap_err();
        assert!(matches!(
            err,
            ParseError::LimitExceeded {
                kind: LimitKind::TagBytes(_),
                ..
            }
        ));
    }

    #[test]
    fn tag_budget_persists_across_documents() {
        let mut guard = LimitGuard::new(tag_limits(10));
        let event = tagged_scalar(TAG_PREFIX_ALLOWANCE + 10);
        guard.observe(&Event::DocumentStart(false), span()).unwrap();
        guard.observe(&event, span()).unwrap();
        guard.observe(&Event::DocumentStart(false), span()).unwrap();
        assert!(guard.observe(&event, span()).is_err());
    }

    #[test]
    fn collection_tags_are_charged() {
        let mut guard = LimitGuard::new(tag_limits(0));
        let tag = Tag {
            handle: "p".repeat(TAG_PREFIX_ALLOWANCE + 1),
            suffix: String::new(),
        };
        let event = Event::MappingStart(0, Some(Cow::Owned(tag)));
        assert!(guard.observe(&event, span()).is_err());
    }

    #[test]
    fn unlimited_tag_budget_never_rejects() {
        let mut guard = LimitGuard::new(tag_limits(usize::MAX));
        let event = tagged_scalar(TAG_PREFIX_ALLOWANCE + 1_000);
        for _ in 0..1_000 {
            guard.observe(&event, span()).unwrap();
        }
    }
}
