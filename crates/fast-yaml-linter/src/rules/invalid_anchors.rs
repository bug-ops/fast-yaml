//! Rule to detect duplicate and unused anchor definitions in YAML documents.

use super::{LintRule, RuleId};
use crate::config::RuleName;
use serde::{Deserialize, Serialize};

use crate::config::{AlwaysTrue, RuleOptions};
use crate::context::source_lines;
use crate::source::offset::ByteOffset;
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity,
    SourceContext, Span,
};
use std::collections::HashMap;

/// Rule to detect duplicate and unused anchor definitions.
///
/// Scans raw YAML source for `&name` anchor definitions and reports any anchor
/// that is defined more than once within the same document. The second and each
/// subsequent definition produce a `Warning` diagnostic. With `forbid-unused-anchors`
/// an anchor that no `*alias` of the same document refers to is reported too.
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

/// Options of the invalid-anchor rule, named like yamllint's `anchors` rule.
///
/// Unlike yamllint, duplicated anchors are reported by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct InvalidAnchorsOptions {
    /// An alias without an earlier anchor is a parse error, so it is always reported; only
    /// `true` is accepted.
    pub forbid_undeclared_aliases: AlwaysTrue,
    /// Report an anchor defined again in the same document.
    pub forbid_duplicated_anchors: bool,
    /// Report an anchor that is never referenced by an alias in its document.
    pub forbid_unused_anchors: bool,
}

impl Default for InvalidAnchorsOptions {
    fn default() -> Self {
        Self {
            forbid_undeclared_aliases: AlwaysTrue,
            forbid_duplicated_anchors: true,
            forbid_unused_anchors: false,
        }
    }
}

impl RuleOptions for InvalidAnchorsOptions {}

impl super::LintRule for InvalidAnchorsRule {
    fn id(&self) -> RuleId<'_> {
        RuleId::BuiltIn(RuleName::InvalidAnchor)
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
}

