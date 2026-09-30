//! Generic streaming formatter with pluggable backend.
//!
//! This module contains the core formatting logic abstracted over
//! different memory allocation strategies via the `FormatterBackend` trait.

use std::borrow::Cow;
use std::fmt::Write;

use saphyr_parser::{Event, ScalarStyle, Span, Tag};

use super::traits::{AnchorStoreOps, ContextStackOps, FormatterBackend};
use super::{Context, INDENT_SPACES, MAX_ANCHOR_ID, MAX_DEPTH};
use crate::emitter::{EmitterConfig, MAX_INDENT, MIN_INDENT, block_scalar_header};

/// Width of the `"- "` sequence entry indicator.
const DASH_WIDTH: usize = 2;

/// Absolute output column at which the entries of a block collection start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Column(usize);

/// Kind of a block collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollectionKind {
    Sequence,
    Mapping,
}

impl CollectionKind {
    /// Context of the entries of a collection of this kind.
    const fn entry_context(self) -> Context {
        match self {
            Self::Sequence => Context::Sequence,
            Self::Mapping => Context::MappingKey,
        }
    }

    /// Flow-style text of an empty collection of this kind.
    const fn empty_flow(self) -> &'static str {
        match self {
            Self::Sequence => "[]",
            Self::Mapping => "{}",
        }
    }
}

/// Whether a node ends its own line (block scalar) or shares it with its context suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeShape {
    Inline,
    BlockScalar,
}

/// Block scalar style requested by the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockStyle {
    Literal,
    Folded,
}

/// A collection start held back until the next event shows whether it is empty.
struct PendingStart {
    kind: CollectionKind,
    anchor_id: usize,
    tag: Option<Tag>,
}

/// Whether `c` is a YAML non-printable that only a double-quoted escape can represent (tab excluded).
fn needs_escape(c: char) -> bool {
    (c.is_control() && c != '\t') || matches!(c, '\u{FFFE}' | '\u{FFFF}')
}

/// Returns the style a scalar must be written in to round-trip its value.
///
/// Plain and single-quoted scalars cannot represent control characters other than tab
/// (a raw line break is folded on re-parse), so they are promoted to double-quoted.
fn effective_style(value: &str, style: ScalarStyle) -> ScalarStyle {
    match style {
        ScalarStyle::Plain | ScalarStyle::SingleQuoted if value.chars().any(needs_escape) => {
            ScalarStyle::DoubleQuoted
        }
        other => other,
    }
}

/// Writes a tag in its shortest re-parseable form.
///
/// Core-schema tags become `!!x`, primary-handle tags `!x`, the non-specific tag `!`;
/// everything else (already expanded by the parser) is written verbatim as `!<...>`.
/// The parser percent-decodes tag text, so characters outside the YAML tag charset
/// are percent-encoded again.
fn write_tag(out: &mut String, tag: &Tag) {
    if tag.handle == YAML_CORE_TAG_PREFIX {
        out.push_str("!!");
        push_tag_text(out, &tag.suffix, TagForm::Shorthand);
    } else if tag.handle == "!" {
        out.push('!');
        push_tag_text(out, &tag.suffix, TagForm::Shorthand);
    } else if tag.handle.is_empty() && tag.suffix == "!" {
        out.push('!');
    } else {
        out.push_str("!<");
        push_tag_text(out, &tag.handle, TagForm::Verbatim);
        push_tag_text(out, &tag.suffix, TagForm::Verbatim);
        out.push('>');
    }
}

/// Tag prefix the parser expands `!!` to.
const YAML_CORE_TAG_PREFIX: &str = "tag:yaml.org,2002:";

/// Non-alphanumeric characters allowed unescaped in every tag form.
const TAG_SAFE_CHARS: &str = "-#;/?:@&=+$_.~*'()";

/// Additional characters allowed unescaped only inside `!<...>`.
const VERBATIM_ONLY_TAG_CHARS: &str = "!,[]";

/// Syntactic form a tag is written in; decides which characters need escaping.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TagForm {
    /// `!x` / `!!x`
    Shorthand,
    /// `!<...>`
    Verbatim,
}

