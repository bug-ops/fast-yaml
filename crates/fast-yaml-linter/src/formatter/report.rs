//! CI-oriented report formats that name the file each diagnostic belongs to.

use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::Diagnostic;

use super::{github, parsable};

/// Error returned when a [`ReportPath`] is built from a relative path.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("report path '{}' is not absolute", .0.display())]
pub struct NotAbsolute(PathBuf);

/// Absolute path of a linted file, as printed by the report formats.
///
/// Reports are consumed from arbitrary working directories, so every format names files by
/// absolute path. Windows verbatim prefixes (`\\?\C:\`, `\\?\UNC\`) produced by canonicalization
/// are stripped so the path matches the one the CI runner exposes. The linter performs no I/O:
/// callers canonicalize before constructing the path.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use fast_yaml_linter::formatter::ReportPath;
///
/// let path = ReportPath::from_absolute(Path::new("/work/a b.yaml"))?;
/// assert_eq!(path.file_uri(), "file:///work/a%20b.yaml");
/// assert!(ReportPath::from_absolute(Path::new("a.yaml")).is_err());
/// # Ok::<(), fast_yaml_linter::formatter::NotAbsolute>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportPath(PathBuf);

impl ReportPath {
    /// Wraps an absolute path, stripping a Windows verbatim prefix.
    ///
    /// # Errors
    ///
    /// Returns [`NotAbsolute`] when `path` is relative.
    pub fn from_absolute(path: &Path) -> Result<Self, NotAbsolute> {
        if !path.is_absolute() {
            return Err(NotAbsolute(path.to_path_buf()));
        }
        let stripped = path.to_str().and_then(strip_verbatim);
        Ok(Self(
            stripped.map_or_else(|| path.to_path_buf(), PathBuf::from),
        ))
    }

    /// Returns the path in the platform's native spelling.
    #[must_use]
    pub fn native(&self) -> &Path {
        &self.0
    }

    /// Returns the path as an RFC 3986 `file:` URI.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use fast_yaml_linter::formatter::ReportPath;
    ///
    /// let path = ReportPath::from_absolute(Path::new("/srv/ci/50%#.yaml"))?;
    /// assert_eq!(path.file_uri(), "file:///srv/ci/50%25%23.yaml");
    /// # Ok::<(), fast_yaml_linter::formatter::NotAbsolute>(())
    /// ```
    #[must_use]
    pub fn file_uri(&self) -> String {
        file_uri_of(self.0.as_os_str().as_encoded_bytes())
    }
}

impl fmt::Display for ReportPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(f)
    }
}

fn strip_verbatim(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\")?;
    if let Some(unc) = rest.strip_prefix(r"UNC\") {
        return Some(format!(r"\\{unc}"));
    }
    is_drive_path(rest.as_bytes()).then(|| rest.to_owned())
}

const fn is_drive_path(bytes: &[u8]) -> bool {
    matches!(bytes, [drive, b':', b'\\' | b'/', ..] if drive.is_ascii_alphabetic())
}

/// A leading `//` is a UNC path only on Windows; on Unix it is an ordinary absolute path.
const fn is_unc_path(bytes: &[u8]) -> bool {
    match bytes {
        [b'\\', b'\\', ..] => true,
        [b'/', b'/', ..] => cfg!(windows),
        _ => false,
    }
}

fn file_uri_of(path: &[u8]) -> String {
    let unc = is_unc_path(path);
    let windows_style = unc || is_drive_path(path);
    let normalized: Vec<u8> = path
        .iter()
        .map(|&b| if windows_style && b == b'\\' { b'/' } else { b })
        .collect();
    let mut uri = String::from("file://");
    if let (true, Some(host_and_path)) = (unc, normalized.get(2..)) {
        percent_encode_into(&mut uri, host_and_path);
    } else {
        if is_drive_path(&normalized) {
            uri.push('/');
        }
        percent_encode_into(&mut uri, &normalized);
    }
    uri
}

fn percent_encode_into(out: &mut String, bytes: &[u8]) {
    for &b in bytes {
        let keep = b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'-' | b'.'
                    | b'_'
                    | b'~'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
                    | b':'
                    | b'@'
                    | b'/'
            );
        if keep {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
}

/// Where the diagnostics of one report entry were found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportSource {
    /// Standard input, which has no path.
    Stdin,
    /// A file on disk.
    File(ReportPath),
}

