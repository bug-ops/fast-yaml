//! Scan-ahead guard: bounds how far the scanner reads past the last emitted event.
//!
//! The saphyr scanner queues a token for everything it has read but not yet reported, so queue
//! memory is at most about 190 times the characters consumed after the end of the last event.
//! [`GuardedInput`] counts consumed characters, [`GuardedParser`] records where the last event
//! ended, and the input turns into an exhausted one once the gap passes the limit. Nothing here
//! models YAML syntax, so every input shape is bounded alike.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use saphyr_parser::input::{Input, SkipTabs};
use saphyr_parser::{Event, Parser, Span, StrInput};

use crate::error::{ParseError, ParseResult, SourcePosition};
use crate::limits::{DocumentCursor, LimitKind, MaxScanAhead};

#[derive(Debug, Default)]
struct ScanState {
    /// Consumed characters as last published by the input; never ahead of the true count.
    consumed: AtomicUsize,
    covered: AtomicUsize,
    exceeded: AtomicBool,
    /// Exact consumed count, for the accounting tests.
    #[cfg(test)]
    exact: AtomicUsize,
}

/// [`StrInput`] that counts consumed characters and turns into EOF once the scan-ahead budget is spent.
///
/// The per-character paths only bump a local counter. The count is published and checked against
/// the budget in `lookahead` (the scanner calls it for every token) and after the bulk methods, so
/// the budget can be overshot by at most one token. The published count may trail the true one by
/// `step` characters (a sixteenth of the limit, at most 1024), which makes `covered` smaller and
/// the guard trip earlier, never later.
pub struct GuardedInput<'a> {
    inner: StrInput<'a>,
    state: Arc<ScanState>,
    limit: usize,
    step: usize,
    consumed: usize,
    /// Count past which the state must be synchronized: the earlier of the budget ceiling
    /// (`covered + limit`, which only grows) and the next publication.
    check_at: usize,
}

impl GuardedInput<'_> {
    #[inline]
    #[allow(
        clippy::missing_const_for_fn,
        reason = "tests also store the exact count"
    )]
    fn count(&mut self, chars: usize) {
        self.consumed += chars;
        #[cfg(test)]
        self.state.exact.store(self.consumed, Ordering::Relaxed);
    }

    #[inline]
    fn check(&mut self) {
        if self.consumed > self.check_at {
            self.sync();
        }
    }

    #[inline]
    fn charge(&mut self, chars: usize) {
        self.count(chars);
        self.check();
    }

    #[cold]
    fn sync(&mut self) {
        self.state.consumed.store(self.consumed, Ordering::Relaxed);
        let ceiling = self
            .state
            .covered
            .load(Ordering::Relaxed)
            .saturating_add(self.limit);
        if self.consumed > ceiling {
            self.trip();
            self.check_at = usize::MAX;
        } else {
            self.check_at = ceiling.min(self.consumed.saturating_add(self.step));
        }
    }

    // The scanner tracks buffered characters through `buflen`, so the exhausted input keeps it.
    fn trip(&mut self) {
        self.state.exceeded.store(true, Ordering::Relaxed);
        let mut exhausted = StrInput::new("");
        exhausted.lookahead(self.inner.buflen());
        self.inner = exhausted;
    }
}

