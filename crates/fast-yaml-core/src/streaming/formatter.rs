//! Generic streaming formatter with pluggable backend.
//!
//! This module contains the core formatting logic abstracted over
//! different memory allocation strategies via the `FormatterBackend` trait.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write;

use saphyr_parser::{Event, ScalarStyle, Span, Tag};

use super::anchors::AnchorNames;
use super::directives::DirectiveScanner;
use super::traits::{AnchorStoreOps, ContextStackOps, FormatterBackend};
use super::{Context, INDENT_SPACES, MAX_ANCHOR_ID, MAX_IMPLICIT_KEY_CHARS};
use crate::emitter::{EmitterConfig, block_scalar_header};
use crate::error::{EmitError, EmitResult};
use crate::limits::Indent;

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

/// Whether `c` is a YAML non-printable that only a double-quoted escape can represent (tab excluded),
/// a byte order mark, which the loader drops where it starts a document, or U+2028/U+2029, which
/// YAML 1.1 readers take for line breaks.
fn needs_escape(c: char) -> bool {
    (c.is_control() && c != '\t')
        || matches!(
            c,
            '\u{FFFE}' | '\u{FFFF}' | '\u{FEFF}' | '\u{2028}' | '\u{2029}'
        )
}

/// Whether a plain scalar with this value would not be read back as itself in block context:
/// it starts with an indicator, or reads as a `---` / `...` marker or a `-`/`?`/`:` entry.
///
/// Flow-context plain scalars may legally start with characters such as `|` or `%`.
pub fn is_unsafe_plain(value: &str) -> bool {
    let followed_by_blank = |rest: &str| rest.is_empty() || rest.starts_with([' ', '\t']);
    value.starts_with([
        '|', '>', '%', '@', '`', '\'', '"', '&', '*', '!', '#', ',', '[', ']', '{', '}',
    ]) || ["---", "..."]
        .iter()
        .any(|marker| value.strip_prefix(marker).is_some_and(followed_by_blank))
        || value
            .strip_prefix(['-', '?', ':'])
            .is_some_and(followed_by_blank)
}

