//! Rule to detect duplicate anchor definitions in YAML documents.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity, SourceContext,
};
use fast_yaml_core::Value;
use std::collections::HashMap;

/// Rule to detect duplicate anchor definitions.
///
/// Scans raw YAML source for `&name` anchor definitions and reports any anchor
/// that is defined more than once within the same document. The second and each
/// subsequent definition produce a `Warning` diagnostic.
///
/// False-positive prevention:
/// - Lines where the entire line is a comment are skipped.
/// - Inline comments (text after an unquoted `#`) are stripped before scanning.
/// - Content inside single- and double-quoted strings is skipped, including
///   strings that span multiple lines.
/// - Content inside block scalars (`|` / `>`) is skipped using indentation-based
///   termination detection.
/// - Document boundaries (`---` at column 0) reset the anchor map.
pub struct InvalidAnchorsRule;

impl super::LintRule for InvalidAnchorsRule {
    fn code(&self) -> &str {
        DiagnosticCode::INVALID_ANCHOR
    }

    fn name(&self) -> &'static str {
        "Invalid Anchors"
    }

    fn description(&self) -> &'static str {
        "Detects duplicate anchor definitions"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(
        &self,
        context: &LintContext,
        _value: &Value,
        _config: &LintConfig,
    ) -> Vec<Diagnostic> {
        scan_duplicate_anchors(context.source(), context.source_context())
    }
}

// ── Anchor name pattern ────────────────────────────────────────────────────
// YAML 1.2.2 ns-anchor-name: one or more ns-char excluding flow indicators.
// We use a permissive byte-level scan that terminates at whitespace and the
// characters `,`, `[`, `]`, `{`, `}`, `:`, `&`, `*`.
const ANCHOR_TERMINATORS: &[u8] = b" \t\r\n,[]{}:&*";

// ── Scanner state ──────────────────────────────────────────────────────────

/// Active quote style for multi-line quoted-string tracking.
#[derive(Clone, Copy, PartialEq, Eq)]
enum QuoteState {
    None,
    Single,
    Double,
}

/// Block scalar tracking: indentation level of the parent key.
#[derive(Clone, Copy)]
struct BlockScalarState {
    /// Indentation level of the block scalar indicator line.
    /// All continuation lines must have strictly greater indentation.
    parent_indent: usize,
}

/// Full scanner state carried across lines.
struct ScanState {
    quote: QuoteState,
    block_scalar: Option<BlockScalarState>,
}

impl ScanState {
    const fn new() -> Self {
        Self {
            quote: QuoteState::None,
            block_scalar: None,
        }
    }
}

// ── Main scan function ─────────────────────────────────────────────────────

fn scan_duplicate_anchors(source: &str, source_context: &SourceContext<'_>) -> Vec<Diagnostic> {
    // Map from anchor name → (1-indexed line, 1-indexed column) of first def.
    let mut seen: HashMap<String, (usize, usize)> = HashMap::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut state = ScanState::new();

    // We need byte offsets of line starts for Span construction.
    let mut line_start_offset: usize = 0;

    for (line_idx, line) in source.lines().enumerate() {
        let line_number = line_idx + 1; // 1-indexed

        // ── Document boundary: reset anchor map ──────────────────────────
        if state.quote == QuoteState::None
            && state.block_scalar.is_none()
            && is_document_start(line)
        {
            seen.clear();
            line_start_offset += line.len() + 1;
            continue;
        }

        // ── Whole-line comment: skip entirely ─────────────────────────────
        if state.quote == QuoteState::None
            && state.block_scalar.is_none()
            && line.trim_start().starts_with('#')
        {
            line_start_offset += line.len() + 1;
            continue;
        }

        // ── Block scalar continuation ─────────────────────────────────────
        if let Some(bs) = state.block_scalar {
            let indent = leading_spaces(line);
            if !line.trim().is_empty() && indent <= bs.parent_indent {
                // Block scalar ended; fall through to normal scanning.
                state.block_scalar = None;
            } else {
                // Still inside block scalar content — skip anchor scanning.
                line_start_offset += line.len() + 1;
                continue;
            }
        }

        // ── Detect block scalar start ─────────────────────────────────────
        // A block scalar indicator `|` or `>` at the end of a value position
        // starts a block scalar. We detect it when not inside a quote.
        if state.quote == QuoteState::None
            && let Some(indent) = detect_block_scalar_start(line)
        {
            // The indicator line itself may still carry an anchor before `|`/`>`,
            // so we continue scanning this line but the next lines will be skipped.
            state.block_scalar = Some(BlockScalarState {
                parent_indent: indent,
            });
        }

        // ── Scan the line for `&name` occurrences ─────────────────────────
        scan_line_for_anchors(
            line,
            line_number,
            line_start_offset,
            source,
            source_context,
            &mut state,
            &mut seen,
            &mut diagnostics,
        );

        line_start_offset += line.len() + 1; // +1 for the '\n'
    }

    diagnostics
}

