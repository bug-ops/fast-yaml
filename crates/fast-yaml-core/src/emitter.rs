use std::borrow::Cow;
use std::fmt::Write as _;

use crate::error::{EmitError, EmitResult, from_saphyr};
use crate::parser::scalar_to_value;
use crate::scalar::{ResolvedScalar, resolve_scalar};
use crate::streaming::{
    effective_style, is_unsafe_plain, write_double_quoted, write_single_quoted,
};
use crate::value::Value;
use memchr::memmem;
use saphyr::{ScalarOwned, YamlEmitter};
use saphyr_parser::{ScalarStyle, Tag};

/// Smallest supported indentation width.
pub(crate) const MIN_INDENT: usize = 1;

/// Largest supported indentation width (a block scalar indentation indicator is one digit).
pub(crate) const MAX_INDENT: usize = 9;

/// Configuration for YAML emission.
///
/// Controls formatting, style, and output options when serializing YAML.
#[derive(Debug, Clone)]
pub struct EmitterConfig {
    /// Indentation width in spaces (default: 2).
    ///
    /// Controls the number of spaces used for each indentation level.
    /// Valid range: 1-9 (values outside this range will be clamped).
    ///
    /// Note: saphyr currently uses fixed 2-space indentation.
    /// This parameter is accepted for `PyYAML` API compatibility but
    /// may require post-processing to fully support custom values.
    pub indent: usize,

    /// Maximum line width for wrapping (default: 80).
    ///
    /// When lines exceed this width, the emitter will attempt to wrap them.
    /// Valid range: 20-1000 (values outside this range will be clamped).
    ///
    /// Note: saphyr has limited control over line wrapping.
    /// This parameter is accepted for `PyYAML` API compatibility.
    pub width: usize,

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
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            indent: 2,
            width: 80,
            default_flow_style: None,
            explicit_start: false,
            compact: true,
            multiline_strings: false,
        }
    }
}

impl EmitterConfig {
    /// Create a new emitter configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set indentation width (clamped to 1-9).
    #[must_use]
    pub fn with_indent(mut self, indent: usize) -> Self {
        self.indent = indent.clamp(MIN_INDENT, MAX_INDENT);
        self
    }

    /// Set line width (clamped to 20-1000).
    #[must_use]
    pub fn with_width(mut self, width: usize) -> Self {
        self.width = width.clamp(20, 1000);
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
/// Wraps saphyr's `YamlEmitter` to provide a consistent API.
#[derive(Debug)]
pub struct Emitter;

impl Emitter {
    /// Emit a single YAML document to a string with configuration.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Format` if the value cannot be serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, EmitterConfig, ScalarOwned, Value};
    ///
    /// let value = Value::Value(ScalarOwned::String("test".to_string()));
    /// let config = EmitterConfig::new().with_explicit_start(true);
    /// let yaml = Emitter::emit_str_with_config(&value, &config)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn emit_str_with_config(value: &Value, config: &EmitterConfig) -> EmitResult<String> {
        let value = &*prepare_for_saphyr(value);
        // When flow style is requested, use the custom path that renders {k: v} / [a, b].
        if config.default_flow_style == Some(true) {
            let raw = Self::emit_flow(value)?;
            let mut output = Self::apply_formatting(raw, config);
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            return Ok(output);
        }

        let estimated_size = Self::estimate_output_size(value);
        let mut output = String::with_capacity(estimated_size);
        {
            let mut emitter = YamlEmitter::new(&mut output);

            // Apply saphyr native configuration
            emitter.compact(config.compact);
            emitter.multiline_strings(config.multiline_strings);

            // Convert YamlOwned to Yaml for emission
            let yaml_borrowed: saphyr::Yaml = value.into();
            emitter.dump(&yaml_borrowed).map_err(from_saphyr)?;
        }

        // Apply post-processing for configuration options
        output = Self::apply_formatting(output, config);

        // Ensure output always ends with a newline
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }

        Ok(output)
    }

    /// Estimate output size based on input value structure.
    fn estimate_output_size(value: &Value) -> usize {
        Self::estimate_value_size(value)
    }

    fn estimate_value_size(value: &Value) -> usize {
        match value {
            Value::Value(scalar) => Self::estimate_scalar_size(scalar),
            Value::Sequence(seq) => {
                // "- " prefix (2) + newline (1) per item + recursive content
                seq.iter().map(|v| 3 + Self::estimate_value_size(v)).sum()
            }
            Value::Mapping(map) => {
                // "key: " (~10) + newline (1) + recursive content
                map.iter()
                    .map(|(k, v)| 11 + Self::estimate_value_size(k) + Self::estimate_value_size(v))
                    .sum()
            }
            Value::Representation(s, _, _) => s.len() + 2,
            Value::Tagged(_, inner) => 10 + Self::estimate_value_size(inner),
            Value::Alias(_) => 10,
            Value::BadValue => 4,
        }
    }

    fn estimate_scalar_size(scalar: &ScalarOwned) -> usize {
        match scalar {
            ScalarOwned::Null => 4,       // "null"
            ScalarOwned::Boolean(_) => 5, // "false"
            ScalarOwned::Integer(i) => {
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
            ScalarOwned::FloatingPoint(_) => 20, // Conservative estimate
            ScalarOwned::String(s) => s.len() + 2, // Possible quotes
        }
    }

    /// Emit a single YAML document to a string with default configuration.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Format` if the value cannot be serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, ScalarOwned, Value};
    ///
    /// let value = Value::Value(ScalarOwned::String("test".to_string()));
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
    /// use fast_yaml_core::{Emitter, EmitterConfig, ScalarOwned, Value};
    ///
    /// let docs = vec![
    ///     Value::Value(ScalarOwned::String("first".to_string())),
    ///     Value::Value(ScalarOwned::String("second".to_string())),
    /// ];
    /// let config = EmitterConfig::new().with_explicit_start(true);
    /// let yaml = Emitter::emit_all_with_config(&docs, &config)?;
    /// assert!(yaml.contains("---"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn emit_all_with_config(values: &[Value], config: &EmitterConfig) -> EmitResult<String> {
        // Pre-calculate total estimated size for all documents
        let total_size: usize =
            values.iter().map(Self::estimate_output_size).sum::<usize>() + values.len() * 5; // Account for "---\n" separators

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
    /// use fast_yaml_core::{Emitter, ScalarOwned, Value};
    ///
    /// let docs = vec![
    ///     Value::Value(ScalarOwned::String("first".to_string())),
    ///     Value::Value(ScalarOwned::String("second".to_string())),
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

        // Fix special float values for YAML 1.2 Core Schema compliance
        // saphyr outputs "inf"/"-inf"/"NaN", but YAML 1.2 requires ".inf"/"-.inf"/".nan"
        output = Self::fix_special_floats(&output);

        // Re-indent when caller requests a width other than saphyr's fixed 2 spaces.
        if config.indent != 2 {
            output = Self::reindent(&output, config.indent);
        }

        output
    }