impl super::SourceRule for InvalidAnchorsRule {
    fn check(&self, context: &LintContext, config: &LintConfig) -> Vec<Diagnostic> {
        let severity = config
            .rules
            .invalid_anchor
            .severity_or(self.default_severity());
        scan_anchors(
            context.source(),
            context.source_context(),
            severity,
            config.rules.invalid_anchor.options,
        )
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

fn scan_anchors(
    source: &str,
    source_context: &SourceContext<'_>,
    severity: Severity,
    options: InvalidAnchorsOptions,
) -> Vec<Diagnostic> {
    let mut scan = AnchorScan {
        source_context,
        severity,
        options,
        state: ScanState::new(),
        seen: HashMap::new(),
        diagnostics: Vec::new(),
    };

    for (line_idx, (line_start_offset, line)) in source_lines(source).enumerate() {
        let line_number = line_idx + 1; // 1-indexed
        let state = &mut scan.state;

        // ── Document boundary: reset anchor map ──────────────────────────
        if state.quote == QuoteState::None
            && state.block_scalar.is_none()
            && (is_document_start(line) || is_document_end(line))
        {
            scan.end_document();
            if is_document_start(line) {
                let rest = line.get(3..).unwrap_or_default();
                scan.scan_line(rest, line_number, line_start_offset + 3);
            }
            continue;
        }

        // ── Whole-line comment: skip entirely ─────────────────────────────
        if state.quote == QuoteState::None
            && state.block_scalar.is_none()
            && line.trim_start().starts_with('#')
        {
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
        scan.scan_line(line, line_number, line_start_offset);
    }

    scan.end_document();
    scan.diagnostics
}

// ── Line-level anchor scanner ──────────────────────────────────────────────

/// Line scanner that accumulates anchor definitions and duplicate diagnostics.
struct AnchorScan<'a> {
    source_context: &'a SourceContext<'a>,
    severity: Severity,
    options: InvalidAnchorsOptions,
    state: ScanState,
    seen: HashMap<String, Anchor>,
    diagnostics: Vec<Diagnostic>,
}

/// An anchor definition of the current document.
struct Anchor {
    /// 1-indexed line of the first definition of the name.
    first_line: usize,
    /// Span of the latest `&name`.
    span: Span,
    used: bool,
}

impl AnchorScan<'_> {
    /// Reports the unused anchors of the finished document and starts a new one.
    fn end_document(&mut self) {
        if self.options.forbid_unused_anchors {
            let mut unused: Vec<(&String, &Anchor)> =
                self.seen.iter().filter(|(_, a)| !a.used).collect();
            unused.sort_by_key(|(_, anchor)| anchor.span.start.offset);
            for (name, anchor) in unused {
                self.diagnostics.push(
                    DiagnosticBuilder::new(
                        DiagnosticCode::INVALID_ANCHOR,
                        self.severity,
                        format!("anchor '&{name}' is never used by an alias"),
                        anchor.span,
                    )
                    .with_suggestion("remove the anchor or reference it", anchor.span, None)
                    .build(),
                );
            }
        }
        self.seen.clear();
    }

    fn scan_line(&mut self, line: &str, line_number: usize, line_start_offset: usize) {
        let bytes = line.as_bytes();
        let mut i = 0;

        while let Some(&b) = bytes.get(i) {
            match self.state.quote {
                QuoteState::Single => {
                    if b == b'\'' {
                        // Check for escaped single quote `''`
                        if bytes.get(i + 1) == Some(&b'\'') {
                            i += 2;
                        } else {
                            self.state.quote = QuoteState::None;
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
                        self.state.quote = QuoteState::None;
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
                    self.state.quote = QuoteState::Single;
                    i += 1;
                }
                b'"' => {
                    self.state.quote = QuoteState::Double;
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
                        let name = line.get(name_start..name_end).unwrap_or_default();

                        let span = self
                            .source_context
                            .span_at(ByteOffset::new(line_start_offset + i), name.len() + 1);
                        let first_line = self.seen.get(name).map_or(line_number, |a| a.first_line);
                        if self.options.forbid_duplicated_anchors && self.seen.contains_key(name) {
                            self.diagnostics.push(
                                DiagnosticBuilder::new(
                                    DiagnosticCode::INVALID_ANCHOR,
                                    self.severity,
                                    format!(
                                        "anchor '&{name}' is defined multiple times; \
                                         the earlier definition is shadowed \
                                         (first defined at line {first_line})"
                                    ),
                                    span,
                                )
                                .with_suggestion("rename this anchor to be unique", span, None)
                                .build(),
                            );
                        }
                        self.seen.insert(
                            name.to_owned(),
                            Anchor {
                                first_line,
                                span,
                                used: false,
                            },
                        );

                        i = name_end;
                    } else {
                        i += 1;
                    }
                }
                b'*' if i == 0 || bytes.get(i - 1).is_some_and(|p| b" \t[{,".contains(p)) => {
                    let name_end = find_anchor_name_end(bytes, i + 1);
                    if let Some(anchor) = line
                        .get(i + 1..name_end)
                        .and_then(|name| self.seen.get_mut(name))
                    {
                        anchor.used = true;
                    }
                    i = name_end.max(i + 1);
                }
                _ => {
                    i += 1;
                }
            }
        }

        // If we exited the loop while still inside a quote that was opened on
        // this line, the quote continues to the next line (multi-line string).
        // `self.state.quote` already reflects this.
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Returns true if `line` is a YAML document-start marker at column 0.
fn is_document_start(line: &str) -> bool {
    line.strip_prefix("---")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t'))
}

/// Returns true if `line` is a YAML document-end marker at column 0.
fn is_document_end(line: &str) -> bool {
    line.strip_prefix("...")
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
    while let Some(&byte) = bytes.get(i) {
        match byte {
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
                return line.get(..i).unwrap_or(line);
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
    while bytes
        .get(end)
        .is_some_and(|b| !ANCHOR_TERMINATORS.contains(b))
    {
        end += 1;
    }
    end
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{RuleName, test_support::config_with_rule},
        rules::SourceRule,
    };

    fn run(yaml: &str) -> Vec<Diagnostic> {
        InvalidAnchorsRule.check(&LintContext::new(yaml), &LintConfig::default())
    }

    #[test]
    fn test_single_anchor_no_warning() {
        assert_eq!(run("a: &anchor value"), []);
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
        assert_eq!(run(yaml), []);
    }

    #[test]
    fn test_anchor_in_double_quoted_string_no_warning() {
        let yaml = "a: \"contains &not_anchor here\"\n";
        assert_eq!(run(yaml), []);
    }

    #[test]
    fn test_anchor_in_single_quoted_string_no_warning() {
        let yaml = "a: 'contains &not_anchor here'\n";
        assert_eq!(run(yaml), []);
    }

    #[test]
    fn test_different_anchor_names_no_warning() {
        let yaml = "a: &anchor1 val\nb: &anchor2 val\n";
        assert_eq!(run(yaml), []);
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
        assert_eq!(run(yaml), []);
    }

    #[test]
    fn test_inline_comment_anchor_no_warning() {
        let yaml = "a: value # &not_anchor\nb: other\n";
        assert_eq!(run(yaml), []);
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
        assert_eq!(run(yaml), []);
    }

    #[test]
    fn test_block_scalar_anchor_no_warning() {
        let yaml = "script: |\n  echo &not_an_anchor\n  curl *endpoint\nkey: value\n";
        assert_eq!(run(yaml), []);
    }

    fn run_with(yaml: &str, options: &str) -> Vec<Diagnostic> {
        let config = config_with_rule(RuleName::InvalidAnchor, options);
        InvalidAnchorsRule.check(&LintContext::new(yaml), &config)
    }

    #[test]
    fn test_forbid_duplicated_anchors_false_allows_redefinition() {
        let yaml = "a: &x 1\nb: &x 2\n";
        assert_eq!(run_with(yaml, "{forbid-duplicated-anchors: false}"), []);
        assert_eq!(run_with(yaml, "{forbid-duplicated-anchors: true}").len(), 1);
    }

    #[test]
    fn test_forbid_unused_anchors() {
        let options = "{forbid-unused-anchors: true}";
        let found = run_with("- &a 1\n- &b 2\n- *a\n", options);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].message, "anchor '&b' is never used by an alias");
        assert_eq!(
            (found[0].span.start.line, found[0].span.start.column),
            (2, 3)
        );
        assert_eq!(run("- &a 1\n- &b 2\n"), []);
    }

    #[test]
    fn test_forbid_unused_anchors_is_per_document() {
        let options = "{forbid-unused-anchors: true}";
        let found = run_with("- &a 1\n- *a\n---\n- &a 2\n", options);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].span.start.line, 4);
        let found = run_with("- &a 1\n...\n---\n- &b 2\n- *b\n", options);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].span.start.line, 1);
    }

    #[test]
    fn test_forbid_unused_anchors_sees_flow_aliases_and_marker_line_anchors() {
        let options = "{forbid-unused-anchors: true}";
        assert_eq!(run_with("- &a 1\n- [*a, 2]\n", options), []);
        assert_eq!(run_with("- &a 1\n- {k: *a}\n", options), []);
        let found = run_with("--- &a x\n", options);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].span.start.column, 5);
    }

    #[test]
    fn test_forbid_unused_anchors_ignores_alias_lookalikes() {
        let options = "{forbid-unused-anchors: true}";
        let found = run_with("a: &x 1\nb: \"*x\"\nc: 2*x\n# *x\n", options);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn test_redefined_anchor_is_unused_again() {
        let options = "{forbid-unused-anchors: true, forbid-duplicated-anchors: false}";
        let found = run_with("- &a 1\n- *a\n- &a 2\n", options);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].span.start.line, 3);
    }

    #[test]
    fn test_forbid_undeclared_aliases_only_accepts_true() {
        let mut rules = crate::config::RulesConfig::default();
        let apply = |rules: &mut crate::config::RulesConfig, entry: &str| {
            rules.apply_rule(
                RuleName::InvalidAnchor,
                serde_norway::Deserializer::from_str(entry),
            )
        };
        assert!(apply(&mut rules, "{forbid-undeclared-aliases: true}").is_ok());
        assert!(apply(&mut rules, "{forbid-undeclared-aliases: false}").is_err());
    }

    #[test]
    fn test_severity_override() {
        let yaml = "a: &anchor value1\nb: &anchor value2\n";
        let config = config_with_rule(RuleName::InvalidAnchor, "{severity: error}");
        let diagnostics = InvalidAnchorsRule.check(&LintContext::new(yaml), &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }
}