// ── Line-level anchor scanner ──────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn scan_line_for_anchors(
    line: &str,
    line_number: usize,
    line_start_offset: usize,
    _source: &str,
    source_context: &SourceContext<'_>,
    state: &mut ScanState,
    seen: &mut HashMap<String, (usize, usize)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        match state.quote {
            QuoteState::Single => {
                if b == b'\'' {
                    // Check for escaped single quote `''`
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        i += 2;
                    } else {
                        state.quote = QuoteState::None;
                        i += 1;
                    }
                } else {
                    i += 1;
                }
                continue;
            }

            QuoteState::Double => {
                if b == b'\\' {
                    i += 2; // skip escaped character
                } else if b == b'"' {
                    state.quote = QuoteState::None;
                    i += 1;
                } else {
                    i += 1;
                }
                continue;
            }

            QuoteState::None => {}
        }

        // Outside quotes:
        match b {
            b'\'' => {
                state.quote = QuoteState::Single;
                i += 1;
            }
            b'"' => {
                state.quote = QuoteState::Double;
                i += 1;
            }
            b'#' => {
                // Inline comment — stop scanning this line.
                // (A `#` that starts an inline comment must be preceded by
                // whitespace per the YAML spec, but skipping everything after
                // any bare `#` outside quotes is safe enough for anchor detection.)
                break;
            }
            b'&' => {
                // Potential anchor definition.
                let name_start = i + 1;
                let name_end = find_anchor_name_end(bytes, name_start);
                if name_end > name_start {
                    let name = &line[name_start..name_end];
                    let col = i + 1; // 1-indexed column of `&`
                    let offset = line_start_offset + i;

                    if let Some(&(first_line, _first_col)) = seen.get(name) {
                        let span = build_span(line_number, col, offset, name.len() + 1);
                        diagnostics.push(
                            DiagnosticBuilder::new(
                                DiagnosticCode::INVALID_ANCHOR,
                                Severity::Warning,
                                format!(
                                    "anchor '&{name}' is defined multiple times; \
                                     the earlier definition is shadowed \
                                     (first defined at line {first_line})"
                                ),
                                span,
                            )
                            .with_suggestion("rename this anchor to be unique", span, None)
                            .build_with_context(source_context),
                        );
                    } else {
                        seen.insert(name.to_owned(), (line_number, col));
                    }

                    i = name_end;
                } else {
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    // If we exited the loop while still inside a quote that was opened on
    // this line, the quote continues to the next line (multi-line string).
    // `state.quote` already reflects this.
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Returns true if `line` is a YAML document-start marker at column 0.
fn is_document_start(line: &str) -> bool {
    line.strip_prefix("---")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t'))
}

/// Returns the number of leading space characters on `line`.
fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|&b| b == b' ').count()
}

/// Detects whether `line` ends with a block scalar indicator (`|` or `>`).
/// Returns the indentation of the line (used as `parent_indent`) when found,
/// otherwise `None`.
fn detect_block_scalar_start(line: &str) -> Option<usize> {
    // Strip inline comment and trailing whitespace.
    let stripped = strip_inline_comment(line).trim_end();
    // The last meaningful character must be `|` or `>` (optionally followed
    // by chomping/indentation modifiers like `|2-` or `>+`).
    // We look for `|` or `>` preceded by `:` or whitespace (value position).
    let last = stripped.chars().next_back()?;
    if !matches!(last, '|' | '>' | '-' | '+') {
        return None;
    }

    // Walk backwards past optional modifiers to find the indicator.
    let mut chars = stripped.chars().rev().peekable();
    // Skip chomping / indentation modifiers: digits, `-`, `+`
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '-' || c == '+' {
            chars.next();
        } else {
            break;
        }
    }
    let indicator = chars.next()?;
    if !matches!(indicator, '|' | '>') {
        return None;
    }

    Some(leading_spaces(line))
}