    /// Fix special float values for YAML 1.2 Core Schema compliance.
    ///
    /// Converts saphyr's output format to YAML 1.2 compliant format:
    /// - `inf` → `.inf`
    /// - `-inf` → `-.inf`
    /// - `NaN` → `.nan`
    fn fix_special_floats(output: &str) -> String {
        if !Self::might_contain_special_floats(output) {
            return output.to_string();
        }

        // Slow path: line-by-line transformation
        Self::fix_special_floats_slow(output)
    }

    /// Quick check if output might contain special float patterns.
    /// Uses SIMD-accelerated memchr for speed - no regex or allocation.
    #[inline]
    fn might_contain_special_floats(output: &str) -> bool {
        let bytes = output.as_bytes();

        // Use SIMD-accelerated memmem for fast substring search
        // These are the only special float indicators in saphyr output
        memmem::find(bytes, b"inf").is_some() || memmem::find(bytes, b"NaN").is_some()
    }

    /// Slow path for `fix_special_floats`: processes line-by-line.
    /// Pre-allocates output buffer to avoid reallocations.
    fn fix_special_floats_slow(output: &str) -> String {
        // Pre-allocate output (same size as input since patterns are similar length)
        let mut result = String::with_capacity(output.len());

        for (i, line) in output.lines().enumerate() {
            if i > 0 {
                result.push('\n');
            }

            // Check if line ends with special float value (with optional whitespace)
            let trimmed = line.trim_end();
            if let Some(prefix) = trimmed.strip_suffix("inf") {
                // Check if it's "-inf" or standalone "inf"
                if let Some(before_minus) = prefix.strip_suffix('-') {
                    // Already has minus, check if it's at value position
                    if Self::is_value_position(before_minus) {
                        result.push_str(before_minus);
                        result.push_str("-.inf");
                        continue;
                    }
                } else if Self::is_value_position(prefix) {
                    result.push_str(prefix);
                    result.push_str(".inf");
                    continue;
                }
            } else if let Some(prefix) = trimmed.strip_suffix("NaN")
                && Self::is_value_position(prefix)
            {
                result.push_str(prefix);
                result.push_str(".nan");
                continue;
            }
            result.push_str(line);
        }

        result
    }

    /// Check if the prefix indicates this is a value position (after `: ` or start of line).
    fn is_value_position(prefix: &str) -> bool {
        prefix.is_empty()
            || prefix.ends_with(": ")
            || prefix.ends_with("- ")
            || prefix.ends_with('\n')
    }