impl Input for GuardedInput<'_> {
    #[inline]
    fn lookahead(&mut self, count: usize) {
        self.check();
        self.inner.lookahead(count);
    }

    #[inline]
    fn buflen(&self) -> usize {
        self.inner.buflen()
    }

    #[inline]
    fn bufmaxlen(&self) -> usize {
        self.inner.bufmaxlen()
    }

    #[inline]
    fn buf_is_empty(&self) -> bool {
        self.inner.buf_is_empty()
    }

    #[inline]
    fn raw_read_ch(&mut self) -> char {
        let c = self.inner.raw_read_ch();
        // NormalizedInput rejects NUL, so it only ever signals the end of the text.
        if c != '\0' {
            self.count(1);
        }
        c
    }

    #[inline]
    fn raw_read_non_breakz_ch(&mut self) -> Option<char> {
        let c = self.inner.raw_read_non_breakz_ch();
        if c.is_some() {
            self.count(1);
        }
        c
    }

    #[inline]
    fn skip(&mut self) {
        self.inner.skip();
        self.count(1);
    }

    #[inline]
    fn skip_n(&mut self, count: usize) {
        self.inner.skip_n(count);
        self.charge(count);
    }

    #[inline]
    fn peek(&self) -> char {
        self.inner.peek()
    }

    #[inline]
    fn peek_nth(&self, n: usize) -> char {
        self.inner.peek_nth(n)
    }

    #[inline]
    fn look_ch(&mut self) -> char {
        self.check();
        self.inner.look_ch()
    }

    #[inline]
    fn next_char_is(&self, c: char) -> bool {
        self.inner.next_char_is(c)
    }

    #[inline]
    fn nth_char_is(&self, n: usize, c: char) -> bool {
        self.inner.nth_char_is(n, c)
    }

    #[inline]
    fn next_2_are(&self, c1: char, c2: char) -> bool {
        self.inner.next_2_are(c1, c2)
    }

    #[inline]
    fn next_3_are(&self, c1: char, c2: char, c3: char) -> bool {
        self.inner.next_3_are(c1, c2, c3)
    }

    #[inline]
    fn next_is_document_indicator(&self) -> bool {
        self.inner.next_is_document_indicator()
    }

    #[inline]
    fn next_is_document_start(&self) -> bool {
        self.inner.next_is_document_start()
    }

    #[inline]
    fn next_is_document_end(&self) -> bool {
        self.inner.next_is_document_end()
    }

    #[inline]
    fn skip_ws_to_eol(&mut self, skip_tabs: SkipTabs) -> (usize, Result<SkipTabs, &'static str>) {
        let result = self.inner.skip_ws_to_eol(skip_tabs);
        self.charge(result.0);
        result
    }

    #[inline]
    fn next_can_be_plain_scalar(&self, in_flow: bool) -> bool {
        self.inner.next_can_be_plain_scalar(in_flow)
    }

    #[inline]
    fn next_is_blank_or_break(&self) -> bool {
        self.inner.next_is_blank_or_break()
    }

    #[inline]
    fn next_is_blank_or_breakz(&self) -> bool {
        self.inner.next_is_blank_or_breakz()
    }

    #[inline]
    fn next_is_blank(&self) -> bool {
        self.inner.next_is_blank()
    }

    #[inline]
    fn next_is_break(&self) -> bool {
        self.inner.next_is_break()
    }

    #[inline]
    fn next_is_breakz(&self) -> bool {
        self.inner.next_is_breakz()
    }

    #[inline]
    fn next_is_z(&self) -> bool {
        self.inner.next_is_z()
    }

    #[inline]
    fn next_is_flow(&self) -> bool {
        self.inner.next_is_flow()
    }

    #[inline]
    fn next_is_digit(&self) -> bool {
        self.inner.next_is_digit()
    }

    #[inline]
    fn next_is_alpha(&self) -> bool {
        self.inner.next_is_alpha()
    }

    #[inline]
    fn skip_while_non_breakz(&mut self) -> usize {
        let chars = self.inner.skip_while_non_breakz();
        self.charge(chars);
        chars
    }

    #[inline]
    fn skip_while_blank(&mut self) -> usize {
        let chars = self.inner.skip_while_blank();
        self.charge(chars);
        chars
    }

    // StrInput returns bytes here, so the characters are counted from the text it appended.
    #[inline]
    fn fetch_while_is_alpha(&mut self, out: &mut String) -> usize {
        let before = out.len();
        let bytes = self.inner.fetch_while_is_alpha(out);
        self.charge(out.get(before..).map_or(0, |s| s.chars().count()));
        bytes
    }

    #[inline]
    fn fetch_while_is_yaml_non_space(&mut self, out: &mut String) -> usize {
        let before = out.len();
        let bytes = self.inner.fetch_while_is_yaml_non_space(out);
        self.charge(out.get(before..).map_or(0, |s| s.chars().count()));
        bytes
    }
}

/// The only way the crate drives the saphyr parser: events plus the scan-ahead limit.
pub struct GuardedParser<'a> {
    parser: Parser<'a, GuardedInput<'a>>,
    state: Arc<ScanState>,
    limit: MaxScanAhead,
    cursor: DocumentCursor,
    last: SourcePosition,
}

