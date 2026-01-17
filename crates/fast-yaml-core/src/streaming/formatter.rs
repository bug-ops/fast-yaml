//! Generic streaming formatter with pluggable backend.
//!
//! This module contains the core formatting logic abstracted over
//! different memory allocation strategies via the `FormatterBackend` trait.

use std::borrow::Cow;
use std::fmt::Write;

use saphyr_parser::{Event, ScalarStyle, Span, Tag};

use super::traits::{AnchorStoreOps, ContextStackOps, FormatterBackend};
use super::{Context, INDENT_SPACES, MAX_ANCHOR_ID, MAX_DEPTH};
use crate::emitter::EmitterConfig;

/// Generic streaming formatter with pluggable backend.
///
/// This struct contains ALL formatting logic and is parameterized over
/// the backend type `B: FormatterBackend`. Through monomorphization,
/// this compiles to specialized code for each backend with zero runtime cost.
pub struct StreamingFormatter<'a, B: FormatterBackend> {
    config: &'a EmitterConfig,
    output: String,
    indent_level: usize,
    /// Tracks whether we need to emit a newline before the next value
    pending_newline: bool,
    /// Tracks whether the last character written was a newline.
    /// Avoids O(n) `ends_with` scans by maintaining state.
    last_char_newline: bool,
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
            output: String::with_capacity(output_capacity),
            indent_level: 0,
            pending_newline: false,
            last_char_newline: true, // Empty buffer conceptually "ends with" newline
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

    /// Emits an anchor marker (&anchorN) if `anchor_id` is valid.
    ///
    /// # Arguments
    ///
    /// * `anchor_id` - The anchor ID to emit (must be in range `1..=MAX_ANCHOR_ID`)
    /// * `emit_newline` - If true, emits newline after anchor; if false, emits space
    ///
    /// # Returns
    ///
    /// Returns true if an anchor was emitted, false otherwise.
    fn emit_anchor_if_present(&mut self, anchor_id: usize, emit_newline: bool) -> bool {
        if anchor_id > 0 && anchor_id <= MAX_ANCHOR_ID {
            self.backend.anchor_store_mut().ensure_capacity(anchor_id);
            let name = self.backend.anchor_store_mut().set_if_empty(anchor_id);
            self.output.push('&');
            self.output.push_str(name);
            if emit_newline {
                self.output.push('\n');
                self.last_char_newline = true;
            } else {
                self.output.push(' ');
                self.last_char_newline = false;
            }
            true
        } else {
            false
        }
    }

    /// Processes a parser event and updates formatter state.
    pub fn format_event(&mut self, event: Event<'_>, _span: Span) {
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
                self.emit_scalar(&value, style, anchor_id, tag.as_ref());
            }

            Event::SequenceStart(anchor_id, tag) => {
                self.start_sequence(anchor_id, tag.as_ref());
            }

            Event::SequenceEnd => {
                self.end_sequence();
            }

            Event::MappingStart(anchor_id, tag) => {
                self.start_mapping(anchor_id, tag.as_ref());
            }

            Event::MappingEnd => {
                self.end_mapping();
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
        _tag: Option<&Cow<'_, Tag>>,
    ) {
        let ctx = self.current_context();

        // Handle pending newline from document start or collection start
        if self.pending_newline {
            self.output.push('\n');
            self.pending_newline = false;
            self.last_char_newline = true;
        }

        // Write indentation and prefix based on context
        match ctx {
            Context::Sequence => {
                self.write_indent();
                self.output.push_str("- ");
                self.last_char_newline = false;
            }
            Context::MappingKey => {
                self.write_indent();
            }
            // Root level scalar and mapping value need no prefix
            Context::Root | Context::MappingValue => {}
        }

        // Handle anchor if present (with bounds check for security)
        self.emit_anchor_if_present(anchor_id, false);

        // Emit value with appropriate style
        self.emit_value_with_style(value, style);

        // Handle context transitions
        match ctx {
            Context::MappingKey => {
                self.output.push(':');
                // Transition to expecting value
                if let Some(last) = self.backend.context_stack_mut().last_mut() {
                    *last = Context::MappingValue;
                }
                // Add space after colon for simple values
                self.output.push(' ');
                self.last_char_newline = false;
            }
            Context::MappingValue => {
                self.output.push('\n');
                self.last_char_newline = true;
                // Transition back to expecting key
                if let Some(last) = self.backend.context_stack_mut().last_mut() {
                    *last = Context::MappingKey;
                }
            }
            Context::Sequence | Context::Root => {
                self.output.push('\n');
                self.last_char_newline = true;
            }
        }
    }

    fn emit_value_with_style(&mut self, value: &str, style: ScalarStyle) {
        match style {
            ScalarStyle::Plain => {
                // Fix special floats for YAML 1.2 compliance
                let fixed = super::fix_special_float_value(value);
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
                        _ => self.output.push(c),
                    }
                }
                self.output.push('"');
                self.last_char_newline = false;
            }
            ScalarStyle::Literal => {
                self.output.push_str("|-");
                self.output.push('\n');
                self.write_block_scalar_lines(value);
                // write_block_scalar_lines always ends with newline
                self.last_char_newline = true;
            }
            ScalarStyle::Folded => {
                self.output.push_str(">-");
                self.output.push('\n');
                self.write_block_scalar_lines(value);
                // write_block_scalar_lines always ends with newline
                self.last_char_newline = true;
            }
        }
    }

    fn start_sequence(&mut self, anchor_id: usize, _tag: Option<&Cow<'_, Tag>>) {
        let ctx = self.current_context();

        // Handle pending newline
        if self.pending_newline {
            self.output.push('\n');
            self.pending_newline = false;
            self.last_char_newline = true;
        }

        // Write prefix based on context
        match ctx {
            Context::Sequence => {
                self.write_indent();
                self.output.push_str("- ");
                self.last_char_newline = false;
            }
            Context::MappingKey => {
                // Sequence as mapping key - unusual but valid
                self.write_indent();
            }
            Context::MappingValue => {
                // Value position - newline and indent for nested sequence
                self.output.push('\n');
                self.last_char_newline = true;
            }
            Context::Root => {}
        }

        // Handle anchor (with bounds check for security)
        self.emit_anchor_if_present(anchor_id, false);

        // Update context for mapping value -> key transition
        if ctx == Context::MappingValue
            && let Some(last) = self.backend.context_stack_mut().last_mut()
        {
            *last = Context::MappingKey;
        }

        // Push sequence context and increase indent (with depth limit)
        if self.backend.context_stack().len() < MAX_DEPTH {
            self.backend.context_stack_mut().push(Context::Sequence);
            self.indent_level += 1;
        }
    }

    fn end_sequence(&mut self) {
        self.backend.context_stack_mut().pop();
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    fn start_mapping(&mut self, anchor_id: usize, _tag: Option<&Cow<'_, Tag>>) {
        let ctx = self.current_context();

        // Handle pending newline
        if self.pending_newline {
            self.output.push('\n');
            self.pending_newline = false;
            self.last_char_newline = true;
        }

        // Write prefix based on context
        match ctx {
            Context::Sequence => {
                self.write_indent();
                self.output.push_str("- ");
                self.last_char_newline = false;
            }
            Context::MappingKey => {
                // Mapping as mapping key - unusual but valid (complex key)
                self.write_indent();
            }
            Context::MappingValue => {
                // Value position - newline for nested mapping
                self.output.push('\n');
                self.last_char_newline = true;
            }
            Context::Root => {}
        }

        // Handle anchor (with bounds check for security)
        self.emit_anchor_if_present(anchor_id, true);

        // Update context for mapping value -> key transition
        if ctx == Context::MappingValue
            && let Some(last) = self.backend.context_stack_mut().last_mut()
        {
            *last = Context::MappingKey;
        }

        // Push mapping context and increase indent (with depth limit)
        if self.backend.context_stack().len() < MAX_DEPTH {
            self.backend.context_stack_mut().push(Context::MappingKey);
            self.indent_level += 1;
        }
    }

    fn end_mapping(&mut self) {
        self.backend.context_stack_mut().pop();
        self.indent_level = self.indent_level.saturating_sub(1);
    }

    fn emit_alias(&mut self, anchor_id: usize) {
        let ctx = self.current_context();

        // Handle pending newline
        if self.pending_newline {
            self.output.push('\n');
            self.pending_newline = false;
            self.last_char_newline = true;
        }

        // Write prefix based on context
        match ctx {
            Context::Sequence => {
                self.write_indent();
                self.output.push_str("- ");
                self.last_char_newline = false;
            }
            Context::MappingKey => {
                self.write_indent();
            }
            Context::Root | Context::MappingValue => {}
        }

        // Emit the alias reference
        self.output.push('*');
        if let Some(name) = self.backend.anchor_store().get(anchor_id) {
            self.output.push_str(name);
        } else {
            // Fallback: generate name directly into output
            let _ = write!(self.output, "anchor{anchor_id}");
        }
        self.last_char_newline = false;

        // Handle context transitions
        match ctx {
            Context::MappingKey => {
                self.output.push(':');
                if let Some(last) = self.backend.context_stack_mut().last_mut() {
                    *last = Context::MappingValue;
                }
                self.output.push(' ');
                // last_char_newline remains false
            }
            Context::MappingValue => {
                self.output.push('\n');
                self.last_char_newline = true;
                if let Some(last) = self.backend.context_stack_mut().last_mut() {
                    *last = Context::MappingKey;
                }
            }
            Context::Sequence | Context::Root => {
                self.output.push('\n');
                self.last_char_newline = true;
            }
        }
    }

    /// Write indentation for block scalar content (literal/folded styles).
    fn write_block_scalar_lines(&mut self, value: &str) {
        let indent_chars = self.indent_level.saturating_mul(self.config.indent);

        for line in value.lines() {
            if indent_chars <= INDENT_SPACES.len() {
                self.output.push_str(&INDENT_SPACES[..indent_chars]);
            } else {
                self.output.push_str(&" ".repeat(indent_chars));
            }
            self.output.push_str(line);
            self.output.push('\n');
        }
    }

    fn write_indent(&mut self) {
        if self.indent_level > 1 {
            let indent_chars = (self.indent_level - 1).saturating_mul(self.config.indent);

            if indent_chars <= INDENT_SPACES.len() {
                self.output.push_str(&INDENT_SPACES[..indent_chars]);
            } else {
                self.output.push_str(&" ".repeat(indent_chars));
            }
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
