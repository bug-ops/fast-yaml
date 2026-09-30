//! Property-based round-trip checks for the streaming formatter.
//!
//! Generated documents are valid YAML by construction, so any failure is a formatter or parser
//! defect rather than a rejected input.

use std::fmt::Write as _;

use fast_yaml_core::{Emitter, Parser};
use proptest::prelude::*;

type Comment = Option<String>;

#[derive(Debug, Clone)]
enum Node {
    Scalar(String),
    Plain(Vec<String>),
    Block { folded: bool, lines: Vec<String> },
    Alias(usize),
    Anchored(Box<Self>),
    Tagged(Box<Self>),
    Seq(Vec<(Comment, Self)>),
    Map(Vec<(Comment, String, Self)>),
}

#[derive(Debug, Clone)]
struct Document {
    root: Node,
    explicit_start: bool,
}

#[derive(Clone, Copy, Default)]
struct Props {
    anchor: bool,
    tag: bool,
}

#[derive(Default)]
struct Renderer {
    out: String,
    next_anchor: usize,
    defined: Vec<String>,
}

fn scalar() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z][a-z0-9_]{0,8}",
        "-?[0-9]{1,6}",
        "-?[0-9]{1,3}\\.[0-9]{1,3}",
        Just("true".to_owned()),
        Just("null".to_owned()),
        Just("~".to_owned()),
        Just("yes".to_owned()),
        Just("0x1F".to_owned()),
        Just("1e3".to_owned()),
        Just(".inf".to_owned()),
        "\"[a-z #:,]{0,8}\"",
        "'[a-z #:,]{0,8}'",
        "[а-яé日本]{1,5}",
        Just("[]".to_owned()),
        Just("{}".to_owned()),
        Just("[a, 1, \"b c\"]".to_owned()),
        Just("{a: 1, b: [x, y]}".to_owned()),
    ]
}

fn key() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-z][a-z0-9_]{0,8}",
        "[0-9]{1,3}",
        "\"[a-z ]{1,6}\"",
        "[а-я]{1,4}",
    ]
}

fn comment() -> impl Strategy<Value = Comment> {
    prop::option::weighted(0.25, "[a-z ]{0,10}")
}

fn leaf() -> impl Strategy<Value = Node> {
    prop_oneof![
        4 => scalar().prop_map(Node::Scalar),
        1 => prop::collection::vec("[a-z]{1,6}", 2..4).prop_map(Node::Plain),
        1 => (any::<bool>(), prop::collection::vec("[a-z][a-z ]{0,6}", 1..4))
            .prop_map(|(folded, lines)| Node::Block { folded, lines }),
        3 => (0..8usize).prop_map(Node::Alias),
    ]
}

fn node() -> impl Strategy<Value = Node> {
    leaf().prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            3 => prop::collection::vec((comment(), inner.clone()), 1..4).prop_map(Node::Seq),
            3 => prop::collection::vec((comment(), key(), inner.clone()), 1..4).prop_map(Node::Map),
            3 => inner.clone().prop_map(|n| Node::Anchored(Box::new(n))),
            1 => inner.prop_map(|n| Node::Tagged(Box::new(n))),
        ]
    })
}

fn documents() -> impl Strategy<Value = String> {
    let doc = (node(), any::<bool>()).prop_map(|(root, explicit_start)| Document {
        root: match root {
            Node::Seq(_) | Node::Map(_) => root,
            other => Node::Seq(vec![(None, other)]),
        },
        explicit_start,
    });
    prop::collection::vec(doc, 1..3).prop_map(|docs| {
        let mut out = String::new();
        for (idx, doc) in docs.iter().enumerate() {
            if doc.explicit_start || idx > 0 {
                out.push_str("---\n");
            }
            let mut renderer = Renderer::default();
            renderer.block(&doc.root, 0);
            out.push_str(&renderer.out);
        }
        out
    })
}

