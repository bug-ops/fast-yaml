//! Peak memory of hostile inputs under the scan-ahead limit (#563).
//!
//! Every shape below makes the saphyr scanner buffer tokens for far more input than it reports
//! as events; the limit must reject it while peak memory stays near `190 x limit`.

use std::sync::{Mutex, MutexGuard, PoisonError};

use fast_yaml_core::{LimitKind, MaxScanAhead, NormalizedInput, ParseError, ParseLimits, Parser};
use peak_alloc::PeakAlloc;

#[global_allocator]
static PEAK: PeakAlloc = PeakAlloc;

static SERIAL: Mutex<()> = Mutex::new(());

/// The allocator counts the whole process, so tests that measure must not overlap.
fn exclusive() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Heap bytes per character of scan-ahead: 376 measured with this allocator on `[1,1,..]`
/// (about 200 as resident memory), rounded up.
const PEAK_PER_CHAR: usize = 400;
/// Shapes the scanner may reject as a syntax error before the limit applies.
const SYNTAX_ERROR_SHAPES: [&str; 2] = [
    "document marker inside flow",
    "adjacent quoted scalars in flow",
];
const LIMIT: usize = 256 * 1024;
const PAYLOAD: usize = 5 * 1024 * 1024;

fn limits(max: usize) -> ParseLimits {
    ParseLimits {
        max_scan_ahead: MaxScanAhead::new(max).unwrap(),
        ..ParseLimits::default()
    }
}

/// Runs the parse and returns its result with the peak heap growth it caused.
fn measured(input: &str, limits: &ParseLimits) -> (Result<(), ParseError>, usize) {
    PEAK.reset_peak_usage();
    let before = PEAK.current_usage();
    let result = Parser::parse_all_with_limits(input, limits).map(drop);
    (result, PEAK.peak_usage().saturating_sub(before))
}

fn items(bytes: usize) -> String {
    "1,".repeat(bytes / 2)
}

const fn is_scan_ahead(result: &Result<(), ParseError>) -> bool {
    matches!(
        result,
        Err(ParseError::LimitExceeded {
            kind: LimitKind::ScanAhead(_),
            ..
        })
    )
}

#[test]
fn hostile_shapes_are_rejected_within_the_memory_bound() {
    let _serial = exclusive();
    let huge = items(PAYLOAD);
    let shapes: Vec<(&str, String)> = vec![
        ("root flow", format!("[{huge}1]")),
        (
            "root flow mapping",
            format!("{{{}}}", "a: 1, ".repeat(PAYLOAD / 6)),
        ),
        ("dash entry flow", format!("- [{huge}1]")),
        ("nested in flow mapping", format!("{{a: [{huge}1]}}")),
        ("document start nested flow", format!("--- [[{huge}1]]")),
        ("a: nested flow", format!("a: [[{huge}1]]")),
        (
            "literal block, later sibling",
            format!("- a: |\n    text\n  b:\n  - [{huge}1]\n"),
        ),
        (
            "literal block, empty body",
            format!("- a: |\n  b:\n  - [{huge}1]\n"),
        ),
        (
            "literal block, flow value",
            format!("- a: |\n  b: [[{huge}1]]\n"),
        ),
        (
            "document marker inside flow",
            format!("[1,\n---\n,{huge}1]"),
        ),
        (
            "plain continuation with quote",
            format!("a: foo\n  'x\nb:\n- [{huge}1]\nc: 'z'\n"),
        ),
        ("tab separated mapping value", format!("a:\t[[{huge}1]]")),
        ("tab separated entry", format!("-\t[{huge}1]")),
        (
            "dash continuation with quote",
            format!("- foo\n 'x\n- [{huge}1]\n- 'z'\n"),
        ),
        ("minified json", format!("{{\"a\":\"]\",\"b\":[{huge}1]}}")),
        (
            "root plain then quote across marker",
            format!("foo\n'x\n---\n[{huge}1]\n--- 'z'\n"),
        ),
        ("anchor with quote", format!("- &a\"b [{huge}1]")),
        (
            "adjacent quoted scalars in flow",
            format!("[{}]", "\"a\"\"b\",".repeat(PAYLOAD / 8)),
        ),
        ("unterminated quote", format!("'[{huge}")),
        ("long comment run", "#c\n".repeat(PAYLOAD / 3)),
        ("single scalar", format!("a: {}", "x".repeat(PAYLOAD))),
    ];
    for (name, input) in &shapes {
        let (result, peak) = measured(input, &limits(LIMIT));
        let bound = PEAK_PER_CHAR * LIMIT + 4 * input.len();
        assert!(peak < bound, "{name}: peak {peak} exceeds {bound}");
        if SYNTAX_ERROR_SHAPES.contains(name) {
            assert!(result.is_err(), "{name}: {result:?}");
        } else {
            assert!(is_scan_ahead(&result), "{name}: {result:?}");
        }
    }
}

