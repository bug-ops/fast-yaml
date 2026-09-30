use std::time::Duration;

use anyhow::{Context, Result};
use fast_yaml_core::{Emitter, EmitterConfig, has_comments};
use fast_yaml_parallel::{CommentPolicy, Error as ParallelError};

use crate::config::CommonConfig;
use crate::error::ExitCode;
use crate::io::{InputSource, OutputWriter};
use crate::reporter::{ReportEvent, Reporter};

/// Hint appended to [`ParallelError::CommentsWouldBeStripped`] messages.
const STRIP_COMMENTS_HINT: &str = "use --strip-comments to allow this";

/// Renders a batch error for the CLI, adding the `--strip-comments` hint where it applies.
pub fn error_message(error: &ParallelError) -> String {
    match error {
        ParallelError::CommentsWouldBeStripped => format!("{error}; {STRIP_COMMENTS_HINT}"),
        _ => error.to_string(),
    }
}

/// Whether formatting altered the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatStatus {
    /// The formatted output differs from the input.
    Changed,
    /// The input is already formatted.
    Unchanged,
}

/// Format command implementation
pub struct FormatCommand {
    config: CommonConfig,
    comments: CommentPolicy,
    dry_run: bool,
}

impl FormatCommand {
    pub const fn new(config: CommonConfig, comments: CommentPolicy) -> Self {
        Self {
            config,
            comments,
            dry_run: false,
        }
    }

    /// Validates and formats the input but never writes the result.
    #[must_use]
    pub const fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Execute format command
    pub fn execute(&self, input: &InputSource, output: &OutputWriter) -> Result<FormatStatus> {
        let emitter_config = EmitterConfig::new()
            .with_indent(self.config.formatter.indent() as usize)
            .with_width(self.config.formatter.width());

        let formatted = Emitter::format_with_config(input.as_str(), &emitter_config)
            .context("Failed to format YAML")?;

        if self.comments == CommentPolicy::Reject
            && has_comments(input.as_str()).context("Failed to scan YAML for comments")?
        {
            anyhow::bail!(error_message(&ParallelError::CommentsWouldBeStripped));
        }

        let status = if formatted == input.as_str() {
            FormatStatus::Unchanged
        } else {
            FormatStatus::Changed
        };

        let rewrites_unchanged_input = status == FormatStatus::Unchanged
            && input.file_path().is_some_and(|path| output.targets(path));
        if !self.dry_run && !rewrites_unchanged_input {
            output.write(&formatted)?;
        }

        Ok(status)
    }

    /// Runs the command and, in dry-run mode, reports the summary and picks the exit code.
    ///
    /// Dry-run exits with [`ExitCode::WouldChange`] when formatting would alter the input.
    pub fn run(&self, input: &InputSource, output: &OutputWriter) -> Result<ExitCode> {
        let status = self.execute(input, output)?;
        if !self.dry_run {
            return Ok(ExitCode::Success);
        }

        let changed = status == FormatStatus::Changed;
        Reporter::new(self.config.output.clone()).report(ReportEvent::BatchSummary {
            total: 1,
            formatted: 0,
            unchanged: usize::from(!changed),
            would_change: usize::from(changed),
            failed: 0,
            duration: Duration::ZERO,
        })?;

        Ok(if changed {
            ExitCode::WouldChange
        } else {
            ExitCode::Success
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FormatterConfig;
    use crate::io::input::InputOrigin;
    use tempfile::NamedTempFile;

    fn make_cmd(comments: CommentPolicy) -> FormatCommand {
        let config = CommonConfig::new()
            .with_formatter(FormatterConfig::new().with_indent(2).with_width(80));
        FormatCommand::new(config, comments)
    }

    #[test]
    fn test_format_simple_yaml() {
        let input = InputSource {
            content: "name:    test\nvalue:   123".to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_file = NamedTempFile::new().unwrap();
        let output =
            OutputWriter::from_args(Some(temp_file.path().to_path_buf()), false, None).unwrap();

        assert!(
            make_cmd(CommentPolicy::Reject)
                .execute(&input, &output)
                .is_ok()
        );

        let formatted = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(formatted.contains("name:"));
        assert!(formatted.contains("value:"));
    }

    #[test]
    fn test_format_invalid_yaml() {
        let input = InputSource {
            content: "invalid: [".to_string(),
            origin: InputOrigin::Stdin,
        };
        let output = OutputWriter::stdout();
        assert!(
            make_cmd(CommentPolicy::Reject)
                .execute(&input, &output)
                .is_err()
        );
    }

    #[test]
    fn test_format_with_custom_indent() {
        let input = InputSource {
            content: "parent:\n  child: value".to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_file = NamedTempFile::new().unwrap();
        let output =
            OutputWriter::from_args(Some(temp_file.path().to_path_buf()), false, None).unwrap();

        let config = CommonConfig::new()
            .with_formatter(FormatterConfig::new().with_indent(4).with_width(80));
        assert!(
            FormatCommand::new(config, CommentPolicy::Reject)
                .execute(&input, &output)
                .is_ok()
        );

        let formatted = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(formatted.contains("parent:"));
    }

    #[test]
    fn test_format_with_comments_no_flag_errors() {
        let input = InputSource {
            content: "# top-level comment\nname: test".to_string(),
            origin: InputOrigin::Stdin,
        };
        let output = OutputWriter::stdout();
        let err = make_cmd(CommentPolicy::Reject)
            .execute(&input, &output)
            .unwrap_err();
        assert!(err.to_string().contains("--strip-comments"));
    }

    #[test]
    fn test_format_with_comments_strip_flag_succeeds() {
        let input = InputSource {
            content: "# top-level comment\nname: test".to_string(),
            origin: InputOrigin::Stdin,
        };
        let temp_file = NamedTempFile::new().unwrap();
        let output =
            OutputWriter::from_args(Some(temp_file.path().to_path_buf()), false, None).unwrap();
        assert!(
            make_cmd(CommentPolicy::Strip)
                .execute(&input, &output)
                .is_ok()
        );
    }
}