    /// Format a YAML string with configuration.
    ///
    /// Uses the streaming formatter, which preserves scalar styles, explicit tags,
    /// anchors and aliases.
    ///
    /// Block scalar styles (`|` literal and `>` folded) are preserved in the output.
    /// `%YAML` and `%TAG` directives are preserved before the document that declared them.
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
        let input = crate::parser::strip_bom(input);
        #[cfg(feature = "arena")]
        let formatted = crate::streaming::format_streaming_arena(input, config)?;
        #[cfg(not(feature = "arena"))]
        let formatted = crate::streaming::format_streaming(input, config)?;
        Ok(formatted)
    }

    /// Emit a scalar key as an inline string (no trailing newline).
    fn emit_scalar_inline(value: &Value) -> EmitResult<String> {
        match value {
            Value::Representation(
                s,
                style @ (ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted),
                _,
            ) => Ok(match effective_style(s, *style) {
                ScalarStyle::SingleQuoted => {
                    let mut out = String::with_capacity(s.len() + 2);
                    write_single_quoted(&mut out, s);
                    out
                }
                _ => double_quoted(s),
            }),
            Value::Representation(s, ScalarStyle::Plain, _) => Ok(if flow_key_is_plain_safe(s) {
                s.clone()
            } else {
                double_quoted(s)
            }),
            Value::Representation(s, _, _) => Ok(if flow_key_needs_quotes(s) {
                double_quoted(s)
            } else {
                s.clone()
            }),
            Value::Value(scalar) => match scalar {
                ScalarOwned::Null => Ok("null".to_string()),
                ScalarOwned::Boolean(b) => Ok(if *b { "true" } else { "false" }.to_string()),
                ScalarOwned::Integer(i) => Ok(i.to_string()),
                ScalarOwned::FloatingPoint(f) => {
                    let s = f.to_string();
                    // Ensure the output is recognisable as a float (YAML Core Schema).
                    // Rust formats e.g. `1.0` as `"1"` and `1.23e10` as `"12300000000"`.
                    // Append `.0` when the string contains no decimal point or exponent and
                    // is not a special value (inf / NaN handled by fix_special_floats).
                    if s.contains('.')
                        || s.contains('e')
                        || s.contains('E')
                        || s.eq_ignore_ascii_case("inf")
                        || s.eq_ignore_ascii_case("-inf")
                        || s.eq_ignore_ascii_case("nan")
                    {
                        Ok(s)
                    } else {
                        Ok(format!("{s}.0"))
                    }
                }
                ScalarOwned::String(s) => Ok(if flow_key_needs_quotes(s) {
                    double_quoted(s)
                } else {
                    s.clone()
                }),
            },
            _ => Err(EmitError::UnsupportedType(
                "complex key not supported".to_string(),
            )),
        }
    }

    /// Emit any non-block-scalar value as an inline string (no trailing newline).
    fn emit_value_inline(value: &Value) -> EmitResult<String> {
        let mut out = String::new();
        {
            let mut emitter = YamlEmitter::new(&mut out);
            emitter.compact(true);
            let yaml: saphyr::Yaml = value.into();
            emitter.dump(&yaml).map_err(from_saphyr)?;
        }
        // saphyr emits "---\nvalue\n" — strip markers
        let trimmed = out
            .strip_prefix("---\n")
            .unwrap_or(&out)
            .trim_end_matches('\n');
        Ok(trimmed.to_string())
    }

    /// Emit a value in YAML flow style: mappings as `{k: v}`, sequences as `[a, b]`.
    ///
    /// Scalar values are rendered inline.  Nested collections are also rendered
    /// in flow style recursively.
    ///
    /// Returns a string without a leading `---\n` marker and without a trailing newline.
    fn emit_flow(value: &Value) -> EmitResult<String> {
        match value {
            Value::Mapping(map) => {
                let mut out = String::from("{");
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    let key_str = Self::emit_scalar_inline(k)?;
                    let val_str = Self::emit_flow(v)?;
                    write!(out, "{key_str}: {val_str}")?;
                }
                out.push('}');
                Ok(out)
            }
            Value::Sequence(seq) => {
                let mut out = String::from("[");
                for (i, item) in seq.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&Self::emit_flow(item)?);
                }
                out.push(']');
                Ok(out)
            }
            // Scalars: use the inline scalar renderer (no trailing newline)
            _ => Self::emit_value_inline(value),
        }
    }

    /// Re-indent saphyr output from 2-space indentation to `target` spaces per level.
    ///
    /// Lines that form block scalar bodies (content under `|` / `>`) retain their
    /// relative spacing; only the base indent level is rescaled.
    ///
    /// `---` / `...` markers and directive lines (`%YAML`, `%TAG`) are left unchanged.
    fn reindent(output: &str, target: usize) -> String {
        let mut result = String::with_capacity(output.len());
        let mut in_block_scalar = false;
        let mut block_scalar_base_indent: usize = 0;

        for (i, line) in output.lines().enumerate() {
            if i > 0 {
                result.push('\n');
            }

            // Directives and document markers: never re-indent.
            let trimmed = line.trim_start();
            if trimmed.starts_with("---")
                || trimmed.starts_with("...")
                || trimmed.starts_with("%YAML")
                || trimmed.starts_with("%TAG")
            {
                in_block_scalar = false;
                result.push_str(line);
                continue;
            }

            let leading = line.len() - trimmed.len();
            let level = leading / 2; // saphyr always uses 2-space indent

            if in_block_scalar {
                // Inside a block scalar body: keep lines that are deeper than the
                // mapping/sequence key that introduced the scalar.
                if leading > block_scalar_base_indent {
                    // Rescale: base_level * target + (extra spaces beyond base)
                    let base_level = block_scalar_base_indent / 2;
                    let extra = leading - block_scalar_base_indent;
                    let new_leading = base_level * target + extra;
                    let spaces = " ".repeat(new_leading);
                    result.push_str(&spaces);
                    result.push_str(trimmed);
                    continue;
                }
                // Dedented back out of the block scalar
                in_block_scalar = false;
            }

            // Detect start of block scalar: line ends with `|` or `>` (with optional
            // chomping indicator and trailing whitespace).
            let value_part = trimmed.trim_end_matches(|c: char| c.is_whitespace());
            let last_nonws = value_part.trim_start_matches(|c: char| c != '|' && c != '>');
            if last_nonws.starts_with('|') || last_nonws.starts_with('>') {
                in_block_scalar = true;
                block_scalar_base_indent = leading;
            }

            let new_leading = level * target;
            let spaces = " ".repeat(new_leading);
            result.push_str(&spaces);
            result.push_str(trimmed);
        }

        // Preserve trailing newline if present
        if output.ends_with('\n') {
            result.push('\n');
        }

        result
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
        || s.chars().any(char::is_control)
        || is_unsafe_plain(s))
}

/// Whether the plain scalar `s` would be resolved to something other than a string.
fn reads_as_non_string(s: &str) -> bool {
    !matches!(
        resolve_scalar(s, ScalarStyle::Plain, None),
        ResolvedScalar::Str(_)
    )
}

/// Whether `value` holds a node saphyr would mis-emit or panic on.
///
/// saphyr's own quoting check has a separate, narrower float grammar, so strings such as
/// `+.inf` would be written plain and change type on re-read. Its emitter also hits
/// `todo!()` on literal/folded representations and writes core-schema tags as
/// `tag:yaml.org,2002:!int`.
fn needs_saphyr_rewrite(value: &Value) -> bool {
    match value {
        Value::Value(ScalarOwned::String(s)) => reads_as_non_string(s),
        Value::Representation(_, style, tag) => {
            matches!(style, ScalarStyle::Literal | ScalarStyle::Folded)
                || tag.as_ref().is_some_and(Tag::is_yaml_core_schema)
        }
        Value::Tagged(_, inner) => needs_saphyr_rewrite(inner),
        Value::Sequence(seq) => seq.iter().any(needs_saphyr_rewrite),
        Value::Mapping(map) => map
            .iter()
            .any(|(k, v)| needs_saphyr_rewrite(k) || needs_saphyr_rewrite(v)),
        _ => false,
    }
}

/// Rewrites a parsed representation into a form saphyr can emit and read back unchanged.
///
/// The scalar is resolved to its typed value; a big integer stays a digit-only plain
/// representation and a string is left to saphyr to quote. A non-core tag is kept, a core-schema
/// tag is dropped because its type is already carried by the resolved value.
fn rewrite_representation(s: &str, style: ScalarStyle, tag: Option<&Tag>) -> Value {
    let custom_tag = tag.filter(|t| !t.is_yaml_core_schema()).cloned();
    match resolve_scalar(s, style, tag) {
        ResolvedScalar::BigInt(_) => {
            Value::Representation(s.to_owned(), ScalarStyle::Plain, custom_tag)
        }
        ResolvedScalar::Str(_) => {
            let string = Value::Value(ScalarOwned::String(s.to_owned()));
            match custom_tag {
                Some(t) => Value::Tagged(t, Box::new(string)),
                None => string,
            }
        }
        other => scalar_to_value(other),
    }
}

