//! Compact index of the nodes of a source, filled while the source loads.
//!
//! The value rules (truthy, quoted-strings, float-values, empty-values) read their scalars from
//! here instead of parsing the source again.

use fast_yaml_core::ScalarStyle;

use crate::rules::node_roles::NodeRole;
use crate::source::offset::{ByteOffset, ByteRange};

/// Nodes per chunk; chunks keep the index free of the copy and slack of a doubling `Vec`.
const CHUNK: usize = 2048;

/// What the rules need to know about the tag of a scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    /// No explicit tag.
    None,
    /// A YAML core schema tag such as `!!str`, which sets the type explicitly.
    Core,
    /// Any other tag.
    Other,
}

/// Where the decoded text of a scalar lives.
#[derive(Debug, Clone, Copy)]
enum TextLoc {
    /// The source text of the scalar's range.
    Whole,
    /// The source text between the quotes of a quoted scalar.
    Inner,
    /// The entry of [`NodeIndex::texts`] that starts where the scalar does.
    Side,
}

/// A scalar of the source.
#[derive(Debug, Clone, Copy)]
pub struct ScalarNode {
    /// The scalar token, quotes included.
    pub range: ByteRange,
    pub style: ScalarStyle,
    pub role: NodeRole,
    /// Whether the scalar sits inside a flow collection.
    pub in_flow: bool,
    pub tag: TagKind,
    /// Whether the scalar defines an anchor.
    pub anchored: bool,
    text: TextLoc,
}

impl ScalarNode {
    /// A scalar with its text not yet placed; [`NodeIndex::push_scalar`] places it.
    pub(crate) const fn new(
        range: ByteRange,
        style: ScalarStyle,
        role: NodeRole,
        in_flow: bool,
        tag: TagKind,
        anchored: bool,
    ) -> Self {
        Self {
            range,
            style,
            role,
            in_flow,
            tag,
            anchored,
            text: TextLoc::Whole,
        }
    }
}

/// One entry of the index.
#[derive(Debug, Clone, Copy)]
pub enum Node {
    Scalar(ScalarNode),
    Alias {
        range: ByteRange,
        role: NodeRole,
    },
    /// The start of a mapping or sequence.
    Open,
}

/// The scalars, aliases and collection starts of a source, in source order.
///
/// Decoded scalar text is kept in a side table only when it differs from the source text.
#[derive(Debug, Default)]
pub struct NodeIndex<'a> {
    source: &'a str,
    chunks: Vec<Vec<Node>>,
    /// Decoded text by the start of its scalar, ascending.
    texts: Vec<(ByteOffset, Box<str>)>,
}

impl<'a> NodeIndex<'a> {
    pub(crate) const fn new(source: &'a str) -> Self {
        Self {
            source,
            chunks: Vec::new(),
            texts: Vec::new(),
        }
    }

    pub(crate) fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.chunks.iter().flatten()
    }

    fn push(&mut self, node: Node) {
        match self.chunks.last_mut() {
            Some(chunk) if chunk.len() < CHUNK => chunk.push(node),
            _ => {
                let mut chunk = Vec::with_capacity(CHUNK);
                chunk.push(node);
                self.chunks.push(chunk);
            }
        }
    }

    pub(crate) fn push_open(&mut self) {
        self.push(Node::Open);
    }

    pub(crate) fn push_alias(&mut self, range: ByteRange, role: NodeRole) {
        self.push(Node::Alias { range, role });
    }

    /// Adds a scalar whose decoded text is `value`.
    pub(crate) fn push_scalar(&mut self, scalar: ScalarNode, value: &str) {
        let text = self.locate(scalar.range, scalar.style, value);
        self.push(Node::Scalar(ScalarNode { text, ..scalar }));
    }

    fn locate(&mut self, range: ByteRange, style: ScalarStyle, value: &str) -> TextLoc {
        if self.slice(range) == Some(value) {
            return TextLoc::Whole;
        }
        if matches!(style, ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted)
            && self.inner(range) == Some(value)
        {
            return TextLoc::Inner;
        }
        self.texts.push((range.start(), value.into()));
        TextLoc::Side
    }

    fn slice(&self, range: ByteRange) -> Option<&str> {
        self.source.get(range.start().get()..range.end().get())
    }

    fn inner(&self, range: ByteRange) -> Option<&str> {
        let end = range.end().get().checked_sub(1)?;
        self.source.get(range.start().add_bytes(1).get()..end)
    }

    /// The decoded text of `scalar`.
    pub(crate) fn text(&self, scalar: &ScalarNode) -> &str {
        match scalar.text {
            TextLoc::Whole => self.slice(scalar.range),
            TextLoc::Inner => self.inner(scalar.range),
            TextLoc::Side => self
                .texts
                .binary_search_by_key(&scalar.range.start(), |(start, _)| *start)
                .ok()
                .and_then(|at| self.texts.get(at))
                .map(|(_, text)| &**text),
        }
        .unwrap_or_default()
    }

    /// The source text of `range`.
    pub(crate) fn source_text(&self, range: ByteRange) -> Option<&str> {
        self.slice(range)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: usize, end: usize) -> ByteRange {
        ByteRange::new(ByteOffset::new(start), ByteOffset::new(end))
    }

    fn scalar(range: ByteRange, style: ScalarStyle) -> ScalarNode {
        ScalarNode::new(range, style, NodeRole::Root, false, TagKind::None, false)
    }

    fn text_of(source: &str, range: ByteRange, style: ScalarStyle, value: &str) -> (String, bool) {
        let mut index = NodeIndex::new(source);
        index.push_scalar(scalar(range, style), value);
        let Some(Node::Scalar(node)) = index.nodes().next().copied() else {
            unreachable!("a scalar was pushed")
        };
        (index.text(&node).to_owned(), index.texts.is_empty())
    }

    #[test]
    fn text_equal_to_the_source_is_not_copied() {
        assert_eq!(
            text_of("a: yes", range(3, 6), ScalarStyle::Plain, "yes"),
            ("yes".to_owned(), true)
        );
        assert_eq!(
            text_of("a: \"yes\"", range(3, 8), ScalarStyle::DoubleQuoted, "yes"),
            ("yes".to_owned(), true)
        );
    }

    #[test]
    fn decoded_text_goes_to_the_side_table() {
        assert_eq!(
            text_of(
                "a: 'it''s'",
                range(3, 10),
                ScalarStyle::SingleQuoted,
                "it's"
            ),
            ("it's".to_owned(), false)
        );
        assert_eq!(
            text_of("a: x\n  y", range(3, 8), ScalarStyle::Plain, "x y"),
            ("x y".to_owned(), false)
        );
    }

    #[test]
    fn empty_scalar_at_the_end_is_whole() {
        assert_eq!(
            text_of("a:", range(2, 2), ScalarStyle::Plain, ""),
            (String::new(), true)
        );
    }

    #[test]
    fn side_texts_are_found_among_many_nodes() {
        let source = "k: 'a''b'\n".repeat(CHUNK);
        let mut index = NodeIndex::new(&source);
        for line in 0..CHUNK {
            let start = line * 10 + 3;
            index.push_scalar(
                scalar(range(start, start + 6), ScalarStyle::SingleQuoted),
                "a'b",
            );
            index.push_open();
        }
        let texts: Vec<&str> = index
            .nodes()
            .filter_map(|node| match node {
                Node::Scalar(scalar) => Some(index.text(scalar)),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), CHUNK);
        assert!(texts.iter().all(|text| *text == "a'b"));
        assert_eq!(index.nodes().count(), 2 * CHUNK);
    }
}
