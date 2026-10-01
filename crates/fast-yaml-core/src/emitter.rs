use std::borrow::Cow;
use std::fmt::Write as _;

use crate::error::{EmitError, EmitResult, from_saphyr};
use crate::input::NormalizedInput;
use crate::limits::{Indent, MaxDepth, Width};
use crate::scalar::{ResolvedScalar, is_c_printable, resolve_scalar};
use crate::streaming::{is_unsafe_plain, write_double_quoted};
use crate::value::Value;
use saphyr::{Scalar, Yaml, YamlEmitter};
use saphyr_parser::{ScalarStyle, Tag};

/// Configuration for YAML emission.
///
/// Controls formatting, style, and output options when serializing YAML.
#[derive(Debug, Clone)]
pub struct EmitterConfig {
    /// Indentation width in spaces (default: 2).
    ///
    /// Controls the number of spaces used for each indentation level.
    ///
    /// Note: saphyr currently uses fixed 2-space indentation.
    /// This parameter is accepted for `PyYAML` API compatibility but
    /// may require post-processing to fully support custom values.
    pub indent: Indent,

    /// Maximum line width for wrapping (default: 80).
    ///
    /// When lines exceed this width, the emitter will attempt to wrap them.
    ///
    /// Note: saphyr has limited control over line wrapping.
    /// This parameter is accepted for `PyYAML` API compatibility.
    pub width: Width,

    /// Default flow style for collections (default: None).
    ///
    /// - `None`: Use block style (multi-line)
    /// - `Some(true)`: Force flow style (inline: `[...]`, `{...}`)
    /// - `Some(false)`: Force block style (explicit)
    pub default_flow_style: Option<bool>,

    /// Add explicit document start marker `---` (default: false).
    ///
    /// When true, prepends `---\n` to the output.
    pub explicit_start: bool,

    /// Enable compact inline notation (default: true).
    ///
    /// Controls whether saphyr uses compact notation for
    /// inline sequences and mappings.
    pub compact: bool,

    /// Render multiline strings in literal style (default: false).
    ///
    /// When true, strings containing newlines will be rendered
    /// using literal block scalar notation (`|`).
    pub multiline_strings: bool,

    /// Maximum nesting depth of collections (default: [`MaxDepth::DEFAULT`]).
    ///
    /// [`Emitter::emit_str_with_config`] fails with [`EmitError::DepthLimitExceeded`] beyond it,
    /// and [`Emitter::format_with_config`] rejects input nested deeper with a
    /// [`ParseError::LimitExceeded`](crate::ParseError::LimitExceeded). Parsing accepts up to
    /// [`MaxDepth::MAX`], so raise this to emit or format data parsed with a higher limit.
    pub max_depth: MaxDepth,
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            indent: Indent::DEFAULT,
            width: Width::DEFAULT,
            default_flow_style: None,
            explicit_start: false,
            compact: true,
            multiline_strings: false,
            max_depth: MaxDepth::DEFAULT,
        }
    }
}

impl EmitterConfig {
    /// Create a new emitter configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set indentation width.
    #[must_use]
    pub const fn with_indent(mut self, indent: Indent) -> Self {
        self.indent = indent;
        self
    }

    /// Set line width.
    #[must_use]
    pub const fn with_width(mut self, width: Width) -> Self {
        self.width = width;
        self
    }

    /// Set the maximum nesting depth.
    ///
    /// Block emission recurses inside saphyr and needs about 2 MiB of stack at depth 512
    /// (release); see [`MaxDepth`] for the smaller-stack guidance.
    #[must_use]
    pub const fn with_max_depth(mut self, max_depth: MaxDepth) -> Self {
        self.max_depth = max_depth;
        self
    }

    /// Set default flow style for collections.
    #[must_use]
    pub const fn with_default_flow_style(mut self, flow_style: Option<bool>) -> Self {
        self.default_flow_style = flow_style;
        self
    }

    /// Set explicit document start marker.
    #[must_use]
    pub const fn with_explicit_start(mut self, explicit_start: bool) -> Self {
        self.explicit_start = explicit_start;
        self
    }

    /// Set compact inline notation.
    #[must_use]
    pub const fn with_compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Set multiline string rendering.
    #[must_use]
    pub const fn with_multiline_strings(mut self, multiline_strings: bool) -> Self {
        self.multiline_strings = multiline_strings;
        self
    }
}

/// Emitter for YAML documents.
///
/// Serializes a [`Value`] through saphyr's block emitter, or through a dedicated flow emitter
/// when [`EmitterConfig::default_flow_style`] is `Some(true)`. Both honor the remembered spelling
/// of a [`Float`](crate::Float) and quote strings that would otherwise read back as another type.
///
/// Emission refuses nesting deeper than [`EmitterConfig::max_depth`] (256 by default) with
/// [`EmitError::DepthLimitExceeded`], while parsing accepts up to [`MaxDepth::MAX`] (512), so a
/// document parsed with raised limits needs a matching `max_depth` to emit.
#[derive(Debug)]
pub struct Emitter;

impl Emitter {
    /// Emit a single YAML document to a string with configuration.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Format` if the value cannot be serialized,
    /// `EmitError::DepthLimitExceeded` if it nests deeper than 256 levels, and
    /// `EmitError::ComplexFlowKey` if flow style is requested for a sequence or mapping key.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, EmitterConfig, Value};
    ///
    /// let value = Value::String("test".to_string());
    /// let config = EmitterConfig::new().with_explicit_start(true);
    /// let yaml = Emitter::emit_str_with_config(&value, &config)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn emit_str_with_config(value: &Value, config: &EmitterConfig) -> EmitResult<String> {
        // When flow style is requested, use the custom path that renders {k: v} / [a, b].
        if config.default_flow_style == Some(true) {
            let mut raw = String::new();
            write_flow(&mut raw, value, 0, config.max_depth)?;
            let mut output = Self::apply_formatting(raw, config);
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            return Ok(output);
        }

        let estimated_size = Self::estimate_value_size(value);
        let mut output = String::with_capacity(estimated_size);
        {
            let mut emitter = YamlEmitter::new(&mut output);

            // Apply saphyr native configuration
            emitter.compact(config.compact);
            emitter.multiline_strings(config.multiline_strings);

            let yaml = to_saphyr(value, 0, config.max_depth)?;
            emitter.dump(&yaml).map_err(from_saphyr)?;
        }

        // Apply post-processing for configuration options
        output = Self::apply_formatting(output, config);
        if config.indent != Indent::DEFAULT {
            output = Self::at_indent(&output, config)?;
        }

        // Ensure output always ends with a newline
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }

        Ok(output)
    }

    fn estimate_value_size(value: &Value) -> usize {
        match value {
            Value::Null => 4,    // "null"
            Value::Bool(_) => 5, // "false"
            Value::Int(i) => {
                // Decimal digits + sign (max 20 for i64)
                if *i == 0 {
                    1
                } else {
                    // Use checked_ilog10 for precise digit count without float conversion
                    i.unsigned_abs()
                        .checked_ilog10()
                        .map_or(1, |d| d as usize + 1)
                        + 1
                }
            }
            Value::BigInt(big) => big.canonical().len(),
            Value::Float(_) => 20,           // Conservative estimate
            Value::String(s) => s.len() + 2, // Possible quotes
            Value::Sequence(seq) => {
                // "- " prefix (2) + newline (1) per item + recursive content
                seq.iter().map(|v| 3 + Self::estimate_value_size(v)).sum()
            }
            Value::Set(set) => set.iter().map(|v| 11 + Self::estimate_value_size(v)).sum(),
            Value::Mapping(map) => {
                // "key: " (~10) + newline (1) + recursive content
                map.iter()
                    .map(|(k, v)| 11 + Self::estimate_value_size(k) + Self::estimate_value_size(v))
                    .sum()
            }
        }
    }

    /// Emit a single YAML document to a string with default configuration.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Format` if the value cannot be serialized, and
    /// `EmitError::DepthLimitExceeded` if it nests deeper than 256 levels.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, Value};
    ///
    /// let value = Value::String("test".to_string());
    /// let yaml = Emitter::emit_str(&value)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn emit_str(value: &Value) -> EmitResult<String> {
        Self::emit_str_with_config(value, &EmitterConfig::default())
    }

    /// Emit multiple YAML documents to a string with document separators and configuration.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Format` if any value cannot be serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, EmitterConfig, Value};
    ///
    /// let docs = vec![
    ///     Value::String("first".to_string()),
    ///     Value::String("second".to_string()),
    /// ];
    /// let config = EmitterConfig::new().with_explicit_start(true);
    /// let yaml = Emitter::emit_all_with_config(&docs, &config)?;
    /// assert!(yaml.contains("---"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn emit_all_with_config(values: &[Value], config: &EmitterConfig) -> EmitResult<String> {
        // Pre-calculate total estimated size for all documents
        let total_size: usize =
            values.iter().map(Self::estimate_value_size).sum::<usize>() + values.len() * 5; // Account for "---\n" separators

        let mut output = String::with_capacity(total_size);

        // Create single config variant for non-first documents (avoids cloning per document)
        let inner_config = EmitterConfig {
            explicit_start: false,
            ..*config
        };

        for (i, value) in values.iter().enumerate() {
            // Add document separator before each document (except first if explicit_start is false)
            if i > 0 || config.explicit_start {
                output.push_str("---\n");
            }

            // Always use inner_config (with explicit_start=false) since we handle
            // document separators explicitly above
            let doc = Self::emit_str_with_config(value, &inner_config)?;
            output.push_str(&doc);

            // Ensure document ends with newline for proper separation
            if !output.ends_with('\n') {
                output.push('\n');
            }
        }

        Ok(output)
    }

    /// Emit multiple YAML documents to a string with document separators.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Format` if any value cannot be serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, Value};
    ///
    /// let docs = vec![
    ///     Value::String("first".to_string()),
    ///     Value::String("second".to_string()),
    /// ];
    /// let yaml = Emitter::emit_all(&docs)?;
    /// assert!(yaml.contains("---"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn emit_all(values: &[Value]) -> EmitResult<String> {
        Self::emit_all_with_config(values, &EmitterConfig::default())
    }

    /// Apply formatting configuration to YAML output.
    ///
    /// Handles `explicit_start` and potentially other post-processing.
    fn apply_formatting(mut output: String, config: &EmitterConfig) -> String {
        // Handle explicit_start
        if config.explicit_start {
            if !output.starts_with("---") {
                output.insert_str(0, "---\n");
            }
        } else if output.starts_with("---\n") {
            output.drain(..4);
        } else if output.starts_with("---") {
            // Find where content starts after "---"
            let skip = 3 + output[3..].chars().take_while(|c| *c == '\n').count();
            output.drain(..skip);
        }

        strip_tag_trailing_space(output)
    }

    /// Rewrites saphyr's fixed 2-space block output at the requested indentation.
    ///
    /// The streaming formatter owns indentation (compact `- - x` and `? ` entries included), so
    /// the text is re-emitted by it instead of being rescaled line by line.
    fn at_indent(output: &str, config: &EmitterConfig) -> EmitResult<String> {
        crate::streaming::format_normalized(&NormalizedInput::new(output)?, config)
    }

    /// Format a YAML string with configuration.
    ///
    /// Uses the streaming formatter, which preserves scalar styles, explicit tags,
    /// anchors and aliases.
    ///
    /// Block scalar styles (`|` literal and `>` folded) are preserved in the output.
    /// `%YAML` and `%TAG` directives are preserved before the document that declared them.
    /// A byte order mark at the start of the input is kept; one in a later document prefix is
    /// dropped (see [`NormalizedInput`]).
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Parse` if the YAML cannot be parsed, `EmitError::Format` if
    /// writing fails, and
    /// `EmitError::DepthLimitExceeded` or `EmitError::AnchorLimitExceeded` if the
    /// document exceeds the formatter's nesting or per-document anchor limits.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, EmitterConfig};
    ///
    /// let yaml = "key: value\nlist:\n  - item1\n  - item2\n";
    /// let config = EmitterConfig::default();
    /// let formatted = Emitter::format_with_config(yaml, &config)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn format_with_config(input: &str, config: &EmitterConfig) -> EmitResult<String> {
        Self::format_normalized(&NormalizedInput::new(input)?, config)
    }

    /// Formats already validated input, for callers that reuse the [`NormalizedInput`] (for
    /// example to scan it for comments) instead of normalizing the text twice.
    ///
    /// A byte order mark at the start of the original text is kept, as in
    /// [`format_with_config`](Self::format_with_config).
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`format_with_config`](Self::format_with_config).
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, EmitterConfig, NormalizedInput};
    ///
    /// let input = NormalizedInput::new("\u{FEFF}a:   1\n")?;
    /// let formatted = Emitter::format_normalized(&input, &EmitterConfig::default())?;
    /// assert_eq!(formatted, "\u{FEFF}a: 1\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn format_normalized(
        input: &NormalizedInput<'_>,
        config: &EmitterConfig,
    ) -> EmitResult<String> {
        let mut formatted = crate::streaming::format_normalized(input, config)?;
        if input.original_offset(0) > 0 {
            formatted.insert(0, '\u{FEFF}');
        }
        Ok(formatted)
    }

    /// Format a YAML string with default configuration.
    ///
    /// Uses the streaming formatter, which preserves scalar styles, explicit tags,
    /// anchors and aliases.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Parse` if the YAML cannot be parsed, `EmitError::Format` if
    /// writing fails, and
    /// `EmitError::DepthLimitExceeded` or `EmitError::AnchorLimitExceeded` if the
    /// document exceeds the formatter's nesting or per-document anchor limits.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::Emitter;
    ///
    /// let yaml = "key: value\nlist:\n  - item1\n  - item2\n";
    /// let formatted = Emitter::format(yaml)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn format(input: &str) -> EmitResult<String> {
        Self::format_with_config(input, &EmitterConfig::default())
    }
}