#[test]
fn non_ascii_directive_name_cannot_hide_a_large_flow_collection() {
    let _serial = exclusive();
    let directive = "\u{1D538}".repeat(300_000);
    let input = format!("%{directive} x\n---\n[{}1]\n", items(1_500_000));
    let (result, peak) = measured(&input, &limits(1024 * 1024));
    assert!(is_scan_ahead(&result), "{result:?}");
    assert!(
        peak < PEAK_PER_CHAR * 1024 * 1024 + 4 * input.len(),
        "peak {peak}"
    );
}

#[test]
fn streaming_shapes_pass_under_a_small_limit() {
    let _serial = exclusive();
    let huge = items(1024 * 1024);
    for input in [format!("a: [{huge}1]"), format!("--- [{huge}1]\n")] {
        let (result, peak) = measured(&input, &limits(64 * 1024));
        assert!(result.is_ok(), "{result:?}");
        assert!(peak < 100 * input.len(), "peak {peak}");
    }
}

#[test]
fn one_large_scalar_needs_a_raised_limit() {
    let _serial = exclusive();
    let input = format!("a: {}\n", "x".repeat(PAYLOAD));
    let (rejected, _) = measured(&input, &limits(4 * 1024 * 1024));
    assert!(is_scan_ahead(&rejected), "{rejected:?}");
    let (accepted, _) = measured(&input, &limits(8 * 1024 * 1024));
    assert!(accepted.is_ok(), "{accepted:?}");
}

#[test]
fn raised_limit_accepts_a_large_root_flow_collection() {
    let _serial = exclusive();
    let input = format!("[{}1]", items(3 * 1024 * 1024));
    let (result, peak) = measured(&input, &limits(4 * 1024 * 1024));
    assert!(result.is_ok(), "{result:?}");
    assert!(peak < PEAK_PER_CHAR * input.len(), "peak {peak}");
}

#[test]
fn overrun_in_a_later_document_is_positioned() {
    let _serial = exclusive();
    let input = format!("a: 1\n---\nb: 2\n---\n[{}1]\n", items(10_000));
    let Err(ParseError::LimitExceeded {
        kind: LimitKind::ScanAhead(_),
        line,
        document,
        ..
    }) = Parser::parse_all_with_limits(&input, &limits(1024))
    else {
        panic!("scan-ahead error expected");
    };
    assert_eq!(document, 2);
    assert!(line >= 4, "line {line}");
}

/// Peak heap growth of normalizing `input`, with the result.
fn normalized(input: &str) -> (Result<(), ParseError>, usize) {
    PEAK.reset_peak_usage();
    let before = PEAK.current_usage();
    let result = NormalizedInput::new(input).map(drop);
    (result, PEAK.peak_usage().saturating_sub(before))
}

#[test]
fn invalid_character_after_a_huge_flow_collection_is_found_within_a_small_budget() {
    let _serial = exclusive();
    for tail in ["\x01", "\x7F"] {
        let input = format!("[{}1]\n{tail}", items(4 * 1024 * 1024));
        let (result, peak) = normalized(&input);
        assert!(matches!(result, Err(ParseError::Syntax(_))), "{result:?}");
        assert!(
            peak < PEAK_PER_CHAR * 64 * 1024 + 4 * input.len(),
            "peak {peak}"
        );
    }
}

#[test]
fn invalid_character_reports_the_document_after_a_huge_flow_collection() {
    let _serial = exclusive();
    let input = format!("a\n---\n- [{}1]\n--- b\n--- c\x01", items(1024 * 1024));
    let (result, _) = normalized(&input);
    let Err(error) = result else {
        panic!("syntax error expected");
    };
    assert_eq!(error.document_index(), 3, "{error}");
}

#[test]
fn invalid_character_in_a_small_input_keeps_its_exact_document() {
    let error = NormalizedInput::new("a: 1\n---\nb: 2\n---\nc\x01").unwrap_err();
    assert_eq!(error.document_index(), 2);
}