impl<'a> GuardedParser<'a> {
    #[allow(
        clippy::disallowed_methods,
        reason = "the guard is the one place that builds a parser"
    )]
    pub fn new(text: &'a str, limit: MaxScanAhead) -> Self {
        let state = Arc::new(ScanState::default());
        let input = GuardedInput {
            inner: StrInput::new(text),
            state: Arc::clone(&state),
            limit: limit.get(),
            step: (limit.get() / 16).clamp(1, 1024),
            consumed: 0,
            check_at: 0,
        };
        Self {
            parser: Parser::new(input),
            state,
            limit,
            cursor: DocumentCursor::default(),
            last: SourcePosition { line: 1, column: 1 },
        }
    }

    /// Characters the scanner has consumed so far.
    #[cfg(test)]
    pub fn consumed(&self) -> usize {
        self.state.exact.load(Ordering::Relaxed)
    }

    /// End of the last event, in characters, as the guard counts it.
    #[cfg(test)]
    pub fn covered(&self) -> usize {
        self.state.covered.load(Ordering::Relaxed)
    }

    /// Position of the start of the last event, 1-indexed.
    pub const fn last_position(&self) -> SourcePosition {
        self.last
    }

    pub fn next_event(&mut self) -> Option<ParseResult<(Event<'a>, Span)>> {
        let next = self.parser.next_event();
        if self.state.exceeded.load(Ordering::Relaxed) {
            return Some(Err(ParseError::LimitExceeded {
                kind: LimitKind::ScanAhead(self.limit),
                line: self.last.line,
                column: self.last.column,
                document: self.cursor.index(),
            }));
        }
        Some(match next? {
            Ok((event, span)) => {
                self.cursor.observe(&event);
                self.cover(span);
                Ok((event, span))
            }
            Err(error) => Err(ParseError::scanner(&error, self.cursor.index())),
        })
    }

    // saphyr counts bytes for non-ASCII directive names, so the end is clamped to the (published)
    // count of characters read; a stale count only makes `covered` smaller.
    #[allow(
        clippy::disallowed_methods,
        reason = "char index compared with char consumption"
    )]
    fn cover(&mut self, span: Span) {
        let consumed = self.state.consumed.load(Ordering::Relaxed);
        let end = span.end.index().min(consumed);
        if end > self.state.covered.load(Ordering::Relaxed) {
            self.state.covered.store(end, Ordering::Relaxed);
        }
        self.last = SourcePosition::from_span(span);
    }
}

impl<'a> Iterator for GuardedParser<'a> {
    type Item = ParseResult<(Event<'a>, Span)>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_event()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use proptest::prelude::*;

    use super::*;

    const HAND_WRITTEN: &[&str] = &[
        "",
        "a: 1\n",
        "%YAML 1.2\n---\na: 1\n...\n",
        "%TAG !e! tag:example.com,2000:\n---\n!e!x a\n",
        "%\u{1D538}\u{1D538} x\n--- a\n",
        "%é x y\n---\n[1, 2]\n--- &a\u{1D538} b\n",
        "a: 'it''s'\nb: \"é\\n\\u00e9\"\nc: |\n  lit\n  eral\nd: >-\n  fol\n  ded\n",
        "# comment\n\n  # another\nkey: value # tail\n",
        "- &x [1, {a: b}]\n- *x\n- !!str tagged\n- ? k\n  : v\n",
        "a:\t[1,\n\t2]\r\nb: c\rd: e\n",
        "[1,\n---\n,2]",
        "'unterminated",
        "\"escape \\",
        "a: [\u{1D538}, é, \u{FEFF}x]\n",
        "{a: 1, b: [2, 3], c: {d: 4}}",
        "? |\n  block key\n: v\n",
    ];

    type Drained<'a> = Vec<ParseResult<(Event<'a>, Span)>>;