/// A byte order mark is dropped by the loader at a document prefix, so strings holding one are escaped.
const BOM: char = '\u{FEFF}';

/// Enters one more collection level, mapping the limit failure to an emit error.
fn descend(max: MaxDepth, depth: usize) -> EmitResult<usize> {
    max.descend(depth)
        .map_err(|_| EmitError::DepthLimitExceeded { limit: max.get() })
}

/// Whether a string key must be quoted to be read back as the same string in flow context.
fn flow_key_needs_quotes(s: &str) -> bool {
    !flow_key_is_plain_safe(s) || reads_as_non_string(s)
}

/// Returns `s` as a double-quoted scalar.
fn double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    write_double_quoted(&mut out, s);
    out
}

/// Whether `s` is structurally safe as a plain flow key, ignoring how it resolves.
///
/// `<<` is rejected because a plain `<<` key is read back as a merge key.
fn flow_key_is_plain_safe(s: &str) -> bool {
    !(s == "<<"
        || s.is_empty()
        || s.starts_with(char::is_whitespace)
        || s.ends_with(char::is_whitespace)
        || s.contains([':', '#', ',', '[', ']', '{', '}', '"', '\''])
        || s.contains(BOM)
        || !s.chars().all(is_c_printable)
        || s.chars().any(char::is_control)
        || is_unsafe_plain(s))
}

/// Whether the plain scalar `s` would be resolved to something other than a string.
fn reads_as_non_string(s: &str) -> bool {
    !matches!(
        resolve_scalar(s, crate::events::ScalarStyle::Plain, None),
        ResolvedScalar::Str(_)
    )
}

/// Whether a string needs a double-quoted spelling that this crate writes itself.
///
/// saphyr's own quoting check has a separate, narrower float grammar (so `+.inf` would be
/// written plain and change type on re-read) and it does not escape every non-printable. A
/// multiline string a literal block cannot represent is double-quoted.
fn string_needs_own_quoting(s: &str) -> bool {
    reads_as_non_string(s)
        || s.contains(BOM)
        || !s.chars().all(is_c_printable)
        || (s.contains(['\n', '\r']) && !literal_block_keeps(s))
}

/// Whether a literal block scalar reads back as exactly `s`: it cannot carry a `\r`, leading
/// whitespace, more than one trailing line break without indicators saphyr does not write, or a
/// line that looks like a document marker or directive at the root.
fn literal_block_keeps(s: &str) -> bool {
    !s.contains('\r')
        && !s.starts_with(char::is_whitespace)
        && !s.ends_with("\n\n")
        && !s
            .lines()
            .any(|line| line.starts_with("---") || line.starts_with("...") || line.starts_with('%'))
}

/// A plain-styled saphyr scalar whose text is written verbatim.
///
/// saphyr writes the body of a double-quoted representation without escaping it, so every
/// spelling that is not a bare scalar is built here and passed through as plain text.
const fn verbatim(text: String) -> Yaml<'static> {
    Yaml::Representation(Cow::Owned(text), ScalarStyle::Plain, None)
}

/// Fails for a set in key position, which block style writes as invalid YAML.
const fn reject_set_key(key: &Value) -> EmitResult<()> {
    match key {
        Value::Set(_) => Err(EmitError::SetAsKey),
        _ => Ok(()),
    }
}

/// Converts a mapping key; a multiline string key stays double-quoted, since a block scalar
/// cannot be a simple key.
fn key_to_saphyr(key: &Value, depth: usize, max: MaxDepth) -> EmitResult<Yaml<'_>> {
    match key {
        Value::String(s) if s.contains('\n') => Ok(verbatim(double_quoted(s))),
        _ => to_saphyr(key, depth, max),
    }
}

/// Converts `value` into a tree saphyr emits without panicking or changing its meaning.
fn to_saphyr(value: &Value, depth: usize, max: MaxDepth) -> EmitResult<Yaml<'_>> {
    Ok(match value {
        Value::Null => Yaml::Value(Scalar::Null),
        Value::Bool(b) => Yaml::Value(Scalar::Boolean(*b)),
        Value::Int(i) => Yaml::Value(Scalar::Integer(*i)),
        Value::BigInt(big) => {
            Yaml::Representation(Cow::Borrowed(big.canonical()), ScalarStyle::Plain, None)
        }
        Value::Float(f) => Yaml::Representation(
            f.spelling()
                .map_or_else(|| Cow::Owned(f.to_string()), Cow::Borrowed),
            ScalarStyle::Plain,
            None,
        ),
        Value::String(s) if string_needs_own_quoting(s) => verbatim(double_quoted(s)),
        Value::String(s) => Yaml::Value(Scalar::String(Cow::Borrowed(s))),
        Value::Sequence(items) => {
            let depth = descend(max, depth)?;
            Yaml::Sequence(
                items
                    .iter()
                    .map(|item| to_saphyr(item, depth, max))
                    .collect::<EmitResult<_>>()?,
            )
        }
        Value::Set(set) => {
            let depth = descend(max, depth)?;
            let mut out = saphyr::Mapping::with_capacity(set.len());
            for member in set {
                reject_set_key(member)?;
                out.insert(
                    key_to_saphyr(member, depth, max)?,
                    Yaml::Value(Scalar::Null),
                );
            }
            Yaml::Tagged(Cow::Owned(set_tag()), Box::new(Yaml::Mapping(out)))
        }
        Value::Mapping(map) => {
            let depth = descend(max, depth)?;
            let mut out = saphyr::Mapping::with_capacity(map.len());
            for (key, value) in map {
                reject_set_key(key)?;
                out.insert(
                    key_to_saphyr(key, depth, max)?,
                    to_saphyr(value, depth, max)?,
                );
            }
            Yaml::Mapping(out)
        }
    })
}

/// Removes the space saphyr leaves after the tag of a block collection (`k: !!set \n`).
///
/// Block scalar bodies are copied untouched, since trailing spaces there are content.
fn strip_tag_trailing_space(output: String) -> String {
    if !output.contains(" \n") {
        return output;
    }
    let mut result = String::with_capacity(output.len());
    let mut block_base: Option<usize> = None;
    for line in output.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = body.trim_end();
        let indent = body.len() - body.trim_start_matches(' ').len();
        if let Some(base) = block_base {
            if trimmed.is_empty() || indent > base {
                result.push_str(line);
                continue;
            }
            block_base = None;
        }
        let last_token = trimmed.rsplit(' ').next().unwrap_or_default();
        if last_token.starts_with(['|', '>'])
            && last_token.get(1..).is_some_and(|rest| {
                rest.chars()
                    .all(|c| c == '-' || c == '+' || c.is_ascii_digit())
            })
        {
            block_base = Some(indent);
        }
        if body.ends_with(' ') && last_token.starts_with('!') && !body.ends_with("  ") {
            result.push_str(trimmed);
            if line.ends_with('\n') {
                result.push('\n');
            }
        } else {
            result.push_str(line);
        }
    }
    result
}

/// The tag saphyr writes as `!!set`; it prints a tag as handle followed by suffix.
fn set_tag() -> Tag {
    Tag {
        handle: "!".into(),
        suffix: "!set".into(),
    }
}

/// Writes one scalar as a mapping key in flow style.
fn write_flow_key(out: &mut String, key: &Value) -> EmitResult<()> {
    match key {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => write!(out, "{i}")?,
        Value::BigInt(big) => out.push_str(big.canonical()),
        Value::Float(f) => write!(out, "{f}")?,
        Value::String(s) if flow_key_needs_quotes(s) => write_double_quoted(out, s),
        Value::String(s) => out.push_str(s),
        Value::Sequence(_) | Value::Mapping(_) | Value::Set(_) => {
            return Err(EmitError::ComplexFlowKey);
        }
    }
    Ok(())
}

/// Writes a scalar value in flow style through saphyr, without the document marker.
fn write_flow_scalar(out: &mut String, value: &Value) -> EmitResult<()> {
    let mut text = String::new();
    {
        let mut emitter = YamlEmitter::new(&mut text);
        emitter.compact(true);
        emitter
            .dump(&to_saphyr(value, 0, MaxDepth::DEFAULT)?)
            .map_err(from_saphyr)?;
    }
    // saphyr emits "---\nvalue\n"
    out.push_str(
        text.strip_prefix("---\n")
            .unwrap_or(&text)
            .trim_end_matches('\n'),
    );
    Ok(())
}

