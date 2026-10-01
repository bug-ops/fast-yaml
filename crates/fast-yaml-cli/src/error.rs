use std::path::PathBuf;
use thiserror::Error;

/// Exit codes for CLI application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    /// Operation completed successfully
    Success = 0,
    /// YAML parsing failed
    ParseError = 1,
    /// Linter found errors
    LintErrors = 2,
    /// I/O operation failed
    IoError = 3,
    /// Invalid command-line arguments
    InvalidArgs = 4,
    /// `format --dry-run` found files that formatting would change
    WouldChange = 5,
}

/// Why a single path cannot be used as input.
#[derive(Debug, Error)]
pub enum PathError {
    /// IO error during directory traversal
    #[error("failed to read '{path}': {source}")]
    IoError {
        /// The path that caused the error
        path: PathBuf,
        /// The underlying IO error
        #[source]
        source: std::io::Error,
    },

    /// Permission denied
    #[error("permission denied: '{path}'")]
    PermissionDenied {
        /// The path where permission was denied
        path: PathBuf,
    },

    /// Broken symbolic link
    #[error("broken symbolic link: '{path}'")]
    BrokenSymlink {
        /// The path to the broken symlink
        path: PathBuf,
    },

    /// Path does not exist
    #[error("path does not exist: '{path}'")]
    PathNotFound {
        /// The path that was not found
        path: PathBuf,
    },

    /// A path that must name a regular file does not (for example a directory on stdin)
    #[error("not a regular file: '{path}'")]
    NotAFile {
        /// The offending path
        path: PathBuf,
    },

    /// An explicitly named file is rejected by the include patterns
    #[error(
        "not matched by the include patterns (default: *.yaml, *.yml; see --include): '{path}'"
    )]
    NotIncluded {
        /// The rejected path
        path: PathBuf,
    },
}

/// Why a `--stdin-files` line was rejected.
#[derive(Debug, Error)]
pub enum StdinLineCause {
    /// The line exceeds the length limit
    #[error("line exceeds {max} bytes")]
    TooLong {
        /// The limit in bytes
        max: usize,
    },

    /// The path on the line cannot be used
    #[error(transparent)]
    Path(#[from] PathError),
}

/// Errors that can occur during file discovery.
#[derive(Debug, Error)]
pub enum DiscoveryError {
    /// Invalid globset pattern (from include/exclude patterns)
    #[error("invalid glob pattern '{pattern}': {source}")]
    InvalidPattern {
        /// The pattern that was invalid
        pattern: String,
        /// The underlying error
        #[source]
        source: globset::Error,
    },

    /// A single path cannot be used as input
    #[error(transparent)]
    Path(#[from] PathError),

    /// A `--stdin-files` line was rejected
    #[error("rejected --stdin-files line {line}")]
    StdinLine {
        /// 1-based line number
        line: usize,
        /// Why the line was rejected
        #[source]
        cause: StdinLineCause,
    },

    /// Glob pattern matched nothing
    #[error("glob pattern matched no files: '{pattern}'")]
    GlobNoMatch {
        /// The pattern that matched nothing
        pattern: String,
    },

    /// Every input was empty or filtered out by include or exclude patterns
    #[error(
        "no YAML files found: every input was empty or filtered out by include/exclude patterns"
    )]
    NoYamlFiles,

    /// Glob pattern is malformed
    #[error("invalid glob pattern '{pattern}': {source}")]
    GlobSyntax {
        /// The malformed pattern
        pattern: String,
        /// The underlying error
        #[source]
        source: glob::PatternError,
    },

    /// Batch flags were given without any input source
    #[error(
        "batch options (--jobs, --include, --exclude) need input: pass at least one path (or use --stdin-files with format)"
    )]
    NoInput,

    /// Error reading from stdin
    #[error("failed to read file list from stdin: {source}")]
    StdinError {
        /// The underlying IO error
        #[source]
        source: std::io::Error,
    },

    /// Too many paths provided
    #[error("exceeded maximum of {max} paths")]
    TooManyPaths {
        /// The maximum allowed
        max: usize,
    },
}

impl ExitCode {
    /// Converts exit code to i32 for use with `std::process::exit`
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

/// The CLI flag that raises the limit a parse error ran into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaiseHint {
    /// `--max-depth`
    MaxDepth,
    /// `--max-alias-bytes`
    MaxAliasBytes,
    /// `--max-input-bytes`
    MaxInputBytes,
    /// `--max-scan-ahead`
    MaxScanAhead,
}