/// Diagnostics of one input, ready to be rendered by a [`ReportFormat`].
#[derive(Debug, Clone, Copy)]
pub struct FileReport<'a> {
    /// The input the diagnostics belong to.
    pub source: &'a ReportSource,
    /// Diagnostics found in that input.
    pub diagnostics: &'a [Diagnostic],
}

/// Machine-readable report format for CI systems.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use fast_yaml_linter::formatter::{FileReport, ReportFormat, ReportPath, ReportSource};
/// use fast_yaml_linter::{DiagnosticBuilder, DiagnosticCode, Location, Severity, Span};
///
/// let span = Span::new(Location::new(3, 5, 20), Location::new(3, 9, 24));
/// let diagnostic =
///     DiagnosticBuilder::new(DiagnosticCode::TRUTHY, Severity::Warning, "truthy value", span)
///         .build_without_context();
/// let source = ReportSource::File(ReportPath::from_absolute(Path::new("/w/a.yaml"))?);
/// let report = FileReport { source: &source, diagnostics: &[diagnostic] };
///
/// let out = ReportFormat::Parsable.render(&[report]);
/// assert_eq!(out, "/w/a.yaml:3:5: [warning] truthy value (truthy)\n");
/// # Ok::<(), fast_yaml_linter::formatter::NotAbsolute>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReportFormat {
    /// GitHub Actions workflow commands (`::error file=...::message`).
    Github,
    /// One `path:line:col: [level] message (code)` line per diagnostic, as yamllint prints.
    Parsable,
    /// SARIF 2.1.0 JSON log.
    #[cfg(feature = "sarif-output")]
    #[cfg_attr(docsrs, doc(cfg(feature = "sarif-output")))]
    Sarif,
}

impl ReportFormat {
    /// Renders `files` in this format.
    ///
    /// The SARIF report is never empty; the line-based formats render an
    /// empty string when there is nothing to report.
    #[must_use]
    pub fn render(self, files: &[FileReport<'_>]) -> String {
        match self {
            Self::Github => github::render(files),
            Self::Parsable => parsable::render(files),
            #[cfg(feature = "sarif-output")]
            Self::Sarif => super::sarif::render(files),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(path: &str) -> String {
        file_uri_of(path.as_bytes())
    }

    #[test]
    fn unix_paths() {
        assert_eq!(uri("/a/b.yaml"), "file:///a/b.yaml");
        assert_eq!(
            uri("/a b/c#d?e[f]%.yaml"),
            "file:///a%20b/c%23d%3Fe%5Bf%5D%25.yaml"
        );
        assert_eq!(uri("/tmp/\u{e9}.yaml"), "file:///tmp/%C3%A9.yaml");
        assert_eq!(uri("/a/b\\c"), "file:///a/b%5Cc");
        assert_eq!(uri("/a\nb"), "file:///a%0Ab");
    }

    #[test]
    fn windows_drive_paths() {
        assert_eq!(uri(r"C:\a b\c.yaml"), "file:///C:/a%20b/c.yaml");
        assert_eq!(uri("D:/x/y.yaml"), "file:///D:/x/y.yaml");
    }

    #[test]
    fn windows_unc_paths() {
        assert_eq!(uri(r"\\srv\share\a.yaml"), "file://srv/share/a.yaml");
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_double_slash_is_not_unc() {
        assert_eq!(uri("//srv/a.yaml"), "file:////srv/a.yaml");
    }

    #[test]
    fn verbatim_prefixes_are_stripped() {
        assert_eq!(
            strip_verbatim(r"\\?\C:\a\b.yaml"),
            Some(r"C:\a\b.yaml".to_owned())
        );
        assert_eq!(
            strip_verbatim(r"\\?\UNC\srv\share\a"),
            Some(r"\\srv\share\a".to_owned())
        );
        assert_eq!(strip_verbatim(r"C:\a"), None);
        assert_eq!(strip_verbatim(r"\\?\Volume{1}\a"), None);
    }

    #[test]
    fn relative_path_is_rejected() {
        assert!(ReportPath::from_absolute(Path::new("a/b.yaml")).is_err());
    }
}
