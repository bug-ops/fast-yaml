//! Role tracking for parser events, shared by rules that need to tell keys from values.

use crate::source::offset::ByteRange;

/// Position of a node relative to its enclosing collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole {
    /// Key of a mapping entry.
    MappingKey,
    /// Value of a mapping entry.
    MappingValue,
    /// Item of a sequence.
    SequenceItem,
    /// Node outside any collection.
    Root,
}

/// Syntax of a collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionStyle {
    /// Indentation-based collection.
    Block,
    /// Bracketed `{...}` or `[...]` collection.
    Flow,
}

impl CollectionStyle {
    /// Style of the collection whose start event covers `range`.
    ///
    /// Block collection starts are empty ranges. A single-pair mapping inside a flow
    /// sequence (`[a: b]`) is not bracketed itself; [`RoleTracker`] makes it inherit `Flow`.
    pub fn of_start(source: &str, range: ByteRange) -> Self {
        let bracketed = range.start() != range.end()
            && matches!(
                source.as_bytes().get(range.start().get()),
                Some(b'{' | b'[')
            );
        if bracketed { Self::Flow } else { Self::Block }
    }
}

enum Scope {
    Mapping {
        style: CollectionStyle,
        expecting_key: bool,
    },
    Sequence {
        style: CollectionStyle,
    },
}

/// Tracks which role each node of an event stream plays.
///
/// Call [`node`](Self::node) for every scalar, alias and collection start, then
/// [`enter_mapping`](Self::enter_mapping) / [`enter_sequence`](Self::enter_sequence) for a
/// collection start, and [`leave`](Self::leave) for every collection end.
#[derive(Default)]
pub struct RoleTracker {
    scopes: Vec<Scope>,
}

impl RoleTracker {
    /// Returns the role of the next node and advances the enclosing mapping to its next slot.
    pub fn node(&mut self) -> NodeRole {
        match self.scopes.last_mut() {
            Some(Scope::Mapping { expecting_key, .. }) => {
                let role = if *expecting_key {
                    NodeRole::MappingKey
                } else {
                    NodeRole::MappingValue
                };
                *expecting_key = !*expecting_key;
                role
            }
            Some(Scope::Sequence { .. }) => NodeRole::SequenceItem,
            None => NodeRole::Root,
        }
    }

    /// Opens a mapping; it is a flow mapping whenever it sits inside a flow collection.
    pub fn enter_mapping(&mut self, style: CollectionStyle) {
        self.scopes.push(Scope::Mapping {
            style: self.inherit(style),
            expecting_key: true,
        });
    }

    /// Opens a sequence; it is a flow sequence whenever it sits inside a flow collection.
    pub fn enter_sequence(&mut self, style: CollectionStyle) {
        self.scopes.push(Scope::Sequence {
            style: self.inherit(style),
        });
    }

    fn inherit(&self, style: CollectionStyle) -> CollectionStyle {
        if self.in_flow() {
            CollectionStyle::Flow
        } else {
            style
        }
    }

    /// Closes the innermost collection.
    pub fn leave(&mut self) {
        self.scopes.pop();
    }

    /// Whether the current position is inside a flow collection.
    pub fn in_flow(&self) -> bool {
        self.scopes.last().is_some_and(|scope| match scope {
            Scope::Mapping { style, .. } | Scope::Sequence { style } => {
                *style == CollectionStyle::Flow
            }
        })
    }
}