impl std::fmt::Display for RaiseHint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MaxDepth => "raise with --max-depth",
            Self::MaxAliasBytes => "raise with --max-alias-bytes",
            Self::MaxInputBytes => "raise with --max-input-bytes or the max-input-bytes config key",
            Self::MaxScanAhead => "raise with --max-scan-ahead or the max-scan-ahead config key",
        })
    }
}

impl RaiseHint {
    /// Finds a raisable limit failure anywhere in the error's source chain.
    ///
    /// Tag, output and dump-node limits have no CLI flag and yield `None`, as do config file
    /// failures, which always use the default limits, and limits already at their maximum.
    #[must_use]
    pub fn of(err: &(dyn std::error::Error + 'static)) -> Option<Self> {
        #[cfg(feature = "linter")]
        if std::iter::successors(Some(err), |e| e.source())
            .any(<dyn std::error::Error>::is::<fast_yaml_linter::ConfigFileError>)
        {
            return None;
        }
        std::iter::successors(Some(err), |e| e.source()).find_map(Self::of_link)
    }

    fn of_link(err: &(dyn std::error::Error + 'static)) -> Option<Self> {
        use fast_yaml_core::limits::{
            InputTooLarge, MaxAliasBytes, MaxDepth, MaxInputBytes, MaxScanAhead,
        };
        use fast_yaml_core::{LimitKind, ParseError};
        let input_limit = |e: &InputTooLarge| {
            (e.limit.get() < MaxInputBytes::MAX.get()).then_some(Self::MaxInputBytes)
        };
        let parse_error = |e: &ParseError| match e {
            ParseError::LimitExceeded {
                kind: LimitKind::Depth(limit),
                ..
            } if limit.get() < MaxDepth::MAX.get() => Some(Self::MaxDepth),
            ParseError::LimitExceeded {
                kind: LimitKind::AliasBytes(limit) | LimitKind::AnchorCopies(limit),
                ..
            } if limit.get() < MaxAliasBytes::MAX.get() => Some(Self::MaxAliasBytes),
            ParseError::LimitExceeded {
                kind: LimitKind::ScanAhead(limit),
                ..
            } if limit.get() < MaxScanAhead::MAX.get() => Some(Self::MaxScanAhead),
            _ => None,
        };
        if let Some(e) = err.downcast_ref::<ParseError>() {
            return parse_error(e);
        }
        if let Some(fast_yaml_core::EmitError::Parse(e)) =
            err.downcast_ref::<fast_yaml_core::EmitError>()
        {
            return parse_error(e);
        }
        if let Some(e) = err.downcast_ref::<InputTooLarge>() {
            return input_limit(e);
        }
        #[cfg(feature = "linter")]
        if let Some(e) = err.downcast_ref::<fast_yaml_linter::LintError>() {
            return match e {
                fast_yaml_linter::LintError::ParseError(e) => parse_error(e),
                fast_yaml_linter::LintError::InputTooLarge(e) => input_limit(e),
                _ => None,
            };
        }
        if let Some(fast_yaml_parallel::Error::InputTooLarge(e)) = err.downcast_ref() {
            return input_limit(e);
        }
        None
    }
}

/// Format error with colored output (if enabled)
pub fn format_error(err: &anyhow::Error, use_color: bool) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    let hint = RaiseHint::of(err.as_ref());

    #[cfg(feature = "colors")]
    if use_color {
        use colored::Colorize;
        let _ = writeln!(output, "{} {}", "error:".red().bold(), err);

        // Show error chain
        for (i, cause) in err.chain().skip(1).enumerate() {
            let _ = writeln!(
                output,
                "  {}{} {}",
                "caused by".dimmed(),
                format!("[{i}]").dimmed(),
                cause.to_string().dimmed()
            );
        }
        if let Some(hint) = hint {
            let _ = writeln!(output, "  {} {hint}", "hint:".yellow());
        }
        return output;
    }

    // Fallback for no-color or when colors feature is disabled
    let _ = writeln!(output, "error: {err}");

    for (i, cause) in err.chain().skip(1).enumerate() {
        let _ = writeln!(output, "  caused by[{i}] {cause}");
    }

    if let Some(hint) = hint {
        let _ = writeln!(output, "  hint: {hint}");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_code_values() {
        assert_eq!(ExitCode::Success.as_i32(), 0);
        assert_eq!(ExitCode::ParseError.as_i32(), 1);
        assert_eq!(ExitCode::LintErrors.as_i32(), 2);
        assert_eq!(ExitCode::IoError.as_i32(), 3);
        assert_eq!(ExitCode::InvalidArgs.as_i32(), 4);
        assert_eq!(ExitCode::WouldChange.as_i32(), 5);
    }

    #[test]
    #[cfg(feature = "linter")]
    fn test_no_raise_hint_for_config_file_failures() {
        use fast_yaml_core::limits::MaxDepth;
        use fast_yaml_core::{LimitKind, ParseError};
        let error = fast_yaml_linter::ConfigFileError::Rejected {
            path: "c.yaml".into(),
            source: ParseError::LimitExceeded {
                kind: LimitKind::Depth(MaxDepth::DEFAULT),
                line: 1,
                column: 1,
                document: 0,
            },
        };
        assert_eq!(RaiseHint::of(&error), None);
    }

    #[test]
    fn test_raise_hint_for_depth_and_alias_only() {
        use fast_yaml_core::limits::{MaxAliasBytes, MaxDepth, MaxScanAhead, MaxTagBytes};
        use fast_yaml_core::{LimitKind, ParseError};
        let limit = |kind| ParseError::LimitExceeded {
            kind,
            line: 1,
            column: 1,
            document: 0,
        };
        assert_eq!(
            RaiseHint::of(&limit(LimitKind::Depth(MaxDepth::DEFAULT))),
            Some(RaiseHint::MaxDepth)
        );
        assert_eq!(
            RaiseHint::of(&limit(LimitKind::AliasBytes(MaxAliasBytes::DEFAULT))),
            Some(RaiseHint::MaxAliasBytes)
        );
        assert_eq!(RaiseHint::of(&limit(LimitKind::Depth(MaxDepth::MAX))), None);
        assert_eq!(
            RaiseHint::of(&limit(LimitKind::AliasBytes(MaxAliasBytes::MAX))),
            None
        );
        assert_eq!(
            RaiseHint::of(&limit(LimitKind::TagBytes(MaxTagBytes::DEFAULT))),
            None
        );
        assert_eq!(
            RaiseHint::of(&limit(LimitKind::ScanAhead(MaxScanAhead::DEFAULT))),
            Some(RaiseHint::MaxScanAhead)
        );
        assert_eq!(
            RaiseHint::of(&limit(LimitKind::ScanAhead(MaxScanAhead::MAX))),
            None
        );
    }

    #[test]
    fn test_raise_hint_for_input_limit_through_wrappers() {
        use fast_yaml_core::limits::{InputTooLarge, MaxInputBytes};
        let too_large = |limit| InputTooLarge { size: 99, limit };
        let small = MaxInputBytes::new(8).unwrap();
        let expected = Some(RaiseHint::MaxInputBytes);
        assert_eq!(RaiseHint::of(&too_large(small)), expected);
        assert_eq!(
            RaiseHint::of(&fast_yaml_parallel::Error::InputTooLarge(too_large(small))),
            expected
        );
        assert_eq!(RaiseHint::of(&too_large(MaxInputBytes::MAX)), None);
        assert_eq!(
            RaiseHint::of(&fast_yaml_parallel::Error::InputTooLarge(too_large(
                MaxInputBytes::MAX
            ))),
            None
        );
    }

    #[test]
    #[cfg(feature = "linter")]
    fn test_raise_hint_unwraps_lint_error() {
        use fast_yaml_core::limits::{InputTooLarge, MaxDepth, MaxInputBytes};
        use fast_yaml_core::{LimitKind, ParseError};
        use fast_yaml_linter::LintError;
        let size = InputTooLarge {
            size: 99,
            limit: MaxInputBytes::new(8).unwrap(),
        };
        assert_eq!(
            RaiseHint::of(&LintError::InputTooLarge(size)),
            Some(RaiseHint::MaxInputBytes)
        );
        let depth = ParseError::LimitExceeded {
            kind: LimitKind::Depth(MaxDepth::DEFAULT),
            line: 1,
            column: 1,
            document: 0,
        };
        assert_eq!(
            RaiseHint::of(&LintError::ParseError(depth)),
            Some(RaiseHint::MaxDepth)
        );
    }

    #[test]
    fn test_format_error_no_color() {
        let err = anyhow::anyhow!("test error");
        let formatted = format_error(&err, false);
        assert!(formatted.contains("error: test error"));
    }

    #[test]
    fn test_format_error_with_chain() {
        use anyhow::Context;
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        // Context trait applies to Result, not Error directly
        let err: anyhow::Error = Err::<(), _>(io_err)
            .context("Failed to read config")
            .unwrap_err();
        let formatted = format_error(&err, false);
        assert!(formatted.contains("Failed to read config"));
        assert!(formatted.contains("caused by"));
    }
}