/// Writes `value` in YAML flow style: mappings as `{k: v}`, sequences as `[a, b]`.
///
/// Scalar values are rendered inline and nested collections recursively in flow style. The
/// output has no leading `---\n` marker and no trailing newline.
fn write_flow(out: &mut String, value: &Value, depth: usize, max: MaxDepth) -> EmitResult<()> {
    match value {
        Value::Mapping(map) => {
            let depth = descend(max, depth)?;
            out.push('{');
            for (i, (key, value)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_flow_key(out, key)?;
                out.push_str(": ");
                write_flow(out, value, depth, max)?;
            }
            out.push('}');
        }
        Value::Sequence(seq) => {
            let depth = descend(max, depth)?;
            out.push('[');
            for (i, item) in seq.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_flow(out, item, depth, max)?;
            }
            out.push(']');
        }
        Value::Set(set) => {
            descend(max, depth)?;
            out.push_str("!!set {");
            for (i, member) in set.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_flow_key(out, member)?;
            }
            out.push('}');
        }
        scalar => write_flow_scalar(out, scalar)?,
    }
    Ok(())
}

/// Build a block scalar header: indicator, optional indentation digit, chomp suffix.
///
/// The digit is emitted when the first non-empty line starts with a space, since
/// the parser would otherwise auto-detect the indentation from that leading space
/// and drop it from the value. `indent` is the content indent relative to
/// the parent node.
/// Chomping is `+` (keep) for trailing blank lines or a lone newline, `-` (strip)
/// without a trailing newline, and clip (nothing) otherwise.
pub(crate) fn block_scalar_header(indicator: char, value: &str, indent: Indent) -> String {
    let mut header = String::from(indicator);
    let leading_space = value
        .lines()
        .find(|line| !line.is_empty())
        .is_some_and(|line| line.starts_with(' '));
    if leading_space {
        header.push(indent.digit());
    }
    if value.ends_with("\n\n") || value == "\n" {
        header.push('+');
    } else if !value.ends_with('\n') {
        header.push('-');
    }
    header
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{BigInt, Float, Mapping, Set};

    #[test]
    fn test_emit_str_string() {
        let value = Value::String("test".to_string());
        let result = Emitter::emit_str(&value).unwrap();
        assert!(result.contains("test"));
    }

    #[test]
    fn test_emit_str_integer() {
        let value = Value::Int(42);
        let result = Emitter::emit_str(&value).unwrap();
        assert!(result.contains("42"));
    }

    #[test]
    fn test_emit_all_multiple() {
        let values = vec![
            Value::String("first".to_string()),
            Value::String("second".to_string()),
        ];
        let result = Emitter::emit_all(&values).unwrap();
        assert!(result.contains("first"));
        assert!(result.contains("second"));
        assert!(result.contains("---"));
    }

    #[test]
    fn test_emit_all_single() {
        let values = vec![Value::String("only".to_string())];
        let result = Emitter::emit_all(&values).unwrap();
        assert!(result.contains("only"));
        assert!(!result.starts_with("---"));
    }

    #[test]
    fn test_emitter_config_default() {
        let config = EmitterConfig::default();
        assert_eq!(config.indent, Indent::DEFAULT);
        assert_eq!(config.width, Width::DEFAULT);
        assert_eq!(config.max_depth, MaxDepth::DEFAULT);
        assert_eq!(config.default_flow_style, None);
        assert!(!config.explicit_start);
        assert!(config.compact);
        assert!(!config.multiline_strings);
    }

    #[test]
    fn test_emitter_config_builder() {
        let config = EmitterConfig::new()
            .with_indent(Indent::new(4).unwrap())
            .with_width(Width::new(120).unwrap())
            .with_explicit_start(true)
            .with_compact(false);

        assert_eq!(config.indent.get(), 4);
        assert_eq!(config.width.get(), 120);
        assert!(config.explicit_start);
        assert!(!config.compact);
    }

    #[test]
    fn test_indent_and_width_reject_out_of_range() {
        for value in [0, 10, 100] {
            assert!(Indent::new(value).is_err(), "{value}");
        }
        for value in [0, 19, 1001, 2000] {
            assert!(Width::new(value).is_err(), "{value}");
        }
        assert_eq!(Indent::new(9), Ok(Indent::MAX));
        assert_eq!(Width::new(20), Ok(Width::MIN));
    }

    #[test]
    fn test_max_depth_is_configurable_for_emission() {
        let mut doc = Value::Int(1);
        for _ in 0..3 {
            doc = Value::Sequence(vec![doc]);
        }
        let config = EmitterConfig::new().with_max_depth(MaxDepth::new(2).unwrap());
        for flow in [None, Some(true)] {
            let config = config.clone().with_default_flow_style(flow);
            assert!(matches!(
                Emitter::emit_str_with_config(&doc, &config),
                Err(EmitError::DepthLimitExceeded { limit: 2 })
            ));
        }
        let wide = EmitterConfig::new().with_max_depth(MaxDepth::MAX);
        assert!(Emitter::emit_str_with_config(&doc, &wide).is_ok());
    }

    #[test]
    fn test_emit_with_explicit_start() {
        let value = Value::String("test".to_string());
        let config = EmitterConfig::new().with_explicit_start(true);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(result.starts_with("---"));
    }

    #[test]
    fn test_emit_without_explicit_start() {
        let value = Value::String("test".to_string());
        let config = EmitterConfig::new().with_explicit_start(false);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(!result.starts_with("---"));
    }

    #[test]
    fn test_emit_all_with_explicit_start() {
        let values = vec![
            Value::String("first".to_string()),
            Value::String("second".to_string()),
        ];
        let config = EmitterConfig::new().with_explicit_start(true);
        let result = Emitter::emit_all_with_config(&values, &config).unwrap();
        assert!(result.starts_with("---"));
        assert_eq!(result.matches("---").count(), 2);
    }

    #[test]
    fn test_emit_with_compact_false() {
        let value = Value::Sequence(vec![Value::Int(1), Value::Int(2)]);
        let config = EmitterConfig::new().with_compact(false);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        // Should contain formatting (exact format depends on saphyr)
        assert!(result.contains('1') && result.contains('2'));
    }

    #[test]
    fn test_emit_with_multiline_strings() {
        let value = Value::String("line1\nline2".to_string());
        let config = EmitterConfig::new().with_multiline_strings(true);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        // Should use literal block scalar notation (|)
        assert!(result.contains("line1") && result.contains("line2"));
    }

    #[test]
    fn test_estimate_value_size_scalars() {
        let size = |v: Value| Emitter::estimate_value_size(&v);
        assert_eq!(size(Value::Null), 4);
        assert_eq!(size(Value::Bool(true)), 5);
        assert_eq!(size(Value::Int(0)), 1);
        assert!(size(Value::Int(5)) >= 1);
        assert!(size(Value::Int(12345)) >= 5);
        assert!(size(Value::Int(-42)) >= 2);
        assert_eq!(size(Value::Float(Float::new(1.23456))), 20);
        assert_eq!(size(Value::String("hello".to_string())), 7);
        let big = BigInt::parse("99999999999999999999").unwrap();
        assert_eq!(size(Value::BigInt(big)), 20);
    }

    #[test]
    fn test_estimate_value_size_mapping() {
        // Create a mapping with string keys and integer values
        let mut map = Mapping::new();
        map.insert(Value::String("key1".to_string()), Value::Int(100));
        map.insert(Value::String("key2".to_string()), Value::Int(200));

        let mapping = Value::Mapping(map);
        let size = Emitter::estimate_value_size(&mapping);

        // Should be > 0 and account for both key-value pairs
        // Each pair has ~11 base overhead + key size + value size
        assert!(
            size > 20,
            "Mapping estimate should be significant: got {size}"
        );

        // Test nested mapping
        let mut nested_map = Mapping::new();
        nested_map.insert(Value::String("outer".to_string()), mapping);

        let nested_size = Emitter::estimate_value_size(&Value::Mapping(nested_map));
        assert!(
            nested_size > size,
            "Nested mapping should have larger estimate"
        );
    }

    #[test]
    fn test_estimate_value_size_sequence() {
        let seq = Value::Sequence(vec![
            Value::Int(1),
            Value::Int(2),
            Value::String("hello".to_string()),
        ]);

        let size = Emitter::estimate_value_size(&seq);

        // Each item: 3 (prefix "- " + newline) + scalar size
        // Item 1: 3 + 2 (digit + overhead) = 5
        // Item 2: 3 + 2 = 5
        // Item 3: 3 + 7 (5 chars + 2 quotes) = 10
        assert!(
            size >= 10,
            "Sequence estimate should be significant: got {size}"
        );
    }

    #[test]
    fn test_emit_all_empty_slice() {
        let empty: Vec<Value> = vec![];
        let config = EmitterConfig::default();

        let result = Emitter::emit_all_with_config(&empty, &config).unwrap();
        assert!(result.is_empty(), "Empty input should produce empty output");
    }

    #[test]
    fn test_emit_all_buffer_preallocation() {
        // Create multiple documents to test buffer pre-allocation
        let docs: Vec<Value> = (0..10)
            .map(|i| Value::String(format!("document_{i}")))
            .collect();

        let config = EmitterConfig::default();
        let result = Emitter::emit_all_with_config(&docs, &config).unwrap();

        // Verify all documents are present
        for i in 0..10 {
            assert!(
                result.contains(&format!("document_{i}")),
                "Should contain document_{i}"
            );
        }

        // Verify document separators (9 separators for 10 documents)
        assert_eq!(
            result.matches("---").count(),
            9,
            "Should have 9 document separators"
        );
    }

    #[test]
    fn test_estimate_value_size_large_integer() {
        let size = |i: i64| Emitter::estimate_value_size(&Value::Int(i));
        assert!(size(i64::MAX) >= 19);
        assert!(size(i64::MIN) >= 19);
        assert!(size(1000) >= 4);
        assert!(size(1_000_000) >= 7);
    }

    // Regression tests for issue #64: YAML 1.1 boolean-like keys must not be quoted.
    // In YAML 1.2.2 Core Schema, `on`, `off`, `yes`, `no` are plain strings.
    #[test]
    fn test_format_yaml11_bool_key_on() {
        let result = Emitter::format("on: push").unwrap();
        assert!(
            !result.contains("\"on\""),
            "key `on` must not be quoted, got: {result}"
        );
        assert!(result.contains("on:"), "key `on` must appear unquoted");
    }

    #[test]
    fn test_format_yaml11_bool_key_off() {
        let result = Emitter::format("off: value").unwrap();
        assert!(
            !result.contains("\"off\""),
            "key `off` must not be quoted, got: {result}"
        );
        assert!(result.contains("off:"), "key `off` must appear unquoted");
    }

    #[test]
    fn test_format_yaml11_bool_key_yes() {
        let result = Emitter::format("yes: value").unwrap();
        assert!(
            !result.contains("\"yes\""),
            "key `yes` must not be quoted, got: {result}"
        );
        assert!(result.contains("yes:"), "key `yes` must appear unquoted");
    }

    #[test]
    fn test_format_yaml11_bool_key_no() {
        let result = Emitter::format("no: value").unwrap();
        assert!(
            !result.contains("\"no\""),
            "key `no` must not be quoted, got: {result}"
        );
        assert!(result.contains("no:"), "key `no` must appear unquoted");
    }

    #[test]
    fn test_format_github_actions_workflow() {
        let yaml = "on:\n  push:\n    branches:\n      - main\n";
        let result = Emitter::format(yaml).unwrap();
        assert!(
            !result.contains("\"on\""),
            "GitHub Actions `on:` trigger must not be quoted, got: {result}"
        );
        assert!(result.contains("on:"), "on: key must appear unquoted");
        assert!(result.contains("push:"), "push: key must appear");
    }

    #[test]
    fn test_format_yaml12_bools_unaffected() {
        // YAML 1.2.2 actual booleans must still be emitted as true/false
        let yaml = "enabled: true\ndisabled: false\n";
        let result = Emitter::format(yaml).unwrap();
        assert!(result.contains("true"), "true value must be preserved");
        assert!(result.contains("false"), "false value must be preserved");
    }

    #[test]
    fn test_format_preserves_float_types() {
        // Regression tests for issue #66: fy format must not change float type to integer

        // 1.0 must remain 1.0
        let result = Emitter::format("version: 1.0").unwrap();
        assert!(
            result.contains("1.0"),
            "version: 1.0 must emit as float, got: {result}"
        );
        assert!(
            !result.contains(": 1\n"),
            "version: 1.0 must not become integer 1, got: {result}"
        );

        // Scientific notation must be preserved
        let result = Emitter::format("count: 1.23e10").unwrap();
        assert!(
            result.contains("1.23e10") || result.contains("1.23e+10"),
            "Scientific notation must be preserved, got: {result}"
        );
        assert!(
            !result.contains("12300000000"),
            "Scientific notation must not expand to integer, got: {result}"
        );

        // Regular float preserved
        let result = Emitter::format("pi: 3.14").unwrap();
        assert!(
            result.contains("3.14"),
            "3.14 must be preserved, got: {result}"
        );
    }

    // Regression tests for issue #62: block scalar styles must be preserved by fy format.
    #[test]
    fn test_format_preserves_literal_block_scalar() {
        let input = "literal: |\n  line one\n  line two\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            result.contains("literal: |"),
            "literal block style should be preserved, got: {result}"
        );
        assert!(result.contains("line one"));
        assert!(result.contains("line two"));
    }

    #[test]
    fn test_format_preserves_folded_block_scalar() {
        let input = "folded: >\n  word1\n  word2\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            result.contains("folded: >"),
            "folded block style should be preserved, got: {result}"
        );
    }

    #[test]
    fn test_format_nested_literal_block_scalar() {
        let input = "outer:\n  inner: |\n    line one\n    line two\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            result.contains("inner: |"),
            "nested literal block should be preserved, got: {result}"
        );
        assert!(result.contains("line one"));
        assert!(result.contains("line two"));
    }

    #[test]
    fn test_format_mixed_block_and_plain_values() {
        let input = "plain: value\nliteral: |\n  block content\nnumber: 42\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(result.contains("plain: value"));
        assert!(result.contains("literal: |"));
        assert!(result.contains("block content"));
        assert!(result.contains("number: 42") || result.contains("number: '42'"));
    }

    #[test]
    fn test_format_block_scalar_not_double_quoted() {
        // Regression: before fix, block scalars were emitted as double-quoted strings
        let input = "key: |\n  multiline\n  content\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            !result.contains("\"multiline"),
            "block scalar should not be double-quoted, got: {result}"
        );
        assert!(
            result.contains("key: |"),
            "literal indicator must be present, got: {result}"
        );
    }

    // Regression tests for issue #65: multi-document YAML stream formatting
    #[test]
    fn test_format_multidoc_preserves_both_documents() {
        let input =
            "---\n- Mark McGwire\n- Sammy Sosa\n\n---\n- Chicago Cubs\n- St Louis Cardinals";
        let result = Emitter::format(input).unwrap();
        assert!(result.contains("Mark McGwire"), "First doc must be present");
        assert!(
            result.contains("Chicago Cubs"),
            "Second doc must be present"
        );
    }

    #[test]
    fn test_format_multidoc_separator_present() {
        let input = "---\nfoo: 1\n---\nbar: 2";
        let result = Emitter::format(input).unwrap();
        assert!(result.contains("---"), "Separator must appear in output");
        assert!(result.contains("foo"), "First doc key must be present");
        assert!(result.contains("bar"), "Second doc key must be present");
    }

    #[test]
    fn test_format_multidoc_issue65_fixture() {
        let input =
            "---\n- Mark McGwire\n- Sammy Sosa\n\n---\n- Chicago Cubs\n- St Louis Cardinals";
        let result = Emitter::format(input).unwrap();
        assert!(result.contains("Mark McGwire"));
        assert!(result.contains("Sammy Sosa"));
        assert!(result.contains("Chicago Cubs"));
        assert!(result.contains("St Louis Cardinals"));
        assert!(result.contains("---"));
    }

    #[test]
    fn test_format_multidoc_three_documents() {
        let input = "---\na: 1\n---\nb: 2\n---\nc: 3";
        let result = Emitter::format(input).unwrap();
        assert!(result.contains('a'), "First doc must be present");
        assert!(result.contains('b'), "Second doc must be present");
        assert!(result.contains('c'), "Third doc must be present");
        assert!(
            result.matches("---").count() >= 2,
            "At least two separators must appear between three documents"
        );
    }

    #[test]
    fn test_format_multidoc_explicit_start() {
        let input = "---\nfoo: 1\n---\nbar: 2";
        let config = EmitterConfig::new().with_explicit_start(true);
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(result.contains("foo"), "First doc must be present");
        assert!(result.contains("bar"), "Second doc must be present");
        assert!(
            !result.contains("---\n---"),
            "Double separator must not appear: {result}"
        );
        assert_eq!(
            result.matches("---").count(),
            2,
            "Exactly two separators for two docs"
        );
    }

    // Regression tests for issue #75: formatter must not produce trailing spaces
    // or double-indented sequence-of-mapping keys.

    #[test]
    fn test_format_nested_mapping_no_trailing_space() {
        // "parent:\n  child: value" — the colon after "parent" must not be followed
        // by a space when the value is a nested mapping.
        let result = Emitter::format("parent:\n  child: value\n").unwrap();
        assert!(
            !result.contains("parent: \n"),
            "trailing space after key with nested value, got: {result:?}"
        );
        assert!(
            result.contains("parent:\n"),
            "parent key must be followed by newline without space, got: {result:?}"
        );
        assert!(
            result.contains("child: value"),
            "child key-value must be preserved, got: {result:?}"
        );
    }

    #[test]
    fn test_format_sequence_of_mappings_indent() {
        // Steps with mapping items — first key of each item must align with the key,
        // not be indented an extra level beyond "- ".
        let yaml = "steps:\n  - uses: actions/checkout@v4\n  - name: Install Rust\n    uses: dtolnay/rust-toolchain@stable\n";
        let result = Emitter::format(yaml).unwrap();
        assert!(
            !result.contains("-     "),
            "sequence item keys must not be double-indented, got: {result:?}"
        );
        assert!(
            result.contains("- uses:"),
            "first item key must directly follow dash, got: {result:?}"
        );
        assert!(
            result.contains("uses: actions/checkout@v4"),
            "got: {result:?}"
        );
        assert!(
            result.contains("uses: dtolnay/rust-toolchain@stable"),
            "got: {result:?}"
        );
    }

    #[test]
    fn test_format_sequence_of_mappings_valid_yaml() {
        // Output must parse back to the same structure (no trailing spaces breaking YAML).
        let yaml = "steps:\n  - uses: actions/checkout@v4\n  - name: Install Rust\n    uses: dtolnay/rust-toolchain@stable\n";
        let result = Emitter::format(yaml).unwrap();
        let reparsed = crate::Parser::parse_str(&result);
        assert!(
            reparsed.is_ok(),
            "formatted output is invalid YAML: {result:?}"
        );
    }

    // Regression tests for issue #76: chomp indicator must not change during formatting.

    #[test]
    fn test_format_preserves_clip_chomp() {
        // `|` (clip) must remain `|`, not be converted to `|-`
        let input = "desc: |\n  line one\n  line two\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            result.contains("desc: |\n"),
            "clip chomp `|` must not be changed to `|-`, got: {result}"
        );
    }

    #[test]
    fn test_format_preserves_strip_chomp() {
        // `|-` (strip) must remain `|-`
        let input = "desc: |-\n  line one\n  line two\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            result.contains("desc: |-\n"),
            "strip chomp `|-` must be preserved, got: {result}"
        );
    }

    #[test]
    fn test_format_preserves_keep_chomp() {
        // `|+` (keep) must remain `|+` when value has trailing blank lines
        let input = "desc: |+\n  line one\n  line two\n\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            result.contains("desc: |+\n"),
            "keep chomp `|+` must be preserved, got: {result}"
        );
    }

    #[test]
    fn test_format_preserves_folded_clip_chomp() {
        // `>` (folded clip) must remain `>`, not be converted to `>-`
        let input = "desc: >\n  line one\n  line two\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            result.contains("desc: >\n"),
            "folded clip `>` must not be changed to `>-`, got: {result}"
        );
    }

    // Regression tests for issue #94: emit output must always end with newline
    #[test]
    fn test_emit_str_ends_with_newline() {
        let value = Value::String("hello".to_string());
        let result = Emitter::emit_str(&value).unwrap();
        assert!(
            result.ends_with('\n'),
            "emit_str output must end with newline, got: {result:?}"
        );
    }

    #[test]
    fn test_emit_str_with_config_ends_with_newline_default() {
        let value = Value::String("hello".to_string());
        let config = EmitterConfig::default();
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(
            result.ends_with('\n'),
            "emit_str_with_config output must end with newline (default config), got: {result:?}"
        );
    }

    #[test]
    fn test_emit_str_with_config_ends_with_newline_explicit_start() {
        let value = Value::String("hello".to_string());
        let config = EmitterConfig::new().with_explicit_start(true);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(
            result.ends_with('\n'),
            "emit_str_with_config output must end with newline (explicit_start=true), got: {result:?}"
        );
    }

    // Regression tests for issue #95: format must preserve %YAML and %TAG directives
    #[test]
    fn test_format_preserves_yaml_directive() {
        let input = "%YAML 1.2\n---\nkey: value\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            result.contains("%YAML 1.2"),
            "format_with_config must preserve %YAML directive, got: {result:?}"
        );
    }

    #[test]
    fn test_format_preserves_tag_directive() {
        let input = "%TAG ! tag:example.com,2000:app/\n---\nkey: value\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            result.contains("%TAG ! tag:example.com,2000:app/"),
            "format_with_config must preserve %TAG directive, got: {result:?}"
        );
    }

    #[test]
    fn test_format_without_directives_works_normally() {
        let input = "key: value\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            result.contains("key: value"),
            "format_with_config without directives must work normally, got: {result:?}"
        );
        assert!(
            result.ends_with('\n'),
            "format_with_config output must end with newline, got: {result:?}"
        );
    }

    #[test]
    fn test_format_yaml_directive_precedes_document_start() {
        let input = "%YAML 1.2\n---\nkey: value\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        let yaml_pos = result
            .find("%YAML 1.2")
            .expect("%YAML directive must be present");
        let doc_start_pos = result.find("---").expect("--- must be present");
        assert!(
            yaml_pos < doc_start_pos,
            "%YAML directive must appear before ---, got: {result:?}"
        );
    }

    #[test]
    fn test_format_yaml_and_tag_directives_together() {
        let input = "%YAML 1.2\n%TAG ! tag:example.com,2000:app/\n---\nkey: value\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            result.contains("%YAML 1.2"),
            "%YAML directive must be preserved, got: {result:?}"
        );
        assert!(
            result.contains("%TAG ! tag:example.com,2000:app/"),
            "%TAG directive must be preserved, got: {result:?}"
        );
        let yaml_pos = result.find("%YAML 1.2").unwrap();
        let tag_pos = result.find("%TAG").unwrap();
        let doc_pos = result.find("---").unwrap();
        assert!(
            yaml_pos < doc_pos,
            "%YAML must precede ---, got: {result:?}"
        );
        assert!(tag_pos < doc_pos, "%TAG must precede ---, got: {result:?}");
    }

    #[test]
    fn test_emit_str_string_inf_nan_stay_strings() {
        for text in ["inf", "-inf", "NaN"] {
            let value = Value::String(text.to_string());
            let emitted = Emitter::emit_str(&value).unwrap();
            let reparsed = crate::Parser::parse_str(&emitted).unwrap();
            assert_eq!(reparsed, Some(value), "{text:?} emitted as {emitted:?}");
        }
    }

    #[test]
    fn test_format_keeps_directives_of_later_documents() {
        let config = EmitterConfig::default();
        let input = "a\n...\n%YAML 1.2\n---\nb\n";
        let once = Emitter::format_with_config(input, &config).unwrap();
        assert_eq!(once, input);
        assert_eq!(Emitter::format_with_config(&once, &config).unwrap(), once);
    }

    #[test]
    fn test_format_directives_in_every_document() {
        let config = EmitterConfig::default();
        let input = "%YAML 1.2\n---\na: 1\n...\n%TAG !e! tag:example.com,2000:\n---\nb: !e!x 2\n...\n%YAML 1.2\n---\nc: 3\n";
        let once = Emitter::format_with_config(input, &config).unwrap();
        assert_eq!(once.matches("%YAML 1.2\n---\n").count(), 2);
        assert!(once.contains("...\n%TAG !e! tag:example.com,2000:\n---\n"));
        assert_eq!(Emitter::format_with_config(&once, &config).unwrap(), once);
    }

    #[test]
    fn test_format_no_document_end_marker_without_directives() {
        let config = EmitterConfig::default();
        let input = "a\n---\nb\n";
        assert_eq!(Emitter::format_with_config(input, &config).unwrap(), input);
    }

    #[test]
    fn test_format_directive_after_non_ascii_and_crlf() {
        let config = EmitterConfig::default();
        let input = "k: \"\u{e9}\"\r\n...\r\n%YAML 1.2\r\n---\r\nb\r\n";
        let out = Emitter::format_with_config(input, &config).unwrap();
        assert!(out.contains("...\n%YAML 1.2\n---\n"), "{out:?}");
    }

    #[test]
    fn test_format_percent_scalar_content_is_not_a_directive() {
        let config = EmitterConfig::default();
        let input = "--- |\n%YAML 1.2\n---\nb\n";
        let out = Emitter::format_with_config(input, &config).unwrap();
        assert!(!out.contains("...\n%YAML"), "{out:?}");
    }

    #[test]
    fn test_format_plain_inf_nan_verbatim() {
        let config = EmitterConfig::default();
        let input = "a: inf\nb: NaN\nc: -inf\ne: .inf\nf: .nan\ng: !!float inf\n";
        assert_eq!(Emitter::format_with_config(input, &config).unwrap(), input);
    }

    #[test]
    fn test_format_directive_only_on_first_document_in_multidoc_stream() {
        let input = "%YAML 1.2\n---\nfirst: doc\n---\nsecond: doc\n";
        let config = EmitterConfig::default();
        let result = Emitter::format_with_config(input, &config).unwrap();
        assert!(
            result.contains("%YAML 1.2"),
            "%YAML directive must be present in output, got: {result:?}"
        );
        // Directive must appear only once (before first document)
        assert_eq!(
            result.matches("%YAML 1.2").count(),
            1,
            "Directive must appear exactly once, got: {result:?}"
        );
        assert!(
            result.contains("first: doc"),
            "first document must be present, got: {result:?}"
        );
        assert!(
            result.contains("second: doc"),
            "second document must be present, got: {result:?}"
        );
    }

    // Tests for issue #127: indent and default_flow_style must be applied.

    #[test]
    fn test_emit_with_indent_4() {
        let mut map = Mapping::new();
        map.insert(
            Value::String("key".to_string()),
            Value::Sequence(vec![Value::Int(1), Value::Int(2)]),
        );
        let value = Value::Mapping(map);
        let config = EmitterConfig::new().with_indent(Indent::new(4).unwrap());
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        // With indent=4, list items under a key should start with 4 spaces.
        assert!(
            result.contains("    - 1") || result.contains("    -"),
            "indent=4 should produce 4-space indentation, got: {result:?}"
        );
    }

    #[test]
    fn test_emit_default_flow_style_true_mapping() {
        let mut map = Mapping::new();
        map.insert(Value::String("a".to_string()), Value::Int(1));
        map.insert(Value::String("b".to_string()), Value::Int(2));
        let value = Value::Mapping(map);
        let config = EmitterConfig::new().with_default_flow_style(Some(true));
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(
            result.contains('{') && result.contains('}'),
            "default_flow_style=true should produce flow mapping {{...}}, got: {result:?}"
        );
        assert!(result.contains("a: 1"), "mapping key a must be present");
        assert!(result.contains("b: 2"), "mapping key b must be present");
    }

    #[test]
    fn test_string_merge_key_round_trips_in_every_style() {
        let doc =
            crate::Parser::parse_str(r#"{"m": {"<<": {"admin": true}, "k": 0}, "n": {"<<": 1}}"#)
                .unwrap()
                .unwrap();
        for flow in [Some(true), Some(false), None] {
            let config = EmitterConfig::new().with_default_flow_style(flow);
            let yaml = Emitter::emit_str_with_config(&doc, &config).unwrap();
            assert_eq!(
                crate::Parser::parse_str(&yaml).unwrap().unwrap(),
                doc,
                "{flow:?}: {yaml}"
            );
        }
    }

    #[test]
    fn test_strings_with_a_bom_round_trip_in_every_style() {
        let mut map = Mapping::new();
        map.insert(
            Value::String("\u{FEFF}admin".into()),
            Value::String("a\u{FEFF}b".into()),
        );
        map.insert(Value::String("k".into()), Value::String("\u{FEFF}".into()));
        for doc in [
            Value::Mapping(map),
            Value::String("\u{FEFF}root".into()),
            Value::Sequence(vec![Value::String("\u{FEFF}".into())]),
        ] {
            for flow in [Some(true), Some(false), None] {
                let config = EmitterConfig::new().with_default_flow_style(flow);
                let yaml = Emitter::emit_str_with_config(&doc, &config).unwrap();
                assert_eq!(
                    crate::Parser::parse_str(&yaml).unwrap().unwrap(),
                    doc,
                    "{flow:?}: {yaml:?}"
                );
            }
        }
    }

    #[test]
    fn test_format_escapes_a_bom_inside_scalars() {
        for input in ["\"\t\u{FEFF}}\"\n", "a: \" \u{FEFF}\"\n", "k: x\u{FEFF}y\n"] {
            let out = Emitter::format(input).unwrap();
            assert_eq!(
                crate::Parser::parse_all(input).unwrap(),
                crate::Parser::parse_all(&out).unwrap(),
                "{out:?}"
            );
        }
    }

    #[test]
    fn test_format_rejects_recursive_aliases() {
        for input in ["&a [*a]\n", "--- &r\nb: *r\n", "&a {k: *a}\n"] {
            let err = Emitter::format(input).unwrap_err();
            assert!(err.to_string().contains("still being defined"), "{err}");
        }
        assert!(Emitter::format("a: &x 1\nb: *x\n").is_ok());
    }

    #[test]
    fn test_format_writes_an_empty_root_block_scalar_quoted() {
        for input in [
            ">\n...\n\u{FEFF}\r1!",
            "|-\n---\nb\n",
            "--- >\n--- |+\n--- 1\n",
        ] {
            let out = Emitter::format(input).unwrap();
            assert_eq!(
                crate::Parser::parse_all(input).unwrap(),
                crate::Parser::parse_all(&out).unwrap(),
                "{input:?} -> {out:?}"
            );
        }
        assert_eq!(Emitter::format("k: |-\n").unwrap(), "k: |-\n");
    }

    #[test]
    fn test_format_keeps_a_bom_that_starts_a_root_plain_scalar() {
        for input in [
            "--- \u{FEFF}x\n",
            "---\n\u{FEFF}x\n",
            "--- |-\n  \u{FEFF}x\n",
        ] {
            let out = Emitter::format(input).unwrap();
            assert_eq!(
                crate::Parser::parse_all(input).unwrap(),
                crate::Parser::parse_all(&out).unwrap(),
                "{input:?} -> {out:?}"
            );
        }
    }

    #[test]
    fn test_format_escapes_a_bom_that_would_start_the_stream() {
        for input in [
            "\n\u{FEFF}- a\n",
            "\n\u{FEFF}[1, 2]\n",
            "# c\n\u{FEFF}admin: true\n",
            "  \u{FEFF}{a: 1}",
            "\t\u{FEFF}}",
            " \u{FEFF}",
        ] {
            let out = Emitter::format(input).unwrap();
            assert_eq!(
                crate::Parser::parse_all(input).unwrap(),
                crate::Parser::parse_all(&out).unwrap(),
                "{input:?} -> {out:?}"
            );
            assert!(!out.contains('\u{FEFF}'), "{out:?}");
        }
    }

    #[test]
    fn test_tag_space_is_stripped_after_a_key_with_leading_unicode_whitespace() {
        let mut set = Set::new();
        set.insert(Value::String("a".into()));
        let mut map = Mapping::new();
        map.insert(Value::String("k".into()), Value::String("one\ntwo".into()));
        map.insert(Value::String("\u{A0}j".into()), Value::Set(set));
        let doc = Value::Mapping(map);
        let config = EmitterConfig::new().with_multiline_strings(true);
        let out = Emitter::emit_str_with_config(&doc, &config).unwrap();
        assert!(out.contains("|-"), "{out:?}");
        assert!(!out.lines().any(|l| l.ends_with(' ')), "{out:?}");
        assert_eq!(crate::Parser::parse_str(&out).unwrap().unwrap(), doc);
    }

    fn map_of(entries: Vec<(&str, Value)>) -> Value {
        Value::Mapping(
            entries
                .into_iter()
                .map(|(k, v)| (Value::String(k.into()), v))
                .collect(),
        )
    }

    #[test]
    fn test_document_markers_inside_a_block_scalar_survive_any_indent() {
        let doc = map_of(vec![(
            "k",
            map_of(vec![(
                "v",
                Value::String("line1\n---x\n...\n%YAML y\nline3".into()),
            )]),
        )]);
        for indent in [2, 4] {
            let config = EmitterConfig::new()
                .with_indent(Indent::new(indent).unwrap())
                .with_multiline_strings(true);
            let yaml = Emitter::emit_str_with_config(&doc, &config).unwrap();
            assert_eq!(
                crate::Parser::parse_str(&yaml).unwrap().unwrap(),
                doc,
                "{yaml}"
            );
        }
    }

    #[test]
    fn test_indent_applies_when_a_key_or_scalar_holds_a_block_indicator() {
        let doc = map_of(vec![(
            "x>y",
            map_of(vec![("c", map_of(vec![("d", Value::Int(1))]))]),
        )]);
        let config = EmitterConfig::new().with_indent(Indent::new(4).unwrap());
        let yaml = Emitter::emit_str_with_config(&doc, &config).unwrap();
        assert_eq!(yaml, "x>y:\n    c:\n        d: 1\n");
    }

    #[test]
    fn test_multiline_strings_a_literal_block_cannot_hold_are_quoted() {
        for text in [" a\nb", "\n\n", "a\r\nb", "a\n\n", "\tz\nq", "a\rb"] {
            let doc = map_of(vec![("k", Value::String(text.into()))]);
            for indent in [2, 4] {
                let config = EmitterConfig::new()
                    .with_indent(Indent::new(indent).unwrap())
                    .with_multiline_strings(true);
                let yaml = Emitter::emit_str_with_config(&doc, &config).unwrap();
                assert_eq!(
                    crate::Parser::parse_str(&yaml).unwrap().unwrap(),
                    doc,
                    "{text:?} at indent {indent}: {yaml:?}"
                );
            }
        }
    }

    #[test]
    fn test_reindent_ignores_unicode_whitespace_in_keys() {
        let mut inner = Mapping::new();
        inner.insert(Value::String("name".into()), Value::String("bob".into()));
        let mut outer = Mapping::new();
        outer.insert(Value::String("user".into()), Value::Mapping(inner));
        for key in [
            " admin",
            "\u{A0}admin",
            "\u{3000}admin",
            "\u{85}admin",
            "\u{2028}admin",
        ] {
            let mut doc = outer.clone();
            doc.insert(Value::String(key.into()), Value::Bool(true));
            let doc = Value::Mapping(doc);
            for indent in 3..=9 {
                let config = EmitterConfig::new().with_indent(Indent::new(indent).unwrap());
                let yaml = Emitter::emit_str_with_config(&doc, &config).unwrap();
                assert_eq!(
                    crate::Parser::parse_str(&yaml).unwrap().unwrap(),
                    doc,
                    "indent {indent}, key {key:?}: {yaml:?}"
                );
            }
        }
    }

    #[test]
    fn test_emit_default_flow_style_true_sequence() {
        let value = Value::Sequence(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let config = EmitterConfig::new().with_default_flow_style(Some(true));
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(
            result.contains('[') && result.contains(']'),
            "default_flow_style=true should produce flow sequence [...], got: {result:?}"
        );
        assert!(result.contains("1, 2, 3"), "sequence items must be inline");
    }

    #[test]
    fn test_emit_default_flow_style_none_is_block() {
        let value = Value::Sequence(vec![Value::Int(1), Value::Int(2)]);
        let config = EmitterConfig::new().with_default_flow_style(None);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        // Block style uses "- " prefix per item.
        assert!(
            result.contains("- 1"),
            "default_flow_style=None should produce block sequence, got: {result:?}"
        );
    }

    #[test]
    fn test_indent_rescales_block_output() {
        let config = EmitterConfig::new().with_indent(Indent::new(4).unwrap());
        let doc = crate::Parser::parse_str("key:\n  nested: value\n")
            .unwrap()
            .unwrap();
        let result = Emitter::emit_str_with_config(&doc, &config).unwrap();
        assert_eq!(result, "key:\n    nested: value\n");
        let config = config.with_explicit_start(true);
        let result = Emitter::emit_str_with_config(&doc, &config).unwrap();
        assert_eq!(result, "---\nkey:\n    nested: value\n");
    }

    #[test]
    fn test_nested_sequences_and_complex_keys_survive_any_indent() {
        let doc = crate::Parser::parse_str(
            "perms: [[read, write], [true, 13]]\n? [a, b]\n: [[1], {k: [2]}]\nm: !!set {x, y}\n",
        )
        .unwrap()
        .unwrap();
        for indent in 1..=9 {
            let config = EmitterConfig::new().with_indent(Indent::new(indent).unwrap());
            let yaml = Emitter::emit_str_with_config(&doc, &config).unwrap();
            assert_eq!(
                crate::Parser::parse_str(&yaml).unwrap().unwrap(),
                doc,
                "indent {indent}: {yaml}"
            );
        }
    }

    #[test]
    fn test_format_with_config_keeps_a_leading_bom() {
        let out =
            Emitter::format_with_config("\u{FEFF}# c\na: 1\n", &EmitterConfig::default()).unwrap();
        assert_eq!(out, "\u{FEFF}a: 1\n");
        let again = Emitter::format_with_config(&out, &EmitterConfig::default()).unwrap();
        assert_eq!(again, out);
    }

    #[test]
    fn test_format_with_config_bom_only_input_stays_bom_only() {
        let out = Emitter::format_with_config("\u{FEFF}", &EmitterConfig::default()).unwrap();
        assert_eq!(out.trim_start_matches('\u{FEFF}').trim(), "");
        assert!(out.starts_with('\u{FEFF}'));
    }

    #[test]
    fn test_format_with_config_drops_prefix_boms_of_later_documents() {
        let out = Emitter::format_with_config(
            "a\n...\n\u{FEFF}%YAML 1.2\n---\nb\n",
            &EmitterConfig::default(),
        )
        .unwrap();
        assert!(!out.contains('\u{FEFF}'), "{out:?}");
        let second = Emitter::format_with_config(&out, &EmitterConfig::default()).unwrap();
        assert_eq!(second, out);
    }

    #[test]
    fn test_format_without_bom_does_not_add_one() {
        let out = Emitter::format_with_config("a: 1\n", &EmitterConfig::default()).unwrap();
        assert!(!out.contains('\u{FEFF}'));
    }

    #[test]
    fn test_format_with_config_bom_before_directive() {
        let out = Emitter::format_with_config(
            "\u{FEFF}%YAML 1.2\n---\na: 1\n",
            &EmitterConfig::default(),
        )
        .unwrap();
        assert!(out.starts_with("\u{FEFF}%YAML 1.2\n"), "got {out:?}");
    }

    #[test]
    fn test_format_with_config_bom_crlf_multi_doc() {
        let out = Emitter::format_with_config(
            "\u{FEFF}---\r\na: 1\r\n---\r\nb: 2\r\n",
            &EmitterConfig::default(),
        )
        .unwrap();
        assert_eq!(out.matches('\u{FEFF}').count(), 1);
        assert!(out.contains("a: 1") && out.contains("b: 2"));
    }

    fn format_at(input: &str, indent: usize) -> String {
        let config = EmitterConfig::new().with_indent(Indent::new(indent).unwrap());
        Emitter::format_with_config(input, &config).unwrap()
    }

    /// Asserts value preservation, idempotency and no trailing whitespace; returns the output.
    fn assert_round_trip(input: &str, indent: usize) -> String {
        let out = format_at(input, indent);
        assert_eq!(
            crate::Parser::parse_all(input).unwrap(),
            crate::Parser::parse_all(&out).unwrap(),
            "value changed (indent {indent}): {out:?}"
        );
        assert_eq!(
            out,
            format_at(&out, indent),
            "not idempotent (indent {indent})"
        );
        assert!(
            !out.lines().any(|l| l.ends_with(' ')),
            "trailing whitespace (indent {indent}): {out:?}"
        );
        out
    }

    #[test]
    fn test_block_scalar_header() {
        assert_eq!(
            block_scalar_header('|', "a\n", Indent::new(2).unwrap()),
            "|"
        );
        assert_eq!(block_scalar_header('|', "a", Indent::new(2).unwrap()), "|-");
        assert_eq!(
            block_scalar_header('>', "a\n\n", Indent::new(2).unwrap()),
            ">+"
        );
        assert_eq!(
            block_scalar_header('|', " a\n", Indent::new(4).unwrap()),
            "|4"
        );
        assert_eq!(
            block_scalar_header('|', " a\n\n", Indent::new(3).unwrap()),
            "|3+"
        );
        assert_eq!(
            block_scalar_header('|', " a", Indent::new(1).unwrap()),
            "|1-"
        );
        assert_eq!(
            block_scalar_header('>', "\n  a\n", Indent::new(2).unwrap()),
            ">2"
        );
        assert_eq!(
            block_scalar_header('|', "\n\n", Indent::new(2).unwrap()),
            "|+"
        );
        assert_eq!(
            block_scalar_header('|', "\n", Indent::new(2).unwrap()),
            "|+"
        );
        assert_eq!(
            block_scalar_header('|', "\ta\n", Indent::new(2).unwrap()),
            "|"
        );
    }

    #[test]
    fn test_block_scalar_leading_space_preserved() {
        let cases = [
            "a: |1\n  x\n",
            "a: |2\n   x\n",
            "a: |2+\n   x\n\n",
            "a: |1-\n  x\n  y",
            "a: >2\n   x\n",
            "a: |2\n\n    lead\n",
            "a:\n  b: |2\n     x\n  c: 1\n",
            "- |1\n  x\n- >1\n  y\n",
            "k:\n  - |2\n     x\n",
        ];
        for indent in [2, 3, 4, 8] {
            for case in cases {
                assert_round_trip(case, indent);
            }
        }
        // Siblings of a compact `- -` first child only line up at indent 2 (pre-existing)
        assert_round_trip("- - |1\n    x\n  - 1\n", 2);
    }

    #[test]
    fn test_block_scalar_compact_parents_use_real_column() {
        let cases = [
            "- a: |2\n     x\n",
            "- - |1\n    x\n",
            "- - - |1\n      x\n",
            "- - a: |1\n      x\n",
            "a:\n  - - |1\n      x\n",
            "- a:\n    b: |1\n      x\n",
        ];
        for indent in [2, 3, 4, 8] {
            for case in cases {
                assert_round_trip(case, indent);
            }
        }
    }

    #[test]
    fn test_block_scalar_compact_mapping_sibling_indent_2() {
        assert_round_trip("- a: |2\n     x\n  b: 1\n", 2);
    }

    #[test]
    fn test_block_scalar_root_leading_space() {
        for case in [
            "|2\n   root\n",
            ">2\n   root\n",
            "--- |1\n  root\n",
            "|+\n  x\n\n",
        ] {
            for indent in [2, 3, 4, 8] {
                assert_round_trip(case, indent);
            }
        }
    }

    #[test]
    fn test_block_scalar_lone_newline_keep() {
        for case in ["a: |+\n\nb: 1\n", "a: |+\n\n\nb: 1\n", "- |+\n\n- 1\n"] {
            for indent in [2, 4] {
                assert_round_trip(case, indent);
            }
        }
    }

    #[test]
    fn test_block_scalar_keep_and_folded_chomp_wide_indent() {
        let cases = [
            "a: >+\n  x\n\n",
            "a: >2+\n   x\n\n\n",
            "a: |+\n  x\n\n\n\nb: 1\n",
            "a: |2+\n   x\n\n\nb: 1\n",
        ];
        for indent in [3, 4, 8] {
            for case in cases {
                assert_round_trip(case, indent);
            }
        }
    }

    #[test]
    fn test_block_scalar_whitespace_only_first_line_preserved() {
        let input = "a: |2\n   \n   x\n";
        for indent in [2, 4] {
            let out = format_at(input, indent);
            assert_eq!(
                crate::Parser::parse_all(input).unwrap(),
                crate::Parser::parse_all(&out).unwrap(),
                "{out:?}"
            );
            assert_eq!(out, format_at(&out, indent));
        }
    }

    #[test]
    fn test_block_scalar_keep_chomp_stable() {
        for input in [
            "a: |+\n  x\n\n\nb: 1\n",
            "a: |+\n  x\n\n",
            "- |+\n  x\n\n- y\n",
        ] {
            assert_round_trip(input, 2);
        }
    }

    #[test]
    fn test_no_extra_blank_line_after_block_scalar() {
        let out = format_at("a: |\n  x\nb: 1\n", 2);
        assert_eq!(out, "a: |\n  x\nb: 1\n");
    }

    #[test]
    fn test_null_forms_round_trip() {
        let cases = [
            "a:\nb: 1\n",
            "- \n- x\n",
            "a: &an\nb: *an\n",
            "- &an\n- *an\n",
            "!!set {x, y}\n",
            "{a, b: }\n",
            "? a\n: v\n? \n: w\n",
            "a: ~\nb: null\nc: \"\"\nd: ''\n",
        ];
        for case in cases {
            let out = assert_round_trip(case, 2);
            assert!(!out.contains(": \n") && !out.contains(":\n"), "got {out:?}");
        }
    }

    #[test]
    fn test_null_value_and_empty_string_distinct() {
        let out = format_at("a:\nb: \"\"\nc: ~\n", 2);
        assert!(out.contains("b: \"\""), "got {out:?}");
        assert!(out.contains("c: ~"), "got {out:?}");
        assert!(
            out.starts_with("a: null\n") || out.starts_with("a: ~\n"),
            "got {out:?}"
        );
    }

    #[test]
    fn test_streaming_omitted_null_emitted_as_null() {
        assert_eq!(format_at("a:\nb: 1\n", 2), "a: null\nb: 1\n");
        assert_eq!(format_at("- \n- x\n", 2), "- null\n- x\n");
        assert_eq!(format_at("a: &an\n", 2), "a: &an null\n");
    }

    #[test]
    fn test_empty_document_is_stable() {
        let out = format_at("---\n", 2);
        assert_eq!(out, format_at(&out, 2));
    }

    #[test]
    fn test_format_preserves_explicit_str_tag() {
        let out = assert_round_trip("a: !!str 1\n", 2);
        assert_eq!(out, "a: !!str 1\n");
        let docs = crate::Parser::parse_all(&out).unwrap();
        let Value::Mapping(map) = &docs[0] else {
            panic!("expected mapping, got {:?}", docs[0]);
        };
        assert!(
            map.values()
                .all(|v| matches!(v, Value::String(s) if s == "1")),
            "tag lost its string type: {map:?}"
        );
    }

    #[test]
    fn test_format_preserves_custom_tags_on_collections() {
        let out = assert_round_trip("m: !custom\n  a: 1\ns: !seq\n  - x\n  - y\n", 2);
        assert!(out.contains("m: !custom\n"), "got {out:?}");
        assert!(out.contains("s: !seq\n"), "got {out:?}");
    }

    #[test]
    fn test_format_preserves_verbatim_tag() {
        let out = assert_round_trip("a: !<tag:example.com,2000:x> v\n", 2);
        assert!(out.contains("!<tag:example.com,2000:x> v"), "got {out:?}");
    }

    #[test]
    fn test_format_preserves_root_tag() {
        let out = assert_round_trip("--- !!str foo\n", 2);
        assert!(out.contains("!!str foo"), "got {out:?}");
    }

    #[test]
    fn test_format_preserves_tags_across_documents() {
        let out = assert_round_trip("--- !!str 1\n--- !custom\na: !!str 2\n", 2);
        assert!(out.contains("!!str 1"), "got {out:?}");
        assert!(out.contains("!custom\na:"), "got {out:?}");
        assert!(out.contains("a: !!str 2"), "got {out:?}");
    }

    #[test]
    fn test_format_preserves_anchor_tag_and_alias() {
        let out = assert_round_trip("a: &x !!str 1\nb: *x\n", 2);
        assert!(
            out.contains("&x !!str 1") || out.contains("!!str &x 1"),
            "got {out:?}"
        );
        assert!(out.contains("b: *x"), "got {out:?}");
    }

    #[test]
    fn test_format_preserves_tagged_block_scalar() {
        let out = assert_round_trip("a: !!str |\n  line1\n  line2\n", 2);
        assert!(out.contains("a: !!str |\n"), "got {out:?}");
    }

    #[test]
    fn test_format_empty_input_is_empty() {
        assert_eq!(format_at("", 2), "");
    }

    fn flow_with_key(key: Value) -> String {
        let mut map = Mapping::new();
        map.insert(key, Value::Int(1));
        let config = EmitterConfig::new().with_default_flow_style(Some(true));
        Emitter::emit_str_with_config(&Value::Mapping(map), &config).unwrap()
    }

    fn string_key(s: &str) -> Value {
        Value::String(s.to_owned())
    }

    #[test]
    fn flow_string_keys_are_escaped_and_round_trip() {
        for key in [
            "x: \"y\"",
            "a\nb",
            "it's",
            "c\\d",
            "",
            "1",
            "true",
            "null",
            ".5",
            "+.inf",
            "- a",
            "[a",
            "a, b",
            "&a",
            "*a",
            "!t",
            "a\tb",
            " a",
            "a#b",
            "-.5",
            "a\u{2028}b",
            "a\u{85}b",
            "a\x7fb",
            "<<",
        ] {
            let out = flow_with_key(string_key(key));
            let Some(Value::Mapping(map)) = crate::Parser::parse_str(&out).unwrap() else {
                panic!("mapping expected for {key:?}: {out}");
            };
            let (k, _) = map.iter().next().unwrap();
            assert_eq!(k, &string_key(key), "{key:?}: {out}");
        }
    }

    #[test]
    fn flow_merge_lookalike_key_is_quoted() {
        assert_eq!(flow_with_key(string_key("<<")), "{\"<<\": 1}\n");
    }

    #[test]
    fn non_string_lookalike_strings_round_trip_in_every_position() {
        for text in [
            "+.inf", "+.Inf", "+.INF", ".5", "-.5", "+.5e3", ".inf", "1", "true", "null",
        ] {
            let mut inner = Mapping::new();
            inner.insert(string_key(text), string_key(text));
            let value = Value::Mapping(inner);
            let seq = Value::Sequence(vec![string_key(text), value.clone()]);
            for flow in [false, true] {
                let config = EmitterConfig::new().with_default_flow_style(flow.then_some(true));
                for doc in [&value, &seq, &string_key(text)] {
                    let out = Emitter::emit_str_with_config(doc, &config).unwrap();
                    let back = crate::Parser::parse_str(&out).unwrap().unwrap();
                    assert_eq!(&back, doc, "{text:?} flow={flow}: {out}");
                }
            }
        }
    }

    const BIG: &str = "123456789012345678901234567890";

    fn roundtrip_all_styles(doc: &Value) {
        for flow in [None, Some(true)] {
            let config = EmitterConfig::new().with_default_flow_style(flow);
            let out = Emitter::emit_str_with_config(doc, &config).unwrap();
            let back = crate::Parser::parse_str(&out).unwrap().unwrap();
            let again = Emitter::emit_str_with_config(&back, &config).unwrap();
            assert_eq!(out, again, "flow={flow:?}: {out}");
            assert!(out.contains(BIG) && !out.contains("tag:yaml.org"), "{out}");
        }
    }

    #[test]
    fn emit_parsed_block_scalar_big_int_does_not_panic() {
        for input in [
            format!("? !!int >-\n  {BIG}\n: v\n"),
            format!("? !!int |-\n  {BIG}\n: v\n"),
            format!("k: !!int |-\n  {BIG}\n"),
            format!("- !!int >-\n  {BIG}\n"),
            format!("--- !!int |-\n  -{BIG}\n"),
            format!("k: &a !!int >-\n  {BIG}\nj: *a\n"),
        ] {
            let doc = crate::Parser::parse_str(&input).unwrap().unwrap();
            roundtrip_all_styles(&doc);
            let out = Emitter::emit_str(&doc).unwrap();
            assert!(out.contains(BIG), "{input:?} -> {out:?}");
        }
    }

    #[test]
    fn emit_core_tagged_quoted_big_int_has_no_expanded_tag() {
        for input in [
            format!("k: !!int \"{BIG}\"\n"),
            format!("k: !!int '{BIG}'\n"),
        ] {
            let doc = crate::Parser::parse_str(&input).unwrap().unwrap();
            roundtrip_all_styles(&doc);
            assert_eq!(Emitter::emit_str(&doc).unwrap(), format!("k: {BIG}\n"));
        }
    }

    #[test]
    fn emit_custom_tagged_big_int_drops_the_tag() {
        let input = format!("k: !foo {BIG}\n");
        let doc = crate::Parser::parse_str(&input).unwrap().unwrap();
        assert_eq!(Emitter::emit_str(&doc).unwrap(), format!("k: {BIG}\n"));
        let flow = EmitterConfig::new().with_default_flow_style(Some(true));
        assert_eq!(
            Emitter::emit_str_with_config(&doc, &flow).unwrap(),
            format!("{{k: {BIG}}}\n")
        );
    }

    #[test]
    fn emit_radix_big_ints_in_flow_collections_and_keys_as_decimal() {
        let decimal = "4722366482869645213695";
        for input in [
            "[0xFFFFFFFFFFFFFFFFFF]",
            "{a: 0xFFFFFFFFFFFFFFFFFF}",
            "{0xFFFFFFFFFFFFFFFFFF: a}",
            "0xFFFFFFFFFFFFFFFFFF: a\nb: [0o7777777777777777777777]\n",
        ] {
            let doc = crate::Parser::parse_str(input).unwrap().unwrap();
            for out in emit_both(&doc) {
                assert!(!out.contains("0x") && !out.contains("0o"), "{input}: {out}");
                let back = crate::Parser::parse_str(&out).unwrap().unwrap();
                assert!(!Emitter::emit_str(&back).unwrap().contains("0x"), "{input}");
            }
        }
        let doc = crate::Parser::parse_str("{0xFFFFFFFFFFFFFFFFFF: 0xFFFFFFFFFFFFFFFFFF}")
            .unwrap()
            .unwrap();
        let flow = EmitterConfig::new().with_default_flow_style(Some(true));
        assert_eq!(
            Emitter::emit_str_with_config(&doc, &flow).unwrap(),
            format!("{{{decimal}: {decimal}}}\n")
        );
    }

    #[test]
    fn emit_radix_big_ints_as_canonical_decimal_and_quote_strings() {
        let doc =
            crate::Parser::parse_str("a: 0xFFFFFFFFFFFFFFFFFF\nb: 0o7777777777777777777777\n")
                .unwrap()
                .unwrap();
        let decimal =
            crate::Parser::parse_str("a: 4722366482869645213695\nb: 73786976294838206463\n")
                .unwrap()
                .unwrap();
        for out in emit_both(&doc) {
            let back = crate::Parser::parse_str(&out).unwrap().unwrap();
            assert_eq!(back, decimal, "{out}");
        }
        assert_eq!(
            Emitter::emit_str(&doc).unwrap(),
            "a: 4722366482869645213695\nb: 73786976294838206463\n"
        );
        let quoted = crate::Parser::parse_str("a: \"0xFFFFFFFFFFFFFFFFFF\"\n")
            .unwrap()
            .unwrap();
        assert_eq!(
            Emitter::emit_str(&quoted).unwrap(),
            "a: \"0xFFFFFFFFFFFFFFFFFF\"\n"
        );
    }

    #[test]
    fn emit_all_with_block_scalar_big_ints() {
        let docs = [
            crate::Parser::parse_str(&format!("!!int |-\n  {BIG}\n"))
                .unwrap()
                .unwrap(),
            crate::Parser::parse_str(&format!("k: !!int >-\n  {BIG}\n"))
                .unwrap()
                .unwrap(),
        ];
        let out = Emitter::emit_all(&docs).unwrap();
        assert_eq!(out.matches(BIG).count(), 2, "{out}");
    }

    fn emit_both(doc: &Value) -> [String; 2] {
        [None, Some(true)].map(|flow| {
            let config = EmitterConfig::new().with_default_flow_style(flow);
            Emitter::emit_str_with_config(doc, &config).unwrap()
        })
    }

    #[test]
    fn emit_nested_block_scalar_key_and_negative_big_int() {
        for input in [
            format!("a:\n  b:\n    ? !!int >-\n      {BIG}\n    : v\n"),
            format!("--- !!int |-\n  -{BIG}\n"),
        ] {
            let doc = crate::Parser::parse_str(&input).unwrap().unwrap();
            roundtrip_all_styles(&doc);
        }
    }

    #[test]
    fn emit_all_flow_with_block_scalar_big_ints() {
        let docs = [
            crate::Parser::parse_str(&format!("!!int |-\n  {BIG}\n"))
                .unwrap()
                .unwrap(),
            crate::Parser::parse_str(&format!("k: !!int >-\n  {BIG}\n"))
                .unwrap()
                .unwrap(),
        ];
        let config = EmitterConfig::new().with_default_flow_style(Some(true));
        let out = Emitter::emit_all_with_config(&docs, &config).unwrap();
        assert_eq!(out.matches(BIG).count(), 2, "{out}");
    }

    const NON_PRINTABLE: &str = "q\"u\\o\x7Ft\u{86}e\u{FFFE}d\u{FFFF}";

    #[test]
    fn non_printable_strings_round_trip_in_every_position() {
        let text = string_key(NON_PRINTABLE);
        let mut keyed = Mapping::new();
        keyed.insert(text.clone(), text.clone());
        let docs = [
            text.clone(),
            Value::Sequence(vec![text]),
            Value::Mapping(keyed),
        ];
        for flow in [None, Some(true)] {
            let config = EmitterConfig::new().with_default_flow_style(flow);
            for doc in &docs {
                let out = Emitter::emit_str_with_config(doc, &config).unwrap();
                assert!(
                    !out.contains(['\u{FFFE}', '\u{FFFF}', '\u{7F}', '\u{86}']),
                    "{out:?}"
                );
                let back = crate::Parser::parse_str(&out).unwrap().unwrap();
                assert_eq!(&back, doc, "flow={flow:?}: {out:?}");
            }
        }
    }

    #[test]
    fn float_spelling_is_written_by_both_emitters() {
        let float = |t: &str| Value::Float(Float::parse(t).unwrap());
        let mut map = Mapping::new();
        map.insert(string_key("a"), float("1.0E5"));
        map.insert(string_key("b"), float("0.10"));
        map.insert(string_key("c"), Value::Float(Float::new(100_000.0)));
        let doc = Value::Mapping(map);
        assert_eq!(
            Emitter::emit_str(&doc).unwrap(),
            "a: 1.0E5\nb: 0.10\nc: 100000.0\n"
        );
        let flow = EmitterConfig::new().with_default_flow_style(Some(true));
        assert_eq!(
            Emitter::emit_str_with_config(&doc, &flow).unwrap(),
            "{a: 1.0E5, b: 0.10, c: 100000.0}\n"
        );
    }

    #[test]
    fn special_floats_are_core_schema_in_values_and_keys() {
        let floats = [
            Value::Float(Float::new(f64::NAN)),
            Value::Float(Float::new(f64::INFINITY)),
            Value::Float(Float::new(f64::NEG_INFINITY)),
        ];
        for flow in [None, Some(true)] {
            let config = EmitterConfig::new().with_default_flow_style(flow);
            for float in &floats {
                let mut map = Mapping::new();
                map.insert(float.clone(), float.clone());
                let doc = Value::Mapping(map);
                let out = Emitter::emit_str_with_config(&doc, &config).unwrap();
                let back = crate::Parser::parse_str(&out).unwrap().unwrap();
                assert_eq!(back, doc, "flow={flow:?}: {out:?}");
            }
        }
    }

    #[test]
    fn nesting_beyond_the_emit_depth_is_an_error_not_a_stack_overflow() {
        let mut doc = Value::Int(1);
        for _ in 0..=MaxDepth::DEFAULT.get() {
            doc = Value::Sequence(vec![doc]);
        }
        for flow in [None, Some(true)] {
            let config = EmitterConfig::new().with_default_flow_style(flow);
            let err = Emitter::emit_str_with_config(&doc, &config).unwrap_err();
            assert!(
                matches!(err, EmitError::DepthLimitExceeded { limit: 256 }),
                "flow={flow:?}: {err:?}"
            );
        }
        let mut ok = Value::Int(1);
        for _ in 0..MaxDepth::DEFAULT.get() {
            ok = Value::Sequence(vec![ok]);
        }
        assert!(Emitter::emit_str(&ok).is_ok());
    }

    #[test]
    fn collection_keys_fail_in_flow_style_only() {
        let mut map = Mapping::new();
        map.insert(Value::Sequence(vec![Value::Int(1)]), Value::Int(2));
        let doc = Value::Mapping(map);
        assert!(Emitter::emit_str(&doc).is_ok());
        let flow = EmitterConfig::new().with_default_flow_style(Some(true));
        assert!(matches!(
            Emitter::emit_str_with_config(&doc, &flow),
            Err(EmitError::ComplexFlowKey)
        ));
    }

    #[test]
    fn emit_set_round_trips_in_block_and_flow() {
        for yaml in [
            "!!set {a, b}",
            "m: !!set {a}",
            "!!set {1, a}",
            "outer:\n  inner: !!set {x, y}\nlist: [!!set {k}]",
            "list: [!!set {}]",
            "m: !!omap [a: 1, b: 2]",
            "k: !!str 123",
            "!!set {null, true, 1.5, '1'}",
        ] {
            let doc = crate::Parser::parse_str(yaml).unwrap().unwrap();
            for out in emit_both(&doc) {
                assert_eq!(out.contains("!!set"), yaml.contains("!!set"), "{out}");
                let back = crate::Parser::parse_str(&out).unwrap().unwrap();
                assert_eq!(back, doc, "{yaml} -> {out}");
            }
        }
    }

    #[test]
    fn emit_block_tagged_collection_has_no_trailing_space() {
        let doc = crate::Parser::parse_str("k: !!set {a, b}\nl:\n  - !!set {c}\ns: \"x !t \\n\"")
            .unwrap()
            .unwrap();
        let out = Emitter::emit_str(&doc).unwrap();
        assert!(out.lines().all(|l| !l.ends_with(' ')), "{out:?}");
        assert_eq!(crate::Parser::parse_str(&out).unwrap().unwrap(), doc);
    }

    #[test]
    fn emit_literal_block_keeps_trailing_space_after_bang_word() {
        let doc = crate::Parser::parse_str("k: \"a !b \\nc\\n\"")
            .unwrap()
            .unwrap();
        let config = EmitterConfig::new().with_multiline_strings(true);
        let out = Emitter::emit_str_with_config(&doc, &config).unwrap();
        assert_eq!(
            crate::Parser::parse_str(&out).unwrap().unwrap(),
            doc,
            "{out:?}"
        );
    }

    #[test]
    fn emit_set_members_that_are_collections_fail_in_flow_only() {
        let mut set = Set::new();
        set.insert(Value::Sequence(vec![Value::Int(1)]));
        let doc = Value::Set(set);
        assert!(Emitter::emit_str(&doc).is_ok());
        let flow = EmitterConfig::new().with_default_flow_style(Some(true));
        assert!(matches!(
            Emitter::emit_str_with_config(&doc, &flow),
            Err(EmitError::ComplexFlowKey)
        ));
    }

    #[test]
    fn emit_set_as_a_key_or_member_is_an_error() {
        let set = |member: &str| {
            let mut set = Set::new();
            set.insert(Value::String(member.into()));
            Value::Set(set)
        };
        let mut as_key = Mapping::new();
        as_key.insert(set("a"), Value::Int(1));
        let mut nested = Set::new();
        nested.insert(set("a"));
        for doc in [Value::Mapping(as_key), Value::Set(nested)] {
            for flow in [None, Some(true)] {
                let config = EmitterConfig::new().with_default_flow_style(flow);
                assert!(
                    matches!(
                        Emitter::emit_str_with_config(&doc, &config),
                        Err(EmitError::SetAsKey | EmitError::ComplexFlowKey)
                    ),
                    "{flow:?}"
                );
            }
        }
    }
}