/// Strips the inline comment part of a line (everything from an unquoted `#`
/// that follows whitespace).  Used only for block scalar detection.
fn strip_inline_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if !in_double => {
                in_single = !in_single;
                i += 1;
            }
            b'"' if !in_single => {
                in_double = !in_double;
                i += 1;
            }
            b'\\' if in_double => {
                i += 2;
            }
            b'#' if !in_single && !in_double => {
                return &line[..i];
            }
            _ => {
                i += 1;
            }
        }
    }
    line
}

/// Finds the end byte index of an anchor name starting at `start` in `bytes`.
fn find_anchor_name_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len() && !ANCHOR_TERMINATORS.contains(&bytes[end]) {
        end += 1;
    }
    end
}

/// Constructs a `Span` for an anchor token (`&name`) at a given position.
///
/// `col` is 1-indexed column of `&`; `len` is `name.len() + 1` (includes `&`).
const fn build_span(line: usize, col: usize, offset: usize, len: usize) -> crate::Span {
    use crate::{Location, Span};
    Span::new(
        Location::new(line, col, offset),
        Location::new(line, col + len, offset + len),
    )
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::LintRule;
    use fast_yaml_core::Parser;

    fn run(yaml: &str) -> Vec<Diagnostic> {
        let value = Parser::parse_str(yaml)
            .unwrap()
            .unwrap_or(Value::Value(fast_yaml_core::ScalarOwned::Null));
        InvalidAnchorsRule.check(&LintContext::new(yaml), &value, &LintConfig::default())
    }

    #[test]
    fn test_single_anchor_no_warning() {
        assert!(run("a: &anchor value").is_empty());
    }

    #[test]
    fn test_duplicate_anchor_one_warning() {
        let diags = run("a: &anchor value1\nb: &anchor value2\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("anchor '&anchor'"));
        assert_eq!(diags[0].span.start.line, 2);
        assert_eq!(diags[0].severity, Severity::Warning);
    }

    #[test]
    fn test_triple_anchor_two_warnings() {
        let diags = run("a: &anchor v1\nb: &anchor v2\nc: &anchor v3\n");
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn test_anchor_in_comment_no_warning() {
        let yaml = "a: value\n# &anchor is not an anchor\nb: other\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn test_anchor_in_double_quoted_string_no_warning() {
        let yaml = "a: \"contains &not_anchor here\"\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn test_anchor_in_single_quoted_string_no_warning() {
        let yaml = "a: 'contains &not_anchor here'\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn test_different_anchor_names_no_warning() {
        let yaml = "a: &anchor1 val\nb: &anchor2 val\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn test_first_defined_line_in_message() {
        let diags = run("x: &foo 1\ny: other\nz: &foo 2\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("first defined at line 1"));
        assert_eq!(diags[0].span.start.line, 3);
    }

    #[test]
    fn test_document_boundary_resets_anchors() {
        // Second document redefines &anchor — no warning because boundary resets map.
        let yaml = "a: &anchor val\n---\nb: &anchor val\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn test_inline_comment_anchor_no_warning() {
        let yaml = "a: value # &not_anchor\nb: other\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn test_permissive_anchor_name_with_dots() {
        let diags = run("a: &config.prod 1\nb: &config.prod 2\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("&config.prod"));
    }

    #[test]
    fn test_anchor_in_multiline_double_quoted_no_warning() {
        // The `&not_anchor` on the continuation line is inside a double-quoted string.
        let yaml = "desc: \"first line\n  &not_anchor continuation\"\n";
        assert!(run(yaml).is_empty());
    }

    #[test]
    fn test_block_scalar_anchor_no_warning() {
        let yaml = "script: |\n  echo &not_an_anchor\n  curl *endpoint\nkey: value\n";
        assert!(run(yaml).is_empty());
    }
}