fn rewrite_for_saphyr(value: &Value) -> Value {
    match value {
        Value::Value(ScalarOwned::String(s)) if reads_as_non_string(s) => {
            Value::Representation(s.clone(), ScalarStyle::DoubleQuoted, None)
        }
        Value::Representation(s, style, tag) if needs_saphyr_rewrite(value) => {
            rewrite_representation(s, *style, tag.as_ref())
        }
        Value::Tagged(tag, inner) => {
            Value::Tagged(tag.clone(), Box::new(rewrite_for_saphyr(inner)))
        }
        Value::Sequence(seq) => Value::Sequence(seq.iter().map(rewrite_for_saphyr).collect()),
        Value::Mapping(map) => Value::Mapping(
            map.iter()
                .map(|(k, v)| (rewrite_for_saphyr(k), rewrite_for_saphyr(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Returns `value` rewritten so that saphyr's emitter neither panics nor changes its meaning.
fn prepare_for_saphyr(value: &Value) -> Cow<'_, Value> {
    if needs_saphyr_rewrite(value) {
        Cow::Owned(rewrite_for_saphyr(value))
    } else {
        Cow::Borrowed(value)
    }
}

/// Build a block scalar header: indicator, optional indentation digit, chomp suffix.
///
/// The digit is emitted when the first non-empty line starts with a space, since
/// the parser would otherwise auto-detect the indentation from that leading space
/// and drop it from the value. `indent_width` is the content indent relative to
/// the parent node and must be in `1..=9` (guaranteed by `EmitterConfig`).
/// Chomping is `+` (keep) for trailing blank lines or a lone newline, `-` (strip)
/// without a trailing newline, and clip (nothing) otherwise.
pub(crate) fn block_scalar_header(indicator: char, value: &str, indent_width: usize) -> String {
    let mut header = String::from(indicator);
    let leading_space = value
        .lines()
        .find(|line| !line.is_empty())
        .is_some_and(|line| line.starts_with(' '));
    if leading_space {
        header.extend(
            u32::try_from(indent_width)
                .ok()
                .and_then(|d| char::from_digit(d, 10)),
        );
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
    use ordered_float::OrderedFloat;
    use saphyr::{MappingOwned, ScalarOwned};

    #[test]
    fn test_emit_str_string() {
        let value = Value::Value(ScalarOwned::String("test".to_string()));
        let result = Emitter::emit_str(&value).unwrap();
        assert!(result.contains("test"));
    }

    #[test]
    fn test_emit_str_integer() {
        let value = Value::Value(ScalarOwned::Integer(42));
        let result = Emitter::emit_str(&value).unwrap();
        assert!(result.contains("42"));
    }

    #[test]
    fn test_emit_all_multiple() {
        let values = vec![
            Value::Value(ScalarOwned::String("first".to_string())),
            Value::Value(ScalarOwned::String("second".to_string())),
        ];
        let result = Emitter::emit_all(&values).unwrap();
        assert!(result.contains("first"));
        assert!(result.contains("second"));
        assert!(result.contains("---"));
    }

    #[test]
    fn test_emit_all_single() {
        let values = vec![Value::Value(ScalarOwned::String("only".to_string()))];
        let result = Emitter::emit_all(&values).unwrap();
        assert!(result.contains("only"));
        assert!(!result.starts_with("---"));
    }

    #[test]
    fn test_emitter_config_default() {
        let config = EmitterConfig::default();
        assert_eq!(config.indent, 2);
        assert_eq!(config.width, 80);
        assert_eq!(config.default_flow_style, None);
        assert!(!config.explicit_start);
        assert!(config.compact);
        assert!(!config.multiline_strings);
    }

    #[test]
    fn test_emitter_config_builder() {
        let config = EmitterConfig::new()
            .with_indent(4)
            .with_width(120)
            .with_explicit_start(true)
            .with_compact(false);

        assert_eq!(config.indent, 4);
        assert_eq!(config.width, 120);
        assert!(config.explicit_start);
        assert!(!config.compact);
    }

    #[test]
    fn test_emitter_config_clamp_indent() {
        let config = EmitterConfig::new().with_indent(100);
        assert_eq!(config.indent, 9);

        let config = EmitterConfig::new().with_indent(0);
        assert_eq!(config.indent, 1);
    }

    #[test]
    fn test_emitter_config_clamp_width() {
        let config = EmitterConfig::new().with_width(10);
        assert_eq!(config.width, 20);

        let config = EmitterConfig::new().with_width(2000);
        assert_eq!(config.width, 1000);
    }

    #[test]
    fn test_emit_with_explicit_start() {
        let value = Value::Value(ScalarOwned::String("test".to_string()));
        let config = EmitterConfig::new().with_explicit_start(true);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(result.starts_with("---"));
    }

    #[test]
    fn test_emit_without_explicit_start() {
        let value = Value::Value(ScalarOwned::String("test".to_string()));
        let config = EmitterConfig::new().with_explicit_start(false);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(!result.starts_with("---"));
    }

    #[test]
    fn test_emit_all_with_explicit_start() {
        let values = vec![
            Value::Value(ScalarOwned::String("first".to_string())),
            Value::Value(ScalarOwned::String("second".to_string())),
        ];
        let config = EmitterConfig::new().with_explicit_start(true);
        let result = Emitter::emit_all_with_config(&values, &config).unwrap();
        assert!(result.starts_with("---"));
        assert_eq!(result.matches("---").count(), 2);
    }

    #[test]
    fn test_emit_with_compact_false() {
        let value = Value::Sequence(vec![
            Value::Value(ScalarOwned::Integer(1)),
            Value::Value(ScalarOwned::Integer(2)),
        ]);
        let config = EmitterConfig::new().with_compact(false);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        // Should contain formatting (exact format depends on saphyr)
        assert!(result.contains('1') && result.contains('2'));
    }

    #[test]
    fn test_emit_with_multiline_strings() {
        let value = Value::Value(ScalarOwned::String("line1\nline2".to_string()));
        let config = EmitterConfig::new().with_multiline_strings(true);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        // Should use literal block scalar notation (|)
        assert!(result.contains("line1") && result.contains("line2"));
    }

    #[test]
    fn test_estimate_scalar_size_all_types() {
        // Test Null
        let null_size = Emitter::estimate_scalar_size(&ScalarOwned::Null);
        assert_eq!(null_size, 4); // "null"

        // Test Boolean
        let bool_size = Emitter::estimate_scalar_size(&ScalarOwned::Boolean(true));
        assert_eq!(bool_size, 5); // "false" (conservative estimate)

        // Test Integer - edge cases
        // Zero case (special handling)
        let zero_size = Emitter::estimate_scalar_size(&ScalarOwned::Integer(0));
        assert_eq!(zero_size, 1);

        // Single digit
        let single_digit = Emitter::estimate_scalar_size(&ScalarOwned::Integer(5));
        assert!(single_digit >= 1);

        // Multi-digit positive
        let multi_digit = Emitter::estimate_scalar_size(&ScalarOwned::Integer(12345));
        assert!(multi_digit >= 5);

        // Negative number
        let negative = Emitter::estimate_scalar_size(&ScalarOwned::Integer(-42));
        assert!(negative >= 2); // "-" + digits

        // Test Float
        let float_size =
            Emitter::estimate_scalar_size(&ScalarOwned::FloatingPoint(OrderedFloat(1.23456)));
        assert_eq!(float_size, 20); // Conservative estimate

        // Test String
        let string_size = Emitter::estimate_scalar_size(&ScalarOwned::String("hello".to_string()));
        assert_eq!(string_size, 7); // 5 chars + 2 for possible quotes
    }

    #[test]
    fn test_estimate_value_size_mapping() {
        use saphyr::MappingOwned;

        // Create a mapping with string keys and integer values
        let mut map = MappingOwned::new();
        map.insert(
            Value::Value(ScalarOwned::String("key1".to_string())),
            Value::Value(ScalarOwned::Integer(100)),
        );
        map.insert(
            Value::Value(ScalarOwned::String("key2".to_string())),
            Value::Value(ScalarOwned::Integer(200)),
        );

        let mapping = Value::Mapping(map);
        let size = Emitter::estimate_value_size(&mapping);

        // Should be > 0 and account for both key-value pairs
        // Each pair has ~11 base overhead + key size + value size
        assert!(
            size > 20,
            "Mapping estimate should be significant: got {size}"
        );

        // Test nested mapping
        let mut nested_map = MappingOwned::new();
        nested_map.insert(
            Value::Value(ScalarOwned::String("outer".to_string())),
            mapping,
        );

        let nested_size = Emitter::estimate_value_size(&Value::Mapping(nested_map));
        assert!(
            nested_size > size,
            "Nested mapping should have larger estimate"
        );
    }

    #[test]
    fn test_might_contain_special_floats_positive() {
        // Direct "inf" patterns
        assert!(Emitter::might_contain_special_floats("inf"));
        assert!(Emitter::might_contain_special_floats("key: inf"));
        assert!(Emitter::might_contain_special_floats("-inf"));
        assert!(Emitter::might_contain_special_floats("key: -inf"));
        assert!(Emitter::might_contain_special_floats("- inf\n- -inf"));

        // Direct "NaN" patterns
        assert!(Emitter::might_contain_special_floats("NaN"));
        assert!(Emitter::might_contain_special_floats("key: NaN"));
        assert!(Emitter::might_contain_special_floats(
            "values:\n  - NaN\n  - inf"
        ));

        // Mixed content
        assert!(Emitter::might_contain_special_floats(
            "---\npi: 3.14\nspecial: inf\n"
        ));
    }

    #[test]
    fn test_might_contain_special_floats_false_positives() {
        // Words containing "inf" substring that will trigger the fast-path check
        // (but won't be converted because they're not in value positions)
        assert!(
            Emitter::might_contain_special_floats("information"),
            "'information' contains 'inf' substring"
        );
        assert!(
            Emitter::might_contain_special_floats("infinity"),
            "'infinity' contains 'inf' substring"
        );
        assert!(
            Emitter::might_contain_special_floats("infinite"),
            "'infinite' contains 'inf' substring"
        );
        assert!(
            Emitter::might_contain_special_floats("reinforce"),
            "'reinforce' contains 'inf' substring"
        );

        // Strings that should NOT trigger the check (no "inf" or "NaN" substring)
        assert!(!Emitter::might_contain_special_floats("hello world"));
        assert!(!Emitter::might_contain_special_floats("key: value"));
        assert!(!Emitter::might_contain_special_floats("number: 42"));
        assert!(!Emitter::might_contain_special_floats("pi: 3.14159"));
        assert!(!Emitter::might_contain_special_floats("config")); // "config" does NOT contain "inf"
        assert!(!Emitter::might_contain_special_floats("nan")); // lowercase "nan" != "NaN"
        assert!(!Emitter::might_contain_special_floats("INF")); // uppercase "INF" != "inf"
    }

    #[test]
    fn test_fix_special_floats_inf() {
        // Test standalone inf conversion
        let result = Emitter::fix_special_floats("inf");
        assert_eq!(result, ".inf");

        // Test inf in a mapping value position
        let result = Emitter::fix_special_floats("key: inf");
        assert_eq!(result, "key: .inf");

        // Test -inf conversion
        let result = Emitter::fix_special_floats("-inf");
        assert_eq!(result, "-.inf");

        // Test -inf in a mapping value position
        let result = Emitter::fix_special_floats("key: -inf");
        assert_eq!(result, "key: -.inf");

        // Test inf in a sequence
        let result = Emitter::fix_special_floats("- inf");
        assert_eq!(result, "- .inf");

        // Test -inf in a sequence
        let result = Emitter::fix_special_floats("- -inf");
        assert_eq!(result, "- -.inf");

        // Test mixed document with multiple inf values
        let input = "positive: inf\nnegative: -inf\nlist:\n  - inf\n  - -inf";
        let result = Emitter::fix_special_floats(input);
        assert!(result.contains("positive: .inf"));
        assert!(result.contains("negative: -.inf"));
        assert!(result.contains("- .inf"));
        assert!(result.contains("- -.inf"));
    }

    #[test]
    fn test_fix_special_floats_nan() {
        // Test standalone NaN conversion
        let result = Emitter::fix_special_floats("NaN");
        assert_eq!(result, ".nan");

        // Test NaN in a mapping value position
        let result = Emitter::fix_special_floats("value: NaN");
        assert_eq!(result, "value: .nan");

        // Test NaN in a sequence
        let result = Emitter::fix_special_floats("- NaN");
        assert_eq!(result, "- .nan");

        // Test document with multiple NaN values
        let input = "nan_value: NaN\nlist:\n  - NaN";
        let result = Emitter::fix_special_floats(input);
        assert!(result.contains("nan_value: .nan"));
        assert!(result.contains("- .nan"));

        // Test that strings containing "NaN" as part of word are not converted
        // (this relies on is_value_position check)
        let result = Emitter::fix_special_floats("name: BaNaNa");
        assert_eq!(result, "name: BaNaNa", "BaNaNa should not be modified");

        // Test mixed special floats
        let input = "inf_val: inf\nnan_val: NaN\nneg_inf: -inf";
        let result = Emitter::fix_special_floats(input);
        assert!(result.contains("inf_val: .inf"));
        assert!(result.contains("nan_val: .nan"));
        assert!(result.contains("neg_inf: -.inf"));
    }

    #[test]
    fn test_estimate_value_size_sequence() {
        let seq = Value::Sequence(vec![
            Value::Value(ScalarOwned::Integer(1)),
            Value::Value(ScalarOwned::Integer(2)),
            Value::Value(ScalarOwned::String("hello".to_string())),
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
    fn test_estimate_value_size_all_variants() {
        use saphyr_parser::{ScalarStyle, Tag};

        // Test Representation variant
        let repr = Value::Representation("custom".to_string(), ScalarStyle::Plain, None);
        let repr_size = Emitter::estimate_value_size(&repr);
        assert_eq!(repr_size, 8); // 6 chars + 2

        // Test Tagged variant
        let tag = Tag {
            handle: "!".to_string(),
            suffix: "custom".to_string(),
        };
        let tagged = Value::Tagged(tag, Box::new(Value::Value(ScalarOwned::Integer(42))));
        let tagged_size = Emitter::estimate_value_size(&tagged);
        // 10 (tag overhead) + inner value size
        assert!(tagged_size >= 10, "Tagged value should have tag overhead");

        // Test Alias variant (usize anchor ID)
        let alias = Value::Alias(1);
        let alias_size = Emitter::estimate_value_size(&alias);
        assert_eq!(alias_size, 10);

        // Test BadValue variant
        let bad = Value::BadValue;
        let bad_size = Emitter::estimate_value_size(&bad);
        assert_eq!(bad_size, 4);
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
            .map(|i| Value::Value(ScalarOwned::String(format!("document_{i}"))))
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
    fn test_estimate_scalar_size_large_integer() {
        // Test max i64
        let max_int = Emitter::estimate_scalar_size(&ScalarOwned::Integer(i64::MAX));
        // i64::MAX = 9223372036854775807 (19 digits + potential sign)
        assert!(max_int >= 19, "Max i64 should have at least 19 chars");

        // Test min i64
        let min_int = Emitter::estimate_scalar_size(&ScalarOwned::Integer(i64::MIN));
        // i64::MIN = -9223372036854775808 (19 digits + sign)
        assert!(min_int >= 19, "Min i64 should have at least 19 chars");

        // Test powers of 10
        let thousand = Emitter::estimate_scalar_size(&ScalarOwned::Integer(1000));
        assert!(thousand >= 4, "1000 should have at least 4 chars");

        let million = Emitter::estimate_scalar_size(&ScalarOwned::Integer(1_000_000));
        assert!(million >= 7, "1000000 should have at least 7 chars");
    }

    #[test]
    fn test_might_contain_special_floats_empty() {
        assert!(!Emitter::might_contain_special_floats(""));
    }

    #[test]
    fn test_fix_special_floats_no_changes() {
        // Test output that doesn't contain special floats (fast path)
        let input = "key: value\nlist:\n  - item1\n  - item2\nnumber: 42\n";
        let result = Emitter::fix_special_floats(input);
        assert_eq!(result, input, "No changes should be made for normal YAML");
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
        let value = Value::Value(ScalarOwned::String("hello".to_string()));
        let result = Emitter::emit_str(&value).unwrap();
        assert!(
            result.ends_with('\n'),
            "emit_str output must end with newline, got: {result:?}"
        );
    }

    #[test]
    fn test_emit_str_with_config_ends_with_newline_default() {
        let value = Value::Value(ScalarOwned::String("hello".to_string()));
        let config = EmitterConfig::default();
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        assert!(
            result.ends_with('\n'),
            "emit_str_with_config output must end with newline (default config), got: {result:?}"
        );
    }

    #[test]
    fn test_emit_str_with_config_ends_with_newline_explicit_start() {
        let value = Value::Value(ScalarOwned::String("hello".to_string()));
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
            let value = Value::Value(ScalarOwned::String(text.to_string()));
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
        use saphyr::MappingOwned;

        let mut map = MappingOwned::new();
        map.insert(
            Value::Value(ScalarOwned::String("key".to_string())),
            Value::Sequence(vec![
                Value::Value(ScalarOwned::Integer(1)),
                Value::Value(ScalarOwned::Integer(2)),
            ]),
        );
        let value = Value::Mapping(map);
        let config = EmitterConfig::new().with_indent(4);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        // With indent=4, list items under a key should start with 4 spaces.
        assert!(
            result.contains("    - 1") || result.contains("    -"),
            "indent=4 should produce 4-space indentation, got: {result:?}"
        );
    }

    #[test]
    fn test_emit_default_flow_style_true_mapping() {
        use saphyr::MappingOwned;

        let mut map = MappingOwned::new();
        map.insert(
            Value::Value(ScalarOwned::String("a".to_string())),
            Value::Value(ScalarOwned::Integer(1)),
        );
        map.insert(
            Value::Value(ScalarOwned::String("b".to_string())),
            Value::Value(ScalarOwned::Integer(2)),
        );
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
    fn test_emit_default_flow_style_true_sequence() {
        let value = Value::Sequence(vec![
            Value::Value(ScalarOwned::Integer(1)),
            Value::Value(ScalarOwned::Integer(2)),
            Value::Value(ScalarOwned::Integer(3)),
        ]);
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
        let value = Value::Sequence(vec![
            Value::Value(ScalarOwned::Integer(1)),
            Value::Value(ScalarOwned::Integer(2)),
        ]);
        let config = EmitterConfig::new().with_default_flow_style(None);
        let result = Emitter::emit_str_with_config(&value, &config).unwrap();
        // Block style uses "- " prefix per item.
        assert!(
            result.contains("- 1"),
            "default_flow_style=None should produce block sequence, got: {result:?}"
        );
    }

    #[test]
    fn test_reindent_basic() {
        // saphyr emits 2-space indent; reindent to 4 should double it.
        let input = "key:\n  nested: value\n";
        let result = Emitter::reindent(input, 4);
        assert!(
            result.contains("    nested: value"),
            "reindent(4) should produce 4 spaces, got: {result:?}"
        );
    }

    #[test]
    fn test_reindent_preserves_markers() {
        let input = "---\nkey: value\n";
        let result = Emitter::reindent(input, 4);
        assert!(result.contains("---"), "--- marker must be preserved");
        assert!(result.contains("key: value"));
    }

    #[test]
    fn test_format_with_config_strips_bom() {
        let out =
            Emitter::format_with_config("\u{FEFF}# c\na: 1\n", &EmitterConfig::default()).unwrap();
        assert!(!out.contains('\u{FEFF}'), "BOM leaked into output: {out:?}");
        assert!(out.contains("a: 1"));
    }

    #[test]
    fn test_format_with_config_bom_before_directive() {
        let out = Emitter::format_with_config(
            "\u{FEFF}%YAML 1.2\n---\na: 1\n",
            &EmitterConfig::default(),
        )
        .unwrap();
        assert!(out.starts_with("%YAML 1.2\n"), "got {out:?}");
    }

    #[test]
    fn test_format_with_config_bom_crlf_multi_doc() {
        let out = Emitter::format_with_config(
            "\u{FEFF}---\r\na: 1\r\n---\r\nb: 2\r\n",
            &EmitterConfig::default(),
        )
        .unwrap();
        assert!(!out.contains('\u{FEFF}'));
        assert!(out.contains("a: 1") && out.contains("b: 2"));
    }

    fn format_at(input: &str, indent: usize) -> String {
        let config = EmitterConfig::new().with_indent(indent);
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
        assert_eq!(block_scalar_header('|', "a\n", 2), "|");
        assert_eq!(block_scalar_header('|', "a", 2), "|-");
        assert_eq!(block_scalar_header('>', "a\n\n", 2), ">+");
        assert_eq!(block_scalar_header('|', " a\n", 4), "|4");
        assert_eq!(block_scalar_header('|', " a\n\n", 3), "|3+");
        assert_eq!(block_scalar_header('|', " a", 1), "|1-");
        assert_eq!(block_scalar_header('>', "\n  a\n", 2), ">2");
        assert_eq!(block_scalar_header('|', "\n\n", 2), "|+");
        assert_eq!(block_scalar_header('|', "\n", 2), "|+");
        assert_eq!(block_scalar_header('|', "\ta\n", 2), "|");
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
                .all(|v| matches!(v, Value::Value(ScalarOwned::String(s)) if s == "1")),
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
        use saphyr::MappingOwned;

        let mut map = MappingOwned::new();
        map.insert(key, Value::Value(ScalarOwned::Integer(1)));
        let config = EmitterConfig::new().with_default_flow_style(Some(true));
        Emitter::emit_str_with_config(&Value::Mapping(map), &config).unwrap()
    }

    fn string_key(s: &str) -> Value {
        Value::Value(ScalarOwned::String(s.to_owned()))
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
    fn flow_quoted_representation_keys_are_escaped() {
        let double =
            Value::Representation("say \"hi\"\n".to_owned(), ScalarStyle::DoubleQuoted, None);
        assert_eq!(flow_with_key(double), "{\"say \\\"hi\\\"\\n\": 1}\n");
        let plain = Value::Representation("1".to_owned(), ScalarStyle::Plain, None);
        assert_eq!(flow_with_key(plain), "{1: 1}\n");
        let single = Value::Representation("it's".to_owned(), ScalarStyle::SingleQuoted, None);
        assert_eq!(flow_with_key(single), "{'it''s': 1}\n");
    }

    #[test]
    fn flow_merge_lookalike_key_is_quoted() {
        assert_eq!(flow_with_key(string_key("<<")), "{\"<<\": 1}\n");
        let plain = Value::Representation("<<".to_owned(), ScalarStyle::Plain, None);
        assert_eq!(flow_with_key(plain), "{\"<<\": 1}\n");
    }

    #[test]
    fn flow_block_and_plain_representation_keys_round_trip() {
        for (text, style) in [
            ("a\nb", ScalarStyle::Plain),
            ("a\nb\n", ScalarStyle::Literal),
            ("a\nb\n", ScalarStyle::Folded),
            ("x: y", ScalarStyle::Plain),
            ("a, b", ScalarStyle::Literal),
        ] {
            let key = Value::Representation(text.to_owned(), style, None);
            let out = flow_with_key(key);
            let Some(Value::Mapping(map)) = crate::Parser::parse_str(&out).unwrap() else {
                panic!("mapping expected for {text:?}: {out}");
            };
            assert_eq!(map.len(), 1, "{text:?} {style:?}: {out}");
            let (k, _) = map.iter().next().unwrap();
            assert_eq!(k, &string_key(text), "{text:?} {style:?}: {out}");
        }
        for style in [ScalarStyle::Literal, ScalarStyle::Folded] {
            let big = Value::Representation("123456789012345678901234567890".into(), style, None);
            assert_eq!(
                flow_with_key(big),
                "{\"123456789012345678901234567890\": 1}\n"
            );
        }
        let big = Value::Representation(
            "123456789012345678901234567890".into(),
            ScalarStyle::Plain,
            None,
        );
        assert_eq!(flow_with_key(big), "{123456789012345678901234567890: 1}\n");
    }

    #[test]
    fn non_string_lookalike_strings_round_trip_in_every_position() {
        use saphyr::MappingOwned;

        for text in [
            "+.inf", "+.Inf", "+.INF", ".5", "-.5", "+.5e3", ".inf", "1", "true", "null",
        ] {
            let mut inner = MappingOwned::new();
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
    fn emit_custom_tagged_big_int_keeps_tag() {
        let input = format!("k: !foo {BIG}\n");
        let doc = crate::Parser::parse_str(&input).unwrap().unwrap();
        assert_eq!(Emitter::emit_str(&doc).unwrap(), input);
        let flow = EmitterConfig::new().with_default_flow_style(Some(true));
        assert_eq!(
            Emitter::emit_str_with_config(&doc, &flow).unwrap(),
            format!("{{k: !foo {BIG}}}\n")
        );
    }

    #[test]
    fn emit_hand_built_block_representation_as_string() {
        for style in [ScalarStyle::Literal, ScalarStyle::Folded] {
            let doc = Value::Representation("a\nb".to_string(), style, None);
            for flow in [None, Some(true)] {
                let config = EmitterConfig::new().with_default_flow_style(flow);
                let out = Emitter::emit_str_with_config(&doc, &config).unwrap();
                let back = crate::Parser::parse_str(&out).unwrap().unwrap();
                assert_eq!(back, Value::Value(ScalarOwned::String("a\nb".to_string())));
            }
        }
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

    fn tag(handle: &str, suffix: &str) -> Tag {
        Tag {
            handle: handle.to_string(),
            suffix: suffix.to_string(),
        }
    }

    fn core_tag(suffix: &str) -> Tag {
        tag("tag:yaml.org,2002:", suffix)
    }

    fn repr(text: &str, style: ScalarStyle, tag: Option<Tag>) -> Value {
        Value::Representation(text.to_string(), style, tag)
    }

    fn string(text: &str) -> Value {
        Value::Value(ScalarOwned::String(text.to_string()))
    }

    fn emit_both(doc: &Value) -> [String; 2] {
        [None, Some(true)].map(|flow| {
            let config = EmitterConfig::new().with_default_flow_style(flow);
            Emitter::emit_str_with_config(doc, &config).unwrap()
        })
    }

    #[test]
    fn emit_tagged_wrapper_around_block_representation_does_not_panic() {
        let custom = tag("!", "foo");
        let wrapped_big = Value::Tagged(
            custom.clone(),
            Box::new(repr(BIG, ScalarStyle::Literal, Some(core_tag("int")))),
        );
        for out in emit_both(&wrapped_big) {
            assert_eq!(out.trim_end(), format!("!foo {BIG}"));
        }

        let mut map = MappingOwned::new();
        map.insert(string("k"), repr("a\nb", ScalarStyle::Folded, None));
        let wrapped_map = Value::Tagged(custom, Box::new(Value::Mapping(map)));
        for out in emit_both(&wrapped_map) {
            assert!(out.contains("!foo"), "{out}");
        }
    }

    #[test]
    fn emit_core_tagged_hand_built_scalars_keep_their_type() {
        let cases = [
            (
                repr("123", ScalarStyle::DoubleQuoted, Some(core_tag("int"))),
                ScalarOwned::Integer(123),
            ),
            (
                repr("1.5", ScalarStyle::Plain, Some(core_tag("float"))),
                ScalarOwned::FloatingPoint(1.5.into()),
            ),
            (
                repr("true", ScalarStyle::SingleQuoted, Some(core_tag("bool"))),
                ScalarOwned::Boolean(true),
            ),
            (
                repr("~", ScalarStyle::Plain, Some(core_tag("null"))),
                ScalarOwned::Null,
            ),
        ];
        for (doc, expected) in cases {
            let mut map = MappingOwned::new();
            map.insert(string("k"), doc);
            let doc = Value::Mapping(map);
            for out in emit_both(&doc) {
                let back = crate::Parser::parse_str(&out).unwrap().unwrap();
                let Value::Mapping(back) = back else {
                    panic!("{out}")
                };
                assert_eq!(
                    back.get(&string("k")),
                    Some(&Value::Value(expected.clone())),
                    "{out}"
                );
            }
        }
    }

    #[test]
    fn emit_clip_chomped_core_tagged_representation_is_string() {
        let doc = repr(
            &format!("{BIG}\n"),
            ScalarStyle::Literal,
            Some(core_tag("int")),
        );
        for out in emit_both(&doc) {
            let back = crate::Parser::parse_str(&out).unwrap().unwrap();
            assert_eq!(back, string(&format!("{BIG}\n")), "{out}");
        }
    }

    #[test]
    fn prepare_for_saphyr_borrows_when_nothing_to_rewrite() {
        let plain = crate::Parser::parse_str("a: 1\nb: !foo x\nc: \"q\"\nd: [1, 2]\n")
            .unwrap()
            .unwrap();
        assert!(matches!(prepare_for_saphyr(&plain), Cow::Borrowed(_)));

        let block = crate::Parser::parse_str(&format!("k: !!int |-\n  {BIG}\n"))
            .unwrap()
            .unwrap();
        assert!(matches!(prepare_for_saphyr(&block), Cow::Owned(_)));
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

    #[test]
    fn emit_custom_tag_on_block_scalar_is_preserved_when_hand_built() {
        let doc = repr("a\nb", ScalarStyle::Literal, Some(tag("!", "foo")));
        for out in emit_both(&doc) {
            assert!(out.starts_with("!foo "), "{out}");
        }
    }
}
