//! GitHub Actions workflow-command report.
//!
//! GitHub shows at most 10 error, 10 warning and 10 notice annotations per step, so large
//! reports are truncated by the runner, not by this formatter.

use std::fmt::Write as _;

use crate::Severity;

use super::report::{FileReport, ReportSource};

pub(super) fn render(files: &[FileReport<'_>]) -> String {
    let mut out = String::new();
    for file in files {
        for d in file.diagnostics {
            let level = match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Info | Severity::Hint => "notice",
            };
            let _ = write!(out, "::{level} ");
            if let ReportSource::File(path) = file.source {
                let _ = write!(out, "file={},", escape_property(&path.to_string()));
            }
            let _ = write!(
                out,
                "line={},col={}",
                d.span.start.line(),
                d.span.start.column()
            );
            let _ = write!(
                out,
                ",endLine={}",
                d.span.end.line().max(d.span.start.line())
            );
            if d.span.end.line() == d.span.start.line()
                && d.span.end.column() >= d.span.start.column()
            {
                let _ = write!(out, ",endColumn={}", d.span.end.column());
            }
            let _ = writeln!(
                out,
                ",title={}::{}",
                escape_property(d.code.as_str()),
                escape_data(&d.message)
            );
        }
    }
    out
}

fn escape_data(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn escape_property(s: &str) -> String {
    escape_data(s).replace(':', "%3A").replace(',', "%2C")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiagnosticBuilder, Location, Span};
    #[cfg(unix)]
    use {crate::formatter::ReportPath, std::path::Path};

    #[cfg(unix)]
    #[test]
    fn control_characters_in_a_path_are_escaped() {
        let span = Span::new(Location::new(1, 2, 1), Location::new(1, 5, 4));
        let d = DiagnosticBuilder::new("r", Severity::Info, "m", span).build_without_excerpt();
        let source = ReportSource::File(
            ReportPath::from_absolute(Path::new("/w/a\u{1b}]0;x\u{7}.yaml")).unwrap(),
        );
        let report = FileReport {
            source: &source,
            diagnostics: &[d],
        };
        let out = render(&[report]);
        assert!(!out.trim_end().chars().any(char::is_control), "{out:?}");
        assert!(out.contains("file=/w/a\\u{1b}]0;x\\u{7}.yaml"), "{out:?}");
    }

    #[cfg(unix)]
    #[test]
    fn escapes_properties_and_data() {
        let span = Span::new(Location::new(1, 2, 1), Location::new(1, 5, 4));
        let d = DiagnosticBuilder::new("r", Severity::Info, "50%\nx", span).build_without_excerpt();
        let source =
            ReportSource::File(ReportPath::from_absolute(Path::new("/a,b/c.yaml")).unwrap());
        let report = FileReport {
            source: &source,
            diagnostics: &[d],
        };
        assert_eq!(
            render(&[report]),
            "::notice file=/a%2Cb/c.yaml,line=1,col=2,endLine=1,endColumn=5,title=r::50%25%0Ax\n"
        );
    }

    #[test]
    fn stdin_has_no_file_property() {
        let span = Span::new(Location::new(1, 1, 0), Location::new(2, 1, 4));
        let d = DiagnosticBuilder::new("r", Severity::Error, "m", span).build_without_excerpt();
        let report = FileReport {
            source: &ReportSource::Stdin,
            diagnostics: &[d],
        };
        assert_eq!(
            render(&[report]),
            "::error line=1,col=1,endLine=2,title=r::m\n"
        );
    }
}