    fn corpus() -> Vec<String> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/yaml-spec");
        let mut texts: Vec<String> = fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| fs::read_to_string(entry.ok()?.path()).ok())
                    .collect()
            })
            .unwrap_or_default();
        assert!(texts.len() > 20, "fixture corpus not found");
        texts.extend(HAND_WRITTEN.iter().map(|text| (*text).to_owned()));
        texts.push(format!(
            "a: |\n  {}\nb: >\n  {}\n",
            "x".repeat(300),
            "y z ".repeat(100)
        ));
        texts.push(
            "c: |\r\n  \u{e9}\u{fc}\r\n  \u{1D538}\r\nd: \"".to_owned()
                + &"w".repeat(200)
                + "\"\r\n",
        );
        texts
    }

    fn drain(text: &str, max: MaxScanAhead) -> (GuardedParser<'_>, Drained<'_>) {
        let mut parser = GuardedParser::new(text, max);
        let mut items = Vec::new();
        while let Some(item) = parser.next_event() {
            let stop = item.is_err();
            items.push(item);
            if stop {
                break;
            }
        }
        (parser, items)
    }

    fn assert_accounting(text: &str) {
        let Ok(input) = crate::NormalizedInput::new(text) else {
            return;
        };
        let text = input.as_str();
        let (parser, items) = drain(text, MaxScanAhead::MAX);
        let chars = text.chars().count();
        assert!(parser.consumed() <= chars, "overcounted {text:?}");
        if matches!(items.last(), Some(Ok((Event::StreamEnd, _)))) {
            assert_eq!(parser.consumed(), chars, "undercounted {text:?}");
        }
    }

    #[test]
    fn consumed_equals_the_char_count_at_stream_end() {
        for text in corpus() {
            assert_accounting(&text);
        }
    }

    #[test]
    #[allow(clippy::disallowed_methods, reason = "the raw parser is the reference")]
    fn guarded_events_equal_raw_saphyr_events() {
        for text in corpus() {
            let Ok(input) = crate::NormalizedInput::new(&text) else {
                continue;
            };
            let text = input.as_str();
            let mut reference = Parser::new_from_str(text);
            let mut raw = Vec::new();
            while let Some(item) = reference.next_event() {
                let stop = item.is_err();
                raw.push(item);
                if stop {
                    break;
                }
            }
            let (_, guarded) = drain(text, MaxScanAhead::MAX);
            assert_eq!(raw.len(), guarded.len(), "{text:?}");
            for (raw, guarded) in raw.iter().zip(&guarded) {
                if let (Ok(raw), Ok(guarded)) = (raw, guarded) {
                    assert_eq!(raw, guarded, "{text:?}");
                }
            }
        }
    }

    #[test]
    fn covered_never_passes_consumed_after_non_ascii_directive_names() {
        let text = format!("%{} x\n---\n[1, 2, 3]\n--- a\n", "\u{1D538}".repeat(40));
        let mut parser = GuardedParser::new(&text, MaxScanAhead::MAX);
        while let Some(event) = parser.next_event() {
            assert!(parser.covered() <= parser.consumed());
            if event.is_err() {
                break;
            }
        }
        assert!(parser.covered() > 0);
    }

    #[test]
    fn exceeded_limit_reports_the_last_node_position() {
        let text = "a: 1\nb: 2\n[1, 2, 3, 4, 5, 6, 7, 8]\n";
        let (_, items) = drain(text, MaxScanAhead::new(4).unwrap());
        let Some(Err(ParseError::LimitExceeded {
            kind: LimitKind::ScanAhead(limit),
            line,
            document,
            ..
        })) = items.into_iter().last()
        else {
            panic!("scan-ahead error expected");
        };
        assert_eq!((limit.get(), document), (4, 0));
        assert!(line <= 3);
    }

    fn yamlish() -> impl Strategy<Value = String> {
        "[a-z0-9 :\\-\\[\\]{},#&*!|>'\"\n%.?\t\u{1D538}é]{0,120}"
    }

    proptest! {
        #[test]
        fn accounting_holds_for_generated_text(text in yamlish()) {
            assert_accounting(&text);
        }

        #[test]
        fn accounting_holds_for_non_ascii_directive_names(
            name in "[a-z\u{1D538}é]{0,40}",
            rest in yamlish(),
        ) {
            assert_accounting(&format!("%{name} x\n---\n{rest}"));
        }

        #[test]
        fn tiny_limits_never_panic_and_only_report_known_errors(
            text in yamlish(),
            limit in 1usize..=64,
        ) {
            let Ok(input) = crate::NormalizedInput::new(&text) else {
                return Ok(());
            };
            let (_, items) = drain(input.as_str(), MaxScanAhead::new(limit).unwrap());
            for item in items {
                if let Err(error) = item {
                    let known = matches!(
                        error,
                        ParseError::LimitExceeded { .. } | ParseError::Syntax(_)
                    );
                    prop_assert!(known, "{}", error);
                }
            }
        }
    }

    #[test]
    fn tiny_limits_over_the_corpus_never_panic() {
        for text in corpus() {
            let Ok(input) = crate::NormalizedInput::new(&text) else {
                continue;
            };
            for limit in [1, 2, 3, 7, 16, 64] {
                drain(input.as_str(), MaxScanAhead::new(limit).unwrap());
            }
        }
    }
}