/// Appends `text`, percent-encoding bytes outside the allowed tag charset.
fn push_tag_text(out: &mut String, text: &str, form: TagForm) {
    for c in text.chars() {
        let allowed = c.is_ascii_alphanumeric()
            || TAG_SAFE_CHARS.contains(c)
            || (form == TagForm::Verbatim && VERBATIM_ONLY_TAG_CHARS.contains(c));
        if allowed {
            out.push(c);
        } else {
            let mut buf = [0; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
}

/// Whether a scalar carries an explicit tag that pins its type.
#[derive(Clone, Copy)]
enum ScalarTyping {
    Tagged,
    Implicit,
}

/// Generic streaming formatter with pluggable backend.
///
/// This struct contains ALL formatting logic and is parameterized over
/// the backend type `B: FormatterBackend`. Through monomorphization,
/// this compiles to specialized code for each backend with zero runtime cost.
#[allow(clippy::struct_excessive_bools)]
pub struct StreamingFormatter<'a, B: FormatterBackend> {
    config: &'a EmitterConfig,
    /// `config.indent` normalized to the supported range; `indent` is a public field,
    /// so `EmitterConfig::with_indent` may have been bypassed.
    indent: usize,
    output: String,
    /// Entry columns of the open block collections, innermost last.
    columns: Vec<Column>,
    /// Tracks whether we need to emit a newline before the next value
    pending_newline: bool,
    /// Tracks whether the last character written was a newline.
    /// Avoids O(n) `ends_with` scans by maintaining state.
    last_char_newline: bool,
    /// Space after mapping key colon is deferred until the value is known.
    /// Cleared without emitting when the value is a nested collection.
    pending_space: bool,
    /// The first key of a mapping opened inline after "- " must not call
    /// `write_indent` — the dash already placed the cursor at the right column.
    first_key_after_dash: bool,
    /// The first item of a sequence opened inline after an outer "- " must not
    /// call `write_indent` — the outer dash already positioned the cursor.
    first_item_after_dash: bool,
    /// Collection start not yet written; an immediately following end event
    /// turns it into an empty flow collection (`[]` / `{}`).
    pending_start: Option<PendingStart>,
    /// Backend providing context stack and anchor storage
    backend: B,
}

impl<'a, B: FormatterBackend> StreamingFormatter<'a, B> {
    /// Creates a new formatter with the given configuration and backend.
    ///
    /// # Arguments
    ///
    /// * `config` - Emitter configuration (indent, `explicit_start`, etc.)
    /// * `output_capacity` - Initial capacity for output buffer
    /// * `backend` - Backend providing context stack and anchor storage
    pub fn new(config: &'a EmitterConfig, output_capacity: usize, backend: B) -> Self {
        Self {
            config,
            indent: config.indent.clamp(MIN_INDENT, MAX_INDENT),
            output: String::with_capacity(output_capacity),
            columns: Vec::new(),
            pending_newline: false,
            last_char_newline: true, // Empty buffer conceptually "ends with" newline
            pending_space: false,
            first_key_after_dash: false,
            first_item_after_dash: false,
            pending_start: None,
            backend,
        }
    }

    /// Returns the current YAML structure context.
    ///
    /// # Invariant
    /// The context stack is initialized with `Context::Root` and is never
    /// fully emptied. The `unwrap_or` is a defensive fallback.
    fn current_context(&self) -> Context {
        *self
            .backend
            .context_stack()
            .last()
            .unwrap_or(&Context::Root)
    }

    /// Returns the entry column of the innermost open collection (0 at root).
    fn column(&self) -> Column {
        self.columns.last().copied().unwrap_or(Column(0))
    }

    /// Returns the entry column of a collection opened in context `ctx`.
    fn child_column(&self, ctx: Context) -> Column {
        let parent = self.column();
        match ctx {
            Context::Root => parent,
            Context::Sequence => Column(parent.0 + DASH_WIDTH),
            Context::MappingKey
            | Context::MappingValue
            | Context::ExplicitKey
            | Context::ExplicitValue => Column(parent.0 + self.indent),
        }
    }

    /// Replaces the context on top of the stack.
    fn set_context(&mut self, ctx: Context) {
        if let Some(last) = self.backend.context_stack_mut().last_mut() {
            *last = ctx;
        }
    }

    /// Emits node properties (`&anchorN` and/or the tag), space-separated.
    ///
    /// Returns true if anything was written. No leading or trailing separator is emitted.
    fn emit_properties(&mut self, anchor_id: usize, tag: Option<&Tag>) -> bool {
        let has_anchor = anchor_id > 0 && anchor_id <= MAX_ANCHOR_ID;
        if has_anchor {
            self.backend.anchor_store_mut().ensure_capacity(anchor_id);
            let name = self.backend.anchor_store_mut().set_if_empty(anchor_id);
            self.output.push('&');
            self.output.push_str(name);
        }
        if let Some(tag) = tag {
            if has_anchor {
                self.output.push(' ');
            }
            write_tag(&mut self.output, tag);
        }
        let wrote = has_anchor || tag.is_some();
        if wrote {
            self.last_char_newline = false;
        }
        wrote
    }

    /// Like `emit_properties`, but prefixed with one space that is written only if
    /// properties exist.
    fn emit_properties_after_space(&mut self, anchor_id: usize, tag: Option<&Tag>) {
        let mark = self.output.len();
        self.output.push(' ');
        if !self.emit_properties(anchor_id, tag) {
            self.output.truncate(mark);
        }
    }

    /// Writes the `:` that introduces the value of an explicit (`? `) key.
    fn begin_explicit_value(&mut self) {
        if self.current_context() == Context::ExplicitValue {
            self.write_indent();
            self.output.push(':');
            self.pending_space = true;
            self.last_char_newline = false;
            self.set_context(Context::MappingValue);
        }
    }

    /// Processes a parser event and updates formatter state.
    pub fn format_event(&mut self, event: Event<'_>, _span: Span) {
        if !matches!(event, Event::SequenceEnd | Event::MappingEnd) {
            self.flush_pending_start();
        }

        match event {
            Event::DocumentStart(explicit) => {
                if explicit || self.config.explicit_start {
                    self.output.push_str("---");
                    self.pending_newline = true;
                    self.last_char_newline = false;
                }
            }

            Event::DocumentEnd => {
                if !self.last_char_newline && !self.output.is_empty() {
                    self.output.push('\n');
                    self.last_char_newline = true;
                }
            }

            Event::Scalar(value, style, anchor_id, tag) => {
                self.emit_scalar(&value, style, anchor_id, tag.as_deref());
            }

            Event::SequenceStart(anchor_id, tag) => {
                self.defer_collection(CollectionKind::Sequence, anchor_id, tag);
            }

            Event::SequenceEnd => {
                self.end_collection(CollectionKind::Sequence);
            }

            Event::MappingStart(anchor_id, tag) => {
                self.defer_collection(CollectionKind::Mapping, anchor_id, tag);
            }

            Event::MappingEnd => {
                self.end_collection(CollectionKind::Mapping);
            }

            Event::Alias(anchor_id) => {
                self.emit_alias(anchor_id);
            }

            // Events that require no action
            Event::StreamStart | Event::StreamEnd | Event::Nothing => {}
        }
    }

    fn emit_scalar(
        &mut self,
        value: &str,
        style: ScalarStyle,
        anchor_id: usize,
        tag: Option<&Tag>,
    ) {
        let style = effective_style(value, style);
        let shape = match style {
            ScalarStyle::Literal | ScalarStyle::Folded => NodeShape::BlockScalar,
            _ => NodeShape::Inline,
        };
        let ctx = self.begin_inline_node(shape);

        let wrote_properties = self.emit_properties(anchor_id, tag);
        let tagged_empty = tag.is_some() && style == ScalarStyle::Plain && value.is_empty();
        if wrote_properties && !tagged_empty {
            self.output.push(' ');
        }

        let typing = if tag.is_some() {
            ScalarTyping::Tagged
        } else {
            ScalarTyping::Implicit
        };
        self.emit_value_with_style(value, style, typing);

        self.end_inline_node(ctx, shape);
    }

    /// Writes the pending newline and the context prefix of a scalar, alias or empty
    /// flow collection; returns the context the node was written in.
    ///
    /// A block scalar in key position is written as an explicit `? ` key.
    fn begin_inline_node(&mut self, shape: NodeShape) -> Context {
        self.begin_explicit_value();
        let ctx = self.current_context();

        // Handle pending newline from document start or collection start
        self.flush_pending_newline();

        match ctx {
            Context::Sequence => self.write_dash_prefix(),
            Context::MappingKey => {
                self.write_key_indent();
                if shape == NodeShape::BlockScalar {
                    self.output.push_str("? ");
                    self.last_char_newline = false;
                }
            }
            // Root level needs no prefix; mapping value emits pending space.
            // Explicit* are consumed by begin_explicit_value and never current here.
            Context::Root | Context::ExplicitKey | Context::ExplicitValue => {}
            Context::MappingValue => {
                if self.pending_space {
                    self.output.push(' ');
                    self.pending_space = false;
                    self.last_char_newline = false;
                }
            }
        }
        ctx
    }

    /// Writes the context suffix of a node started by `begin_inline_node`.
    fn end_inline_node(&mut self, ctx: Context, shape: NodeShape) {
        match ctx {
            Context::MappingKey if shape == NodeShape::BlockScalar => {
                self.set_context(Context::ExplicitValue);
            }
            Context::MappingKey => {
                self.output.push(':');
                self.set_context(Context::MappingValue);
                // Defer the space — emitted only when the value is a scalar.
                // If the value is a nested collection, pending_space is cleared
                // without emitting so we avoid a trailing space before the newline.
                self.pending_space = true;
                self.last_char_newline = false;
            }
            Context::MappingValue | Context::Sequence | Context::Root => {
                // Block scalars already end with a newline.
                if shape == NodeShape::Inline {
                    self.output.push('\n');
                    self.last_char_newline = true;
                }
                if ctx == Context::MappingValue {
                    self.set_context(Context::MappingKey);
                }
            }
            Context::ExplicitKey | Context::ExplicitValue => {}
        }
    }

    /// Writes the newline deferred after `---`, so the next node starts on its own line.
    fn flush_pending_newline(&mut self) {
        if self.pending_newline {
            self.output.push('\n');
            self.pending_newline = false;
            self.last_char_newline = true;
        }
    }

    /// Writes the `"- "` prefix of a sequence entry.
    fn write_dash_prefix(&mut self) {
        if self.first_item_after_dash {
            self.first_item_after_dash = false;
        } else {
            self.write_indent();
        }
        self.output.push_str("- ");
        self.last_char_newline = false;
    }

    /// Writes the indentation of a mapping key, unless an outer dash already placed the cursor.
    fn write_key_indent(&mut self) {
        if self.first_key_after_dash {
            self.first_key_after_dash = false;
        } else {
            self.write_indent();
        }
    }

    fn emit_value_with_style(&mut self, value: &str, style: ScalarStyle, typing: ScalarTyping) {
        match style {
            ScalarStyle::Plain => {
                // Fix special floats for YAML 1.2 compliance; an explicit tag pins the type.
                let fixed = match typing {
                    ScalarTyping::Tagged => value,
                    ScalarTyping::Implicit if value.is_empty() => "null",
                    ScalarTyping::Implicit => super::fix_special_float_value(value),
                };
                self.output.push_str(fixed);
                self.last_char_newline = false;
            }
            ScalarStyle::SingleQuoted => {
                self.output.push('\'');
                // Single quotes: escape single quotes by doubling
                for c in value.chars() {
                    if c == '\'' {
                        self.output.push_str("''");
                    } else {
                        self.output.push(c);
                    }
                }
                self.output.push('\'');
                self.last_char_newline = false;
            }
            ScalarStyle::DoubleQuoted => {
                self.output.push('"');
                // Double quotes: escape special characters
                for c in value.chars() {
                    match c {
                        '"' => self.output.push_str("\\\""),
                        '\\' => self.output.push_str("\\\\"),
                        '\n' => self.output.push_str("\\n"),
                        '\r' => self.output.push_str("\\r"),
                        '\t' => self.output.push_str("\\t"),
                        '\0' => self.output.push_str("\\0"),
                        c if c.is_control() => {
                            let _ = write!(self.output, "\\x{:02X}", u32::from(c));
                        }
                        '\u{FFFE}' | '\u{FFFF}' => {
                            let _ = write!(self.output, "\\u{:04X}", u32::from(c));
                        }
                        _ => self.output.push(c),
                    }
                }
                self.output.push('"');
                self.last_char_newline = false;
            }
            ScalarStyle::Literal | ScalarStyle::Folded => {
                let block = if style == ScalarStyle::Folded {
                    BlockStyle::Folded
                } else {
                    BlockStyle::Literal
                };
                self.emit_block_scalar(value, block);
            }
        }
    }

    /// Writes a literal or folded block scalar, always ending with a newline.
    ///
    /// Folded values that contain more-indented lines fall back to literal style,
    /// since those lines are exempt from folding.
    fn emit_block_scalar(&mut self, value: &str, requested: BlockStyle) {
        let body = value.trim_end_matches('\n');
        let trailing = value.len() - body.len();
        let folded = requested == BlockStyle::Folded
            && !body.is_empty()
            && !body.split('\n').any(|line| line.starts_with([' ', '\t']));
        let content_col = self.column().0 + self.indent;

        let indicator = if folded { '>' } else { '|' };
        self.output
            .push_str(&block_scalar_header(indicator, value, self.indent));
        self.output.push('\n');

        if body.is_empty() {
            for _ in 0..trailing {
                self.output.push('\n');
            }
        } else {
            // Blank lines get no indentation: it would create trailing whitespace.
            let mut seen_text = false;
            for line in body.split('\n') {
                if line.is_empty() {
                    self.output.push('\n');
                    continue;
                }
                // A folded line break between text lines is a blank line in the source.
                if folded && seen_text {
                    self.output.push('\n');
                }
                seen_text = true;
                self.push_spaces(content_col);
                self.output.push_str(line);
                self.output.push('\n');
            }
            for _ in 1..trailing {
                self.output.push('\n');
            }
        }
        self.last_char_newline = true;
    }

    /// Holds a collection start back until the next event shows whether it is empty.
    fn defer_collection(
        &mut self,
        kind: CollectionKind,
        anchor_id: usize,
        tag: Option<Cow<'_, Tag>>,
    ) {
        self.pending_start = Some(PendingStart {
            kind,
            anchor_id,
            tag: tag.map(Cow::into_owned),
        });
    }

    /// Writes a deferred collection start, now known to be non-empty.
    fn flush_pending_start(&mut self) {
        if let Some(PendingStart {
            kind,
            anchor_id,
            tag,
        }) = self.pending_start.take()
        {
            self.start_collection(kind, anchor_id, tag.as_ref());
        }
    }

    /// Writes a collection start known to be non-empty and opens its context and column.
    fn start_collection(&mut self, kind: CollectionKind, anchor_id: usize, tag: Option<&Tag>) {
        self.begin_explicit_value();
        let ctx = self.current_context();

        // Handle pending newline
        self.flush_pending_newline();

        // Write prefix and properties inline per context to avoid them landing on the wrong line.
        match ctx {
            Context::Sequence => {
                self.write_dash_prefix();
                if self.emit_properties(anchor_id, tag) {
                    // Properties occupy the "- " line, so children need fresh indentation.
                    self.output.push('\n');
                    self.last_char_newline = true;
                } else if kind == CollectionKind::Sequence {
                    self.first_item_after_dash = true;
                } else {
                    self.first_key_after_dash = true;
                }
            }
            Context::MappingKey => {
                // Collection as mapping key: explicit `?` entry, children on following lines.
                self.write_key_indent();
                self.output.push('?');
                self.emit_properties_after_space(anchor_id, tag);
                self.output.push('\n');
                self.last_char_newline = true;
            }
            Context::MappingValue => {
                // The pending_space after colon is dropped: "key: &anchor\n" or "key:\n".
                self.pending_space = false;
                self.emit_properties_after_space(anchor_id, tag);
                self.output.push('\n');
                self.last_char_newline = true;
            }
            // Explicit* are consumed by begin_explicit_value and never current here.
            Context::Root | Context::ExplicitKey | Context::ExplicitValue => {
                if self.emit_properties(anchor_id, tag) {
                    self.output.push('\n');
                    self.last_char_newline = true;
                }
            }
        }

        match ctx {
            Context::MappingValue => self.set_context(Context::MappingKey),
            Context::MappingKey => self.set_context(Context::ExplicitKey),
            _ => {}
        }

        // Push child context and column together (with depth limit)
        if self.backend.context_stack().len() < MAX_DEPTH {
            let column = self.child_column(ctx);
            self.backend.context_stack_mut().push(kind.entry_context());
            self.columns.push(column);
        }
    }

    fn end_collection(&mut self, kind: CollectionKind) {
        if let Some(PendingStart { anchor_id, tag, .. }) =
            self.pending_start.take_if(|pending| pending.kind == kind)
        {
            let ctx = self.begin_inline_node(NodeShape::Inline);
            if self.emit_properties(anchor_id, tag.as_ref()) {
                self.output.push(' ');
            }
            self.output.push_str(kind.empty_flow());
            self.last_char_newline = false;
            self.end_inline_node(ctx, NodeShape::Inline);
            return;
        }

        self.backend.context_stack_mut().pop();
        self.columns.pop();
        if self.current_context() == Context::ExplicitKey {
            self.set_context(Context::ExplicitValue);
        }
    }

    fn emit_alias(&mut self, anchor_id: usize) {
        let ctx = self.begin_inline_node(NodeShape::Inline);

        // Emit the alias reference
        self.output.push('*');
        if let Some(name) = self.backend.anchor_store().get(anchor_id) {
            self.output.push_str(name);
        } else {
            // Fallback: generate name directly into output
            let _ = write!(self.output, "anchor{anchor_id}");
        }
        self.last_char_newline = false;

        // Anchor names may contain ':', so `*a:` would read as alias "a:".
        if ctx == Context::MappingKey {
            self.output.push(' ');
        }

        self.end_inline_node(ctx, NodeShape::Inline);
    }

    /// Appends `count` spaces, slicing a static buffer when it is long enough.
    fn push_spaces(&mut self, count: usize) {
        if count <= INDENT_SPACES.len() {
            self.output.push_str(&INDENT_SPACES[..count]);
        } else {
            self.output.push_str(&" ".repeat(count));
        }
    }

    fn write_indent(&mut self) {
        let column = self.column();
        if column.0 > 0 {
            self.push_spaces(column.0);
            self.last_char_newline = false;
        }
    }

    /// Completes formatting and returns the output string.
    pub fn finish(mut self) -> String {
        // Ensure output ends with newline
        if !self.output.is_empty() && !self.last_char_newline {
            self.output.push('\n');
        }
        self.output
    }
}

#[cfg(test)]
mod tests {
    use saphyr_parser::{Event, Parser};

    use crate::EmitterConfig;
    use crate::streaming::format_streaming;

    fn fmt(yaml: &str) -> String {
        format_streaming(yaml, &EmitterConfig::default()).unwrap()
    }

    /// Parser events with tags, anchors, and values but without spans, styles, or `---` markers.
    fn events(yaml: &str) -> Vec<String> {
        let tag = |t: Option<&saphyr_parser::Tag>| t.map(|t| format!("{}{}", t.handle, t.suffix));
        Parser::new_from_str(yaml)
            .map(|r| match r.unwrap().0 {
                Event::DocumentStart(_) => "doc".to_owned(),
                Event::Scalar(v, _, a, t) => format!("scalar {v:?} &{a} {:?}", tag(t.as_deref())),
                Event::SequenceStart(a, t) => format!("seq &{a} {:?}", tag(t.as_deref())),
                Event::MappingStart(a, t) => format!("map &{a} {:?}", tag(t.as_deref())),
                other => format!("{other:?}"),
            })
            .collect()
    }

    /// Formats twice and checks idempotency and that the event stream is unchanged.
    fn assert_stable(yaml: &str) -> String {
        let once = fmt(yaml);
        assert_eq!(fmt(&once), once, "not idempotent for {yaml:?}");
        assert_eq!(
            events(&once),
            events(yaml),
            "events changed: {yaml:?} -> {once:?}"
        );
        once
    }

    #[test]
    fn tags_preserved_on_scalars() {
        let out = assert_stable(
            "t: !!str 123\nu: !!str true\nv: !!int \"7\"\nc: !custom x\nb: !!binary aGk=\n",
        );
        assert_eq!(
            out,
            "t: !!str 123\nu: !!str true\nv: !!int \"7\"\nc: !custom x\nb: !!binary aGk=\n"
        );
    }

    #[test]
    fn tags_preserved_on_collections() {
        // Omitted set values become explicit nulls (#319), so events differ by design
        let set = fmt("s: !!set {a, b}\n");
        assert_eq!(set, "s: !!set\n  a: null\n  b: null\n");
        assert_eq!(fmt(&set), set);
        assert_eq!(assert_stable("- !!seq [a]\n"), "- !!seq\n  - a\n");
        assert_eq!(assert_stable("&a !!map {k: v}\n"), "&a !!map\nk: v\n");
    }

    #[test]
    fn tagged_empty_scalar_stays_empty_without_trailing_space() {
        for yaml in ["a: !!str\nb: 1\n", "- !!str\n- x\n"] {
            let out = assert_stable(yaml);
            assert!(out.contains("!!str\n"), "{out:?}");
            assert!(!out.lines().any(|l| l.ends_with(' ')), "{out:?}");
        }
    }

    #[test]
    fn tagged_special_float_untouched() {
        assert_eq!(assert_stable("a: !!str inf\n"), "a: !!str inf\n");
    }

    #[test]
    fn tag_forms_round_trip() {
        let out = assert_stable("%TAG !e! tag:example.com,2000:\n---\na: !e!x 1\n");
        assert!(out.contains("!<tag:example.com,2000:x> 1"), "{out:?}");
        assert!(assert_stable("a: !<tag:x.org,1:y> z\n").contains("!<tag:x.org,1:y> z"));
        assert!(assert_stable("q: !\n").contains("q: !"));
    }

    #[test]
    fn tag_suffix_is_percent_encoded() {
        assert_eq!(assert_stable("a: !foo%20bar x\n"), "a: !foo%20bar x\n");
        assert_eq!(assert_stable("a: !foo%21bar x\n"), "a: !foo%21bar x\n");
        assert_eq!(assert_stable("a: !foo%2Cbar x\n"), "a: !foo%2Cbar x\n");
        let out = assert_stable("a: !<tag:x.org,2000:a%20b> z\n");
        assert!(out.contains("!<tag:x.org,2000:a%20b> z"), "{out:?}");
    }

    #[test]
    fn anchor_and_tag_together() {
        assert_stable("a: &x !!str 1\nb: *x\n");
        assert_stable("- &x !custom y\n- *x\n");
        assert_stable("- &x !!seq [a]\n- *x\n");
        assert_stable("a: &x !!map {k: v}\nb: *x\n");
    }

    #[test]
    fn tagged_empty_and_quoted_scalars() {
        assert_stable("a: !!null\nb: 1\n");
        assert_stable("- !!null\n- x\n");
        assert_eq!(
            assert_stable("a: !!str \"x y\"\nb: !t 'q'\n"),
            "a: !!str \"x y\"\nb: !t 'q'\n"
        );
    }

    #[test]
    fn tagged_block_scalar() {
        assert_eq!(
            assert_stable("a: !!str |+\n  x\n\n"),
            "a: !!str |+\n  x\n\n"
        );
        assert_eq!(assert_stable("- !t >\n  x\n"), "- !t >\n  x\n");
    }

    #[test]
    fn tags_in_multi_document_stream() {
        let out = assert_stable("%TAG !e! tag:e.com,2000:\n---\na: !e!x 1\n---\nb: !!str 2\n");
        assert!(out.contains("!<tag:e.com,2000:x> 1"), "{out:?}");
        assert!(out.contains("b: !!str 2"), "{out:?}");
    }

    #[test]
    fn keep_chomp_stable() {
        assert_eq!(assert_stable("a: |+\n  x\n\n"), "a: |+\n  x\n\n");
        assert_eq!(
            assert_stable("a: |+\n  x\n\n\nb: 1\n"),
            "a: |+\n  x\n\n\nb: 1\n"
        );
        assert_eq!(
            assert_stable("a: |+\n  x\n\n\n\nb: 1\n"),
            "a: |+\n  x\n\n\n\nb: 1\n"
        );
    }

    #[test]
    fn keep_chomp_in_containers() {
        assert_eq!(assert_stable("- |+\n  x\n\n- y\n"), "- |+\n  x\n\n- y\n");
        assert_stable("a:\n  b: |+\n    x\n\nc: 1\n");
        assert_stable("a: |+\n  x\n\n---\nb: |+\n  y\n\n");
        assert_stable("a: >+\n  x\n\nb: 1\n");
        assert_stable("- >+\n  x\n\n\n- y\n");
    }

    #[test]
    fn strip_chomp_with_trailing_blanks_stable() {
        assert_eq!(
            assert_stable("a: |-\n  x\n\n\nb: 1\n"),
            "a: |-\n  x\nb: 1\n"
        );
    }

    #[test]
    fn clip_block_has_no_extra_blank_line() {
        assert_eq!(fmt("a: |\n  x\nb: 1\n"), "a: |\n  x\nb: 1\n");
        assert_eq!(fmt("- >\n  x\n- y\n"), "- >\n  x\n- y\n");
    }

    #[test]
    fn complex_keys_valid_and_stable() {
        for yaml in [
            "? [a, b]\n: c\n",
            "? {a: 1}\n: v\n",
            "? |\n  block key\n: v\n",
            "? >\n  folded key\n: v\n",
            "k: 1\n? [a]\n: [b]\n",
            "- ? [a]\n  : b\n",
            "- c: d\n  ? [a]\n  : b\n",
            "- ? |\n    k\n  : v\n",
            "outer:\n  ? [a]\n  : b\n",
            "? [a]\n: [b]\n? [c]\n: d\n",
            "? [[a], b]\n: c\n",
            "? {? [a] : b}\n: c\n",
            "? [a]\n: |\n  text\n",
            "? &k [a]\n: b\nc: *k\n",
            "? !!seq [a]\n: b\n",
            "? [a]\n: b\n---\n? {c: d}\n: e\n",
        ] {
            assert_stable(yaml);
        }
        assert_eq!(fmt("? [a, b]\n: c\n"), "?\n  - a\n  - b\n: c\n");
        assert_eq!(fmt("? |\n  block key\n: v\n"), "? |\n  block key\n: v\n");
    }

    #[test]
    fn complex_key_indent_4() {
        let config = EmitterConfig::new().with_indent(4);
        let yaml = "? [a, b]\n: c\n";
        let once = format_streaming(yaml, &config).unwrap();
        assert_eq!(format_streaming(&once, &config).unwrap(), once);
        assert_eq!(events(&once), events(yaml));
    }

    #[test]
    fn alias_keys_get_space_before_colon() {
        for yaml in [
            "&k a: 1\n? *k\n: 2\n",
            "&k a: *k\n*k : *k\n",
            "&k a: 1\n*k :\n  x: 1\n",
            "&k a: 1\n*k : [1, 2]\n",
            "x:\n  y:\n    &k a: 1\n    *k : 2\n",
            "x: {&k a: 1, *k : 2}\n",
            "- &k a\n- *k : 1\n",
            "- &k a: 1\n  *k : 2\n",
        ] {
            assert_stable(yaml);
        }
        assert_eq!(fmt("&k a: 1\n? *k\n: 2\n"), "&k a: 1\n*k : 2\n");
        assert_eq!(fmt("- &k a\n- *k : 1\n"), "- &k a\n- *k : 1\n");
    }

    #[test]
    fn alias_keys_indent_4() {
        let config = EmitterConfig::new().with_indent(4);
        let yaml = "x:\n  &k a: 1\n  *k : 2\n";
        let once = format_streaming(yaml, &config).unwrap();
        assert_eq!(format_streaming(&once, &config).unwrap(), once);
        assert_eq!(events(&once), events(yaml));
    }

    #[test]
    fn multiline_quoted_and_plain_scalars_stay_valid() {
        for yaml in [
            "? 'a\n\n  b'\n: 1\n",
            "k: 'a\n\n  b'\n",
            "- 'a\n\n  b'\n",
            "k: a\n\n  b\n",
            "? a\n\n  b\n: 1\n",
            "? \"a\\n\\nb\"\n: 1\n",
            "? &k 'a\n\n  b'\n: 1\n",
            "? !!str 'a\n\n  b'\n: 1\n",
            "x:\n  y:\n    - ? 'a\n\n        b'\n      : 1\n",
            "'a\n\n  b'\n",
            "\"a\\u0001b\"\n",
            "k: \"a\\rb\"\n",
        ] {
            assert_stable(yaml);
        }
        assert_eq!(fmt("? 'a\n\n  b'\n: 1\n"), "\"a\\nb\": 1\n");
        assert_eq!(fmt("- 'a\n\n  b'\n"), "- \"a\\nb\"\n");
        assert_eq!(fmt("k: 'x\ty'\n"), "k: 'x\ty'\n");
        assert_eq!(fmt("k: \"a\\u0001b\"\n"), "k: \"a\\x01b\"\n");
    }

    #[test]
    fn multiline_scalar_indent_4() {
        let config = EmitterConfig::new().with_indent(4);
        let yaml = "x:\n  k: 'a\n\n    b'\n";
        let once = format_streaming(yaml, &config).unwrap();
        assert_eq!(format_streaming(&once, &config).unwrap(), once);
        assert_eq!(events(&once), events(yaml));
    }

    #[test]
    fn block_scalar_under_alias_key_stays_valid() {
        for yaml in [
            "&k a: 1\nb:\n  c:\n    *k : |\n      text\n",
            "- &k a\n- *k : |\n    text\n",
            "&k a: 1\nb:\n  c:\n    *k : >+\n      text\n\n",
        ] {
            assert_stable(yaml);
        }
    }

    #[test]
    fn nested_alias_key_with_multiline_value_indent_4() {
        let config = EmitterConfig::new().with_indent(4);
        let yaml = "&k a: 1\nx:\n  y:\n    *k : \"p\\nq\"\n    z: 2\n";
        let once = format_streaming(yaml, &config).unwrap();
        assert_eq!(format_streaming(&once, &config).unwrap(), once);
        assert_eq!(events(&once), events(yaml));
    }

    #[test]
    fn non_characters_and_c1_controls_are_escaped() {
        assert_eq!(
            fmt("k: \"a\\uFFFEb\\uFFFF\"\n"),
            "k: \"a\\uFFFEb\\uFFFF\"\n"
        );
        assert_eq!(fmt("k: \"a\\x7Fb\\x85\"\n"), "k: \"a\\x7Fb\\x85\"\n");
        assert_stable("k: \"a\\uFFFEb\"\n");
    }
}