/// Appends `value` as a single-quoted scalar, doubling embedded quotes.
pub fn write_single_quoted(out: &mut String, value: &str) {
    out.push('\'');
    for c in value.chars() {
        if c == '\'' {
            out.push_str("''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
}

/// Appends `value` as a double-quoted scalar, escaping quotes, backslashes and control characters.
pub fn write_double_quoted(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if c.is_control() => {
                let _ = write!(out, "\\x{:02X}", u32::from(c));
            }
            '\u{FFFE}' | '\u{FFFF}' | '\u{FEFF}' | '\u{2028}' | '\u{2029}' => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            _ => out.push(c),
        }
    }
    out.push('"');
}

/// Characters YAML 1.1 readers take for line breaks (NEL, LS, PS); a block scalar cannot hold them.
const YAML_11_BREAKS: [char; 3] = ['\u{85}', '\u{2028}', '\u{2029}'];

/// Returns the style a scalar must be written in to round-trip its value.
///
/// Plain and single-quoted scalars cannot represent control characters other than tab
/// (a raw line break is folded on re-parse), so they are promoted to double-quoted.
/// A plain scalar that block context would misread (see [`is_unsafe_plain`]) is single-quoted.
pub fn effective_style(value: &str, style: ScalarStyle) -> ScalarStyle {
    match style {
        ScalarStyle::Plain | ScalarStyle::SingleQuoted if value.chars().any(needs_escape) => {
            ScalarStyle::DoubleQuoted
        }
        ScalarStyle::Plain if is_unsafe_plain(value) => ScalarStyle::SingleQuoted,
        ScalarStyle::Literal | ScalarStyle::Folded if value.contains(YAML_11_BREAKS) => {
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

/// The two UTF-8 bytes of a character that saphyr-parser decoded from a `%XX%YY` escape.
///
/// It combines the bytes of a two-byte sequence into one 16-bit value instead of decoding them
/// (`%D1%82`, the letter `т`, arrives as U+D182). Longer sequences are rejected by the parser
/// and raw non-ASCII tag text never reaches the formatter, so a character in this range can
/// only be that misreading. Returns `None` for every other character.
fn misdecoded_escape(c: char) -> Option<[u8; 2]> {
    let code = u32::from(c);
    let [_, _, hi, lo] = code.to_be_bytes();
    let is_pair = (0xC2..=0xD7).contains(&hi) && (0x80..=0xBF).contains(&lo);
    is_pair.then_some([hi, lo])
}

/// Appends `text`, percent-encoding bytes outside the allowed tag charset.
fn push_tag_text(out: &mut String, text: &str, form: TagForm) {
    for c in text.chars() {
        if let Some(bytes) = misdecoded_escape(c) {
            for b in bytes {
                let _ = write!(out, "%{b:02X}");
            }
            continue;
        }
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
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScalarTyping {
    Tagged,
    Implicit,
}

/// Where the next block entry of a collection starts.
///
/// The first key of a mapping or the first item of a sequence opened inline after `"- "` must
/// not call `write_indent`: the dash already placed the cursor at the right column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cursor {
    /// At the start of a line: the entry writes its own indentation.
    Normal,
    /// Right after `"- "` on the same line.
    AfterDash,
}

/// Generic streaming formatter with pluggable backend.
///
/// This struct contains ALL formatting logic and is parameterized over
/// the backend type `B: FormatterBackend`. Through monomorphization,
/// this compiles to specialized code for each backend with zero runtime cost.
pub struct StreamingFormatter<'a, B: FormatterBackend> {
    config: &'a EmitterConfig,
    indent: Indent,
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
    /// Where the next block entry starts; see [`Cursor`].
    cursor: Cursor,
    /// Collection start not yet written; an immediately following end event
    /// turns it into an empty flow collection (`[]` / `{}`).
    pending_start: Option<PendingStart>,
    /// Highest anchor id of earlier documents; parser ids are stream-global.
    anchor_base: usize,
    /// Highest anchor id defined so far.
    max_anchor_id: usize,
    /// Number of documents started so far; every one after the first needs an explicit `---`.
    docs_started: usize,
    /// Anchor id most recently emitted under each name in the current document, so that no two
    /// live anchors share a name and every alias resolves to its own anchor.
    name_owner: HashMap<String, usize>,
    /// Original anchor names, recovered from the source between events.
    anchor_names: AnchorNames<'a>,
    /// Directive lines of the source, re-emitted before the documents that had them.
    directives: DirectiveScanner<'a>,
    /// Backend providing context stack and anchor storage
    backend: B,
}

impl<B: FormatterBackend> crate::emitter::EventSink for StreamingFormatter<'_, B> {
    fn event(&mut self, event: Event<'_>) -> EmitResult<()> {
        self.format_event(event, Span::default())
    }
}

impl<'a, B: FormatterBackend> StreamingFormatter<'a, B> {
    /// Creates a new formatter with the given configuration and backend.
    ///
    /// # Arguments
    ///
    /// * `config` - Emitter configuration (indent, `explicit_start`, etc.)
    /// * `output_capacity` - Initial capacity for output buffer
    /// * `backend` - Backend providing context stack and anchor storage
    /// * `source` - Input text, scanned for `%YAML` / `%TAG` directives
    pub fn new(
        config: &'a EmitterConfig,
        output_capacity: usize,
        backend: B,
        source: &'a str,
    ) -> Self {
        Self {
            config,
            indent: config.indent,
            output: String::with_capacity(output_capacity),
            columns: Vec::new(),
            pending_newline: false,
            last_char_newline: true, // Empty buffer conceptually "ends with" newline
            pending_space: false,
            cursor: Cursor::Normal,
            pending_start: None,
            anchor_base: 0,
            max_anchor_id: 0,
            docs_started: 0,
            name_owner: HashMap::new(),
            anchor_names: AnchorNames::new(source),
            directives: DirectiveScanner::new(source),
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
            | Context::ExplicitValue => Column(parent.0 + self.indent.get()),
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
    fn emit_properties(&mut self, anchor_id: usize, tag: Option<&Tag>) -> EmitResult<bool> {
        if anchor_id.saturating_sub(self.anchor_base) > MAX_ANCHOR_ID {
            return Err(EmitError::AnchorLimitExceeded {
                limit: MAX_ANCHOR_ID,
            });
        }
        self.max_anchor_id = self.max_anchor_id.max(anchor_id);
        let has_anchor = anchor_id > 0;
        if has_anchor {
            self.backend.anchor_store_mut().ensure_capacity(anchor_id);
            let name = self.backend.anchor_store_mut().set_if_empty(anchor_id);
            if let Some(owner) = self.name_owner.get_mut(name) {
                *owner = anchor_id;
            } else {
                self.name_owner.insert(name.to_owned(), anchor_id);
            }
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
        Ok(wrote)
    }

    /// Like `emit_properties`, but prefixed with one space that is written only if
    /// properties exist.
    fn emit_properties_after_space(
        &mut self,
        anchor_id: usize,
        tag: Option<&Tag>,
    ) -> EmitResult<()> {
        let mark = self.output.len();
        self.output.push(' ');
        if !self.emit_properties(anchor_id, tag)? {
            self.output.truncate(mark);
        }
        Ok(())
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
    ///
    /// # Errors
    ///
    /// Returns [`EmitError::DepthLimitExceeded`] or [`EmitError::AnchorLimitExceeded`] when
    /// the document exceeds the formatter limits.
    pub fn format_event(&mut self, event: Event<'_>, span: Span) -> EmitResult<()> {
        if !matches!(event, Event::SequenceEnd | Event::MappingEnd) {
            self.flush_pending_start()?;
        }
        if let Event::Scalar(_, _, anchor_id, _)
        | Event::SequenceStart(anchor_id, _)
        | Event::MappingStart(anchor_id, _) = &event
        {
            self.name_anchor(*anchor_id, span);
        }
        self.anchor_names.advance(&event, span);
        self.dispatch(event, span)
    }

    /// Gives a new anchor its original name, or leaves it to be generated as `anchor{id}`.
    ///
    /// A name already used by another anchor of the document is not reused, so an alias can
    /// never bind to the wrong definition.
    fn name_anchor(&mut self, anchor_id: usize, span: Span) {
        if anchor_id == 0 {
            return;
        }
        let recovered = self
            .anchor_names
            .name_before(span)
            .filter(|name| !self.name_owner.contains_key(*name));
        let store = self.backend.anchor_store_mut();
        store.ensure_capacity(anchor_id);
        if store.get(anchor_id).is_some() {
            return;
        }
        if let Some(name) = recovered {
            store.set_name(anchor_id, name);
            return;
        }
        let mut generated = format!("anchor{anchor_id}");
        let mut attempt = 0;
        while self.name_owner.contains_key(&generated) {
            attempt += 1;
            generated = format!("anchor{anchor_id}_{attempt}");
        }
        store.set_name(anchor_id, &generated);
    }

    fn dispatch(&mut self, event: Event<'_>, span: Span) -> EmitResult<()> {
        match event {
            Event::DocumentStart(explicit) => {
                self.name_owner.clear();
                self.anchor_base = self.max_anchor_id;
                if explicit || self.config.explicit_start || self.docs_started > 0 {
                    if explicit && let Some(directives) = self.directives.before(span.start.line())
                    {
                        if self.docs_started > 0 {
                            self.output.push_str("...\n");
                        }
                        self.output.push_str(&directives);
                    }
                    self.output.push_str("---");
                    self.pending_newline = true;
                    self.last_char_newline = false;
                }
                self.docs_started += 1;
            }

            Event::DocumentEnd => {
                if !self.last_char_newline && !self.output.is_empty() {
                    self.output.push('\n');
                    self.last_char_newline = true;
                }
            }

            Event::Scalar(value, style, anchor_id, tag) => {
                self.emit_scalar(&value, style, anchor_id, tag.as_deref())?;
            }

            Event::SequenceStart(anchor_id, tag) => {
                self.defer_collection(CollectionKind::Sequence, anchor_id, tag);
            }

            Event::SequenceEnd => {
                self.end_collection(CollectionKind::Sequence)?;
            }

            Event::MappingStart(anchor_id, tag) => {
                self.defer_collection(CollectionKind::Mapping, anchor_id, tag);
            }

            Event::MappingEnd => {
                self.end_collection(CollectionKind::Mapping)?;
            }

            Event::Alias(anchor_id) => {
                self.emit_alias(anchor_id);
            }

            // Events that require no action
            Event::StreamStart | Event::StreamEnd | Event::Nothing => {}
        }
        Ok(())
    }

    fn emit_scalar(
        &mut self,
        value: &str,
        style: ScalarStyle,
        anchor_id: usize,
        tag: Option<&Tag>,
    ) -> EmitResult<()> {
        let mut style = effective_style(value, style);
        // saphyr reads an empty root block scalar together with the next `---` as one document
        if value.is_empty()
            && matches!(style, ScalarStyle::Literal | ScalarStyle::Folded)
            && self.current_context() == Context::Root
        {
            style = ScalarStyle::DoubleQuoted;
        }
        let shape = match style {
            ScalarStyle::Literal | ScalarStyle::Folded => NodeShape::BlockScalar,
            _ => NodeShape::Inline,
        };
        let ctx = self.begin_inline_node(shape);
        let node_start = self.output.len();

        self.emit_scalar_node(value, style, anchor_id, tag)?;

        if self.is_too_long_key(ctx, shape, node_start) {
            self.begin_explicit_key(node_start);
            self.emit_scalar_node(value, style, anchor_id, tag)?;
            self.end_explicit_key(ctx);
        } else {
            // A tag directly before `:` would absorb it (`!:` is a tag).
            if ctx == Context::MappingKey
                && tag.is_some()
                && style == ScalarStyle::Plain
                && value.is_empty()
            {
                self.output.push(' ');
            }
            self.end_inline_node(ctx, shape);
        }
        Ok(())
    }

    /// Whether the inline node written since `node_start` exceeds the implicit key limit.
    fn is_too_long_key(&self, ctx: Context, shape: NodeShape, node_start: usize) -> bool {
        ctx == Context::MappingKey
            && shape == NodeShape::Inline
            && self.output[node_start..].chars().count() > MAX_IMPLICIT_KEY_CHARS
    }

    /// Rewinds to `node_start` and writes the explicit key indicator.
    fn begin_explicit_key(&mut self, node_start: usize) {
        self.output.truncate(node_start);
        self.output.push_str("? ");
    }

    /// Ends the explicit key's line, like a block scalar key.
    fn end_explicit_key(&mut self, ctx: Context) {
        self.output.push('\n');
        self.last_char_newline = true;
        self.end_inline_node(ctx, NodeShape::BlockScalar);
    }

    /// Writes a scalar's properties followed by its value.
    fn emit_scalar_node(
        &mut self,
        value: &str,
        style: ScalarStyle,
        anchor_id: usize,
        tag: Option<&Tag>,
    ) -> EmitResult<()> {
        let wrote_properties = self.emit_properties(anchor_id, tag)?;
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
        Ok(())
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
        if !self.take_after_dash() {
            self.write_indent();
        }
        self.output.push_str("- ");
        self.last_char_newline = false;
    }

    /// Writes the indentation of a mapping key, unless an outer dash already placed the cursor.
    fn write_key_indent(&mut self) {
        if !self.take_after_dash() {
            self.write_indent();
        }
    }

    /// Returns whether an outer dash left the cursor on this entry's line, and resets it.
    fn take_after_dash(&mut self) -> bool {
        std::mem::replace(&mut self.cursor, Cursor::Normal) == Cursor::AfterDash
    }

    fn emit_value_with_style(&mut self, value: &str, style: ScalarStyle, typing: ScalarTyping) {
        match style {
            ScalarStyle::Plain => {
                let fixed = if typing == ScalarTyping::Implicit && value.is_empty() {
                    "null"
                } else {
                    value
                };
                self.output.push_str(fixed);
                self.last_char_newline = false;
            }
            ScalarStyle::SingleQuoted => {
                write_single_quoted(&mut self.output, value);
                self.last_char_newline = false;
            }
            ScalarStyle::DoubleQuoted => {
                write_double_quoted(&mut self.output, value);
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
        let content_col = self.column().0 + self.indent.get();

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
    fn flush_pending_start(&mut self) -> EmitResult<()> {
        if let Some(PendingStart {
            kind,
            anchor_id,
            tag,
        }) = self.pending_start.take()
        {
            self.start_collection(kind, anchor_id, tag.as_ref())?;
        }
        Ok(())
    }

    /// Writes a collection start known to be non-empty and opens its context and column.
    fn start_collection(
        &mut self,
        kind: CollectionKind,
        anchor_id: usize,
        tag: Option<&Tag>,
    ) -> EmitResult<()> {
        self.begin_explicit_value();
        let ctx = self.current_context();

        // Handle pending newline
        self.flush_pending_newline();

        // Write prefix and properties inline per context to avoid them landing on the wrong line.
        match ctx {
            Context::Sequence => {
                self.write_dash_prefix();
                if self.emit_properties(anchor_id, tag)? {
                    // Properties occupy the "- " line, so children need fresh indentation.
                    self.output.push('\n');
                    self.last_char_newline = true;
                } else {
                    self.cursor = Cursor::AfterDash;
                }
            }
            Context::MappingKey => {
                // Collection as mapping key: explicit `?` entry, children on following lines.
                self.write_key_indent();
                self.output.push('?');
                self.emit_properties_after_space(anchor_id, tag)?;
                self.output.push('\n');
                self.last_char_newline = true;
            }
            Context::MappingValue => {
                // The pending_space after colon is dropped: "key: &anchor\n" or "key:\n".
                self.pending_space = false;
                self.emit_properties_after_space(anchor_id, tag)?;
                self.output.push('\n');
                self.last_char_newline = true;
            }
            // Explicit* are consumed by begin_explicit_value and never current here.
            Context::Root | Context::ExplicitKey | Context::ExplicitValue => {
                if self.emit_properties(anchor_id, tag)? {
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

        let column = self.child_column(ctx);
        self.backend.context_stack_mut().push(kind.entry_context());
        self.columns.push(column);
        Ok(())
    }

    fn end_collection(&mut self, kind: CollectionKind) -> EmitResult<()> {
        if let Some(PendingStart { anchor_id, tag, .. }) =
            self.pending_start.take_if(|pending| pending.kind == kind)
        {
            let ctx = self.begin_inline_node(NodeShape::Inline);
            if self.emit_properties(anchor_id, tag.as_ref())? {
                self.output.push(' ');
            }
            self.output.push_str(kind.empty_flow());
            self.last_char_newline = false;
            self.end_inline_node(ctx, NodeShape::Inline);
            return Ok(());
        }

        debug_assert_eq!(
            self.cursor,
            Cursor::Normal,
            "entry cursor left set at collection end"
        );
        self.cursor = Cursor::Normal;
        self.backend.context_stack_mut().pop();
        self.columns.pop();
        if self.current_context() == Context::ExplicitKey {
            self.set_context(Context::ExplicitValue);
        }
        Ok(())
    }

    fn emit_alias(&mut self, anchor_id: usize) {
        let ctx = self.begin_inline_node(NodeShape::Inline);
        let node_start = self.output.len();

        self.emit_alias_node(anchor_id);
        // The space counts toward the 1024 limit: the scanner rejects `:` past column 1024.
        // Anchor names may contain ':', so `*a:` would read as alias "a:".
        if ctx == Context::MappingKey {
            self.output.push(' ');
        }

        if self.is_too_long_key(ctx, NodeShape::Inline, node_start) {
            self.begin_explicit_key(node_start);
            self.emit_alias_node(anchor_id);
            self.end_explicit_key(ctx);
            return;
        }

        self.end_inline_node(ctx, NodeShape::Inline);
    }

    /// Writes the alias reference `*name`.
    fn emit_alias_node(&mut self, anchor_id: usize) {
        self.output.push('*');
        match self.backend.anchor_store().get(anchor_id) {
            Some(name) => self.output.push_str(name),
            None => {
                let _ = write!(self.output, "anchor{anchor_id}");
            }
        }
        self.last_char_newline = false;
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

    /// Completes formatting and returns the output.
    pub(super) fn finish(mut self) -> String {
        // Ensure output ends with newline
        if !self.output.is_empty() && !self.last_char_newline {
            self.output.push('\n');
        }
        self.output
    }
}

#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "tests drive the raw parser as the reference"
)]
mod tests {
    use saphyr_parser::{Event, Parser};

    use super::misdecoded_escape;
    use crate::error::ParseError;
    use crate::limits::{Indent, LimitKind, MaxDepth, MaxTagBytes};
    use crate::streaming::{MAX_ANCHOR_ID, format_streaming};
    use crate::{EmitError, EmitterConfig};

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
    fn saphyr_misdecodes_two_byte_tag_escapes() {
        // Canary for the workaround in `misdecoded_escape`: when saphyr-parser starts decoding
        // `%D1%82` as `т` (U+0442) instead of U+D182, remove the workaround.
        for (yaml, suffix) in [
            ("!a%D1%82 x\n", "a\u{D182}"),
            ("!a%C3%A9 x\n", "a\u{C3A9}"),
            ("!a%C2%80 x\n", "a\u{C280}"),
        ] {
            let tag = Parser::new_from_str(yaml)
                .find_map(|event| match event.unwrap().0 {
                    Event::Scalar(_, _, _, Some(tag)) => Some(tag.into_owned()),
                    _ => None,
                })
                .unwrap();
            assert_eq!(tag.suffix, suffix, "{yaml:?}");
        }
        for yaml in [
            "!a%E2%82%AC x\n",
            "!a%D8%80 x\n",
            "!\u{442}\u{435}\u{433} x\n",
        ] {
            assert!(
                Parser::new_from_str(yaml).any(|event| event.is_err()),
                "{yaml:?}"
            );
        }
    }

    #[test]
    fn two_byte_tag_escapes_round_trip() {
        for (yaml, expected) in [
            ("a: !a%D1%82 x\n", "a: !a%D1%82 x\n"),
            ("a: !a%C3%A9%20b x\n", "a: !a%C3%A9%20b x\n"),
            ("a: !<tag:x%D1%82,2:y> z\n", "a: !<tag:x%D1%82,2:y> z\n"),
            (
                "%TAG !e! tag:x%D1%82,2000:\n---\na: !e!y z\n",
                "%TAG !e! tag:x%D1%82,2000:\n---\na: !<tag:x%D1%82,2000:y> z\n",
            ),
        ] {
            let out = assert_stable(yaml);
            assert_eq!(out, expected, "{yaml:?}");
        }
    }

    #[test]
    fn only_a_two_byte_misreading_is_unescaped() {
        assert_eq!(misdecoded_escape('\u{D182}'), Some([0xD1, 0x82]));
        assert_eq!(misdecoded_escape('\u{C280}'), Some([0xC2, 0x80]));
        assert_eq!(misdecoded_escape('\u{D7BF}'), Some([0xD7, 0xBF]));
        for c in [
            'a',
            '\u{E9}',
            '\u{442}',
            '\u{C27F}',
            '\u{C2C0}',
            '\u{C180}',
            '\u{1F600}',
        ] {
            assert_eq!(misdecoded_escape(c), None, "{c:?}");
        }
    }

    #[test]
    fn anchor_names_survive_non_ascii_directive_names() {
        let out = fmt("%ÄÖÜ x\n---\na: &abc 1\nb: *abc\n");
        assert!(out.contains("&abc") && out.contains("*abc"), "{out}");
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
        let config = EmitterConfig::new().with_indent(Indent::new(4).unwrap());
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
        let config = EmitterConfig::new().with_indent(Indent::new(4).unwrap());
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
        let config = EmitterConfig::new().with_indent(Indent::new(4).unwrap());
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
        let config = EmitterConfig::new().with_indent(Indent::new(4).unwrap());
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

    fn nested_maps(depth: usize) -> String {
        let mut yaml = String::new();
        for level in 0..depth {
            yaml.push_str(&"  ".repeat(level));
            yaml.push_str("a:\n");
        }
        yaml.push_str(&"  ".repeat(depth));
        yaml.push_str("v\n");
        yaml
    }

    fn nested_mixed(depth: usize) -> String {
        let mut yaml = String::new();
        for level in 0..depth {
            yaml.push_str(&"  ".repeat(level));
            yaml.push_str(if level % 2 == 0 { "-\n" } else { "a:\n" });
        }
        yaml.push_str(&"  ".repeat(depth));
        yaml.push_str("v\n");
        yaml
    }

    fn nested_seqs(depth: usize) -> String {
        format!("{}v\n", "- ".repeat(depth))
    }

    fn format_all_backends(yaml: &str) -> Vec<Result<String, EmitError>> {
        let config = EmitterConfig::default();
        vec![
            format_streaming(yaml, &config),
            #[cfg(feature = "arena")]
            crate::streaming::format_streaming_arena(yaml, &config),
        ]
    }

    fn assert_depth_error(yaml: &str) {
        for result in format_all_backends(yaml) {
            assert!(
                matches!(
                    result,
                    Err(EmitError::Parse(ParseError::LimitExceeded {
                        kind: LimitKind::Depth(limit),
                        ..
                    })) if limit == MaxDepth::DEFAULT
                ),
                "{result:?}"
            );
        }
    }

    fn assert_anchor_error(yaml: &str) {
        for result in format_all_backends(yaml) {
            assert!(
                matches!(result, Err(EmitError::AnchorLimitExceeded { limit }) if limit == MAX_ANCHOR_ID)
            );
        }
    }

    #[test]
    fn depth_256_is_formatted_and_257_is_rejected() {
        for gen_yaml in [nested_maps, nested_seqs, nested_mixed] {
            let ok = gen_yaml(256);
            for result in format_all_backends(&ok) {
                let out = result.unwrap();
                assert_eq!(events(&out), events(&ok));
            }
            assert_stable(&ok);
            assert_depth_error(&gen_yaml(257));
        }
    }

    fn anchored_items(count: usize) -> String {
        use std::fmt::Write as _;
        let mut yaml = String::new();
        for i in 1..=count {
            writeln!(yaml, "- &a{i} v{i}").unwrap();
        }
        writeln!(yaml, "- *a{count}").unwrap();
        yaml
    }

    #[test]
    fn anchor_limit_is_enforced() {
        assert_stable(&anchored_items(4096));
        assert_anchor_error(&anchored_items(4097));
    }

    #[test]
    fn anchor_limit_counts_collections() {
        let filler = anchored_items(4095);
        for last in ["- &b [1]\n", "- &b {k: v}\n"] {
            let yaml = format!("{filler}{last}");
            assert!(format_streaming(&yaml, &EmitterConfig::default()).is_ok());
        }
        for last in ["- &b [1]\n- &c [2]\n", "- &b {k: v}\n- &c {k: v}\n"] {
            let yaml = format!("{filler}{last}");
            assert_anchor_error(&yaml);
        }
    }

    #[test]
    fn anchor_limit_counts_keys() {
        use std::fmt::Write as _;
        let mut yaml = String::new();
        for i in 1..=4096 {
            writeln!(yaml, "&a{i} k{i}: v").unwrap();
        }
        assert!(format_streaming(&yaml, &EmitterConfig::default()).is_ok());
        yaml.push_str("&b kb: v\n");
        assert_anchor_error(&yaml);
    }

    #[test]
    fn anchor_limit_is_per_document() {
        let doc = |n: usize| format!("---\n{}", anchored_items(n));
        let ok = format!("{}{}{}", doc(3000), doc(3000), doc(3000));
        for result in format_all_backends(&ok) {
            let out = result.unwrap();
            assert_eq!(events(&out), events(&ok));
        }
        let too_many = format!("{}{}", doc(10), doc(4097));
        assert_anchor_error(&too_many);
    }

    fn key_of(len: usize, filler: char) -> String {
        std::iter::repeat_n(filler, len).collect()
    }

    #[test]
    fn keys_up_to_1024_chars_stay_implicit() {
        for filler in ['k', 'é'] {
            let yaml = format!("{}: v\n", key_of(1024, filler));
            assert_eq!(assert_stable(&yaml), yaml);
        }
        let anchored = format!("&x {}: v\n", key_of(1020, 'k'));
        assert_eq!(assert_stable(&anchored), anchored);
        let tagged = format!("!!str {}: v\n", key_of(1018, 'k'));
        assert_eq!(assert_stable(&tagged), tagged);
        let anchor_tagged = format!("&x !!str {}: v\n", key_of(1014, 'k'));
        assert_eq!(assert_stable(&anchor_tagged), anchor_tagged);
        let quoted = format!("\"{}\": v\n", key_of(1022, 'k'));
        assert_eq!(assert_stable(&quoted), quoted);
    }

    #[test]
    fn keys_over_1024_chars_use_explicit_form() {
        let long = key_of(1025, 'k');
        for yaml in [
            format!("? {long}\n: v\n"),
            format!("? '{long}'\n: v\n"),
            format!("? \"{long}\"\n: v\n"),
            format!("? {}\n: v\n", key_of(1025, 'é')),
            format!("? &x {}\n: v\n", key_of(1023, 'k')),
            format!("? \"{}\"\n: v\n", key_of(1023, 'k')),
            format!("- ? {long}\n  : v\n  o: 1\n"),
            format!("a:\n  b:\n    ? {long}\n    : v\n    c: 2\n"),
            format!("{{{long}: v, w: 1}}\n"),
            format!("? !!str {}\n: v\n", key_of(1020, 'k')),
            format!("? !custom {}\n: v\n", key_of(1020, 'k')),
            format!("? &x !!str {}\n: v\n", key_of(1016, 'k')),
        ] {
            let out = assert_stable(&yaml);
            assert!(out.contains("? "), "no explicit key for {yaml:.40?}");
        }
        assert_eq!(fmt(&format!("? {long}\n: v\n")), format!("? {long}\n: v\n"));
    }

    #[test]
    fn alias_keys_follow_implicit_key_limit() {
        for len in [1022, 1023, 1024, 1100] {
            let name = key_of(len, 'a');
            let key = if len >= 1023 {
                format!("? *{name}\n: v\n")
            } else {
                format!("*{name} : v\n")
            };
            let yaml = format!("a: &{name} x\n{key}o: 1\n");
            let out = assert_stable(&yaml);
            assert_eq!(out.contains("? *"), len >= 1023, "len {len}");
        }
    }

    #[test]
    fn multiline_scalar_values_round_trip() {
        for yaml in [
            "a: x\n  y\n\n  z\n",
            "a: 'x\n\n  y'\n",
            "- x\n  y\n\n  z\n",
            "- 'p\n\n  q'\n",
            "x\n  y\n\n  z\n",
            "[x\n  y\n\n  z, 'p\n\n  q']\n",
        ] {
            assert_stable(yaml);
        }
    }

    fn amplifying_input() -> String {
        let mut doc = format!("%TAG !e! tag:e.com,{}\n---\n", "a".repeat(100_000));
        doc.extend((0..1_000).map(|i| format!("k{i}: !e!x v\n")));
        doc
    }

    #[test]
    fn tag_prefix_amplification_is_rejected() {
        for result in format_all_backends(&amplifying_input()) {
            assert!(matches!(
                result,
                Err(EmitError::TagLimitExceeded { limit }) if limit == MaxTagBytes::DEFAULT
            ));
        }
    }

    #[test]
    fn cross_document_alias_is_rejected() {
        for result in format_all_backends("--- &a [x]\n--- *a\n") {
            let err = result.unwrap_err();
            assert!(err.to_string().contains("unknown anchor"), "{err}");
        }
    }

    #[test]
    fn tag_prefix_starting_with_hash_survives_formatting() {
        let out = assert_stable("%TAG !e! #x\n---\na: !e!y 1\n");
        assert!(out.starts_with("%TAG !e! #x\n"), "{out:?}");
    }

    #[test]
    fn ordinary_tag_directive_still_formats() {
        let out = fmt("%TAG !e! tag:e.com,2000:\n---\nk: !e!x v\n");
        assert!(out.contains("tag:e.com,2000:x"), "{out}");
    }
}
