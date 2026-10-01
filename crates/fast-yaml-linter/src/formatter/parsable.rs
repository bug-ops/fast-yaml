//! yamllint-style `parsable` report.
//!
//! yamllint knows only `error` and `warning`, so `info` and `hint` diagnostics print as `warning`.

use std::fmt::Write as _;

use crate::Severity;

use super::report::{FileReport, ReportSource};

pub(super) fn render(files: &[FileReport<'_>]) -> String {
    let mut out = String::new();
    for file in files {
        for d in file.diagnostics {
            let _ = match file.source {
                ReportSource::Stdin => write!(out, "stdin"),
                ReportSource::File(path) => write!(out, "{}", flatten(&path.to_string())),
            };
            let message = flatten(&d.message);
            let _ = writeln!(
                out,
                ":{}:{}: [{}] {} ({})",
                d.span.start.line,
                d.span.start.column,
                level(d.severity),
                message,
                d.code.as_str()
            );
        }
    }
    out
}

const fn level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning | Severity::Info | Severity::Hint => "warning",
    }
}

fn flatten(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiagnosticBuilder, Location, Severity, Span};

    #[cfg(unix)]
    #[test]
    fn newline_in_path_cannot_start_a_new_line() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 2, 1));
        let d = DiagnosticBuilder::new("r", Severity::Error, "m", span).build_without_excerpt();
        let path = std::path::Path::new("/w/a\n::error::x.yaml");
        let source = ReportSource::File(crate::formatter::ReportPath::from_absolute(path).unwrap());
        let report = FileReport {
            source: &source,
            diagnostics: &[d],
        };
        assert_eq!(render(&[report]).lines().count(), 1);
    }

    #[test]
    fn info_and_hint_print_as_warning() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(1, 2, 1));
        let diagnostics = [Severity::Info, Severity::Hint]
            .map(|s| DiagnosticBuilder::new("r", s, "m", span).build_without_excerpt());
        let report = FileReport {
            source: &ReportSource::Stdin,
            diagnostics: &diagnostics,
        };
        assert_eq!(
            render(&[report]),
            "stdin:1:1: [warning] m (r)\nstdin:1:1: [warning] m (r)\n"
        );
    }

    #[test]
    fn stdin_and_multiline_message() {
        let span = Span::new(Location::new(2, 3, 5), Location::new(2, 4, 6));
        let d =
            DiagnosticBuilder::new("rule", Severity::Error, "a\nb", span).build_without_excerpt();
        let report = FileReport {
            source: &ReportSource::Stdin,
            diagnostics: &[d],
        };
        assert_eq!(render(&[report]), "stdin:2:3: [error] a b (rule)\n");
    }
}