impl Renderer {
    fn block(&mut self, node: &Node, indent: usize) {
        let pad = " ".repeat(indent);
        match node {
            Node::Seq(items) => {
                for (comment, item) in items {
                    self.comment(comment, &pad);
                    self.entry(&format!("{pad}-"), item, indent);
                }
            }
            Node::Map(entries) => {
                for (comment, key, value) in entries {
                    self.comment(comment, &pad);
                    self.entry(&format!("{pad}{key}:"), value, indent);
                }
            }
            leaf => self.entry(&pad, leaf, indent.saturating_sub(2)),
        }
    }

    fn comment(&mut self, comment: &Comment, pad: &str) {
        if let Some(text) = comment {
            let _ = writeln!(self.out, "{pad}# {text}");
        }
    }

    fn entry(&mut self, prefix: &str, value: &Node, indent: usize) {
        self.props_entry(prefix, value, indent, Props::default());
    }

    fn props_entry(&mut self, prefix: &str, value: &Node, indent: usize, props: Props) {
        let child_pad = " ".repeat(indent + 2);
        match value {
            Node::Scalar(text) => {
                let _ = writeln!(self.out, "{prefix} {text}");
            }
            Node::Plain(words) => {
                let joined = words.join(&format!("\n{child_pad}"));
                let _ = writeln!(self.out, "{prefix} {joined}");
            }
            Node::Block { folded, lines } => {
                let indicator = if *folded { '>' } else { '|' };
                let _ = writeln!(self.out, "{prefix} {indicator}");
                for line in lines {
                    let _ = writeln!(self.out, "{child_pad}{line}");
                }
            }
            Node::Alias(idx) => match self.defined.len() {
                0 => {
                    let _ = writeln!(self.out, "{prefix} null");
                }
                len => {
                    let name = &self.defined[idx % len];
                    let _ = writeln!(self.out, "{prefix} *{name}");
                }
            },
            Node::Anchored(inner) | Node::Tagged(inner)
                if matches!(inner.core(), Node::Alias(_)) =>
            {
                self.props_entry(prefix, inner.core(), indent, props);
            }
            Node::Anchored(inner) if props.anchor => self.props_entry(prefix, inner, indent, props),
            Node::Anchored(inner) => {
                let name = format!("a{}", self.next_anchor);
                self.next_anchor += 1;
                let props = Props {
                    anchor: true,
                    ..props
                };
                self.props_entry(&format!("{prefix} &{name}"), inner, indent, props);
                self.defined.push(name);
            }
            Node::Tagged(inner) if props.tag => self.props_entry(prefix, inner, indent, props),
            Node::Tagged(inner) => {
                let props = Props { tag: true, ..props };
                self.props_entry(&format!("{prefix} !custom"), inner, indent, props);
            }
            collection => {
                let _ = writeln!(self.out, "{prefix}");
                self.block(collection, indent + 2);
            }
        }
    }
}

impl Node {
    fn core(&self) -> &Self {
        match self {
            Self::Anchored(inner) | Self::Tagged(inner) => inner.core(),
            other => other,
        }
    }
}

proptest! {
    #[test]
    fn format_is_idempotent(yaml in documents()) {
        let once = Emitter::format(&yaml).expect("generated YAML must format");
        let twice = Emitter::format(&once).expect("formatted YAML must format again");
        prop_assert_eq!(&once, &twice, "input:\n{}", yaml);
    }

    #[test]
    fn format_preserves_parsed_value(yaml in documents()) {
        let before = Parser::parse_all(&yaml).expect("generated YAML must parse");
        let formatted = Emitter::format(&yaml).expect("generated YAML must format");
        let after = Parser::parse_all(&formatted).expect("formatted YAML must parse");
        prop_assert_eq!(before, after, "input:\n{}\nformatted:\n{}", yaml, formatted);
    }

    #[test]
    fn arbitrary_text_never_panics_and_stays_idempotent(bom in prop::option::of(Just('\u{feff}')), body in "[ -~\r\n\t]{0,200}") {
        let input = format!("{}{body}", bom.map(String::from).unwrap_or_default());
        if Parser::parse_all(&input).is_ok()
            && let Ok(once) = Emitter::format(&input)
        {
            let twice = Emitter::format(&once).expect("formatted YAML must format again");
            prop_assert_eq!(once, twice, "input:\n{}", input);
        }
    }
}
