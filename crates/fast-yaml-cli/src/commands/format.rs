use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use fast_yaml_core::{Emitter, NormalizedInput, has_comments_normalized};
use fast_yaml_parallel::{CommentPolicy, Error as ParallelError};

use crate::config::CommonConfig;
use crate::error::ExitCode;
use crate::io::{InputSource, OutputWriter, WriteTarget};
use crate::reporter::{BatchStats, ReportEvent, Reporter};

/// Hint appended to [`ParallelError::CommentsWouldBeStripped`] messages.
const STRIP_COMMENTS_HINT: &str = "use --strip-comments to allow this";

/// Renders a batch error for the CLI, adding the `--strip-comments` hint where it applies.
///
/// The reporter already prefixes the file path, so format errors omit it.
pub fn error_message(error: &ParallelError) -> String {
    match error {
        ParallelError::CommentsWouldBeStripped => format!("{error}; {STRIP_COMMENTS_HINT}"),
        ParallelError::Format { source, .. } => source.to_string(),
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

/// How the formatted output leaves the command.
#[derive(Debug)]
pub enum WriteMode {
    /// Write the formatted YAML to the destination
    Emit(OutputWriter),
    /// Never write; report a summary and signal changes through the exit code
    DryRun,
}

/// What the user asked `fy format` to do with the result, resolved once from the CLI flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditIntent {
    /// `--dry-run`: report only, never write
    Preview,
    /// Write to the destination the write flags name
    Write(WriteTarget),
}

impl EditIntent {
    /// Resolves the flags; `--dry-run` wins over `-i`.
    #[must_use]
    pub fn resolve(dry_run: bool, target: WriteTarget) -> Self {
        if dry_run {
            Self::Preview
        } else {
            Self::Write(target)
        }
    }
}

impl WriteMode {
    /// Picks [`WriteMode::DryRun`] for a preview, otherwise builds the output writer.
    ///
    /// # Errors
    ///
    /// Returns an error if the intent writes in place and there is no input file.
    pub fn new(intent: EditIntent, input_file: Option<&Path>) -> Result<Self> {
        match intent {
            EditIntent::Preview => Ok(Self::DryRun),
            EditIntent::Write(target) => {
                Ok(Self::Emit(OutputWriter::for_write(target, input_file)?))
            }
        }
    }
}

/// Format command implementation
pub struct FormatCommand {
    config: CommonConfig,
    comments: CommentPolicy,
}

impl FormatCommand {
    pub const fn new(config: CommonConfig, comments: CommentPolicy) -> Self {
        Self { config, comments }
    }

    /// Formats the input and either writes it or, in dry-run mode, reports the summary.
    ///
    /// Dry-run exits with [`ExitCode::WouldChange`] when formatting would alter the input.
    pub fn run(&self, input: &InputSource, mode: &WriteMode) -> Result<ExitCode> {
        let (formatted, status) = self.format(input)?;

        match mode {
            WriteMode::Emit(output) => {
                let rewrites_unchanged_input = status == FormatStatus::Unchanged
                    && input.file_path().is_some_and(|path| output.targets(path));
                if !rewrites_unchanged_input {
                    output.write(&formatted)?;
                }
                Ok(ExitCode::Success)
            }
            WriteMode::DryRun => {
                let changed = status == FormatStatus::Changed;
                Reporter::new(self.config.output.clone()).report(ReportEvent::BatchSummary(
                    BatchStats {
                        total: 1,
                        formatted: 0,
                        unchanged: usize::from(!changed),
                        would_change: usize::from(changed),
                        failed: 0,
                        duration: Duration::ZERO,
                    },
                ))?;

                Ok(if changed {
                    ExitCode::WouldChange
                } else {
                    ExitCode::Success
                })
            }
        }
    }

    fn format(&self, input: &InputSource) -> Result<(String, FormatStatus)> {
        let normalized = NormalizedInput::new(input.as_str()).context("Failed to format YAML")?;
        let emitter_config = self.config.formatter.to_emitter_config();
        let formatted = Emitter::format_normalized(&normalized, &emitter_config)
            .context("Failed to format YAML")?;

        if self.comments == CommentPolicy::Reject
            && has_comments_normalized(&normalized, emitter_config.parse_limits.max_scan_ahead)
                .context("Failed to scan YAML for comments")?
        {
            anyhow::bail!(error_message(&ParallelError::CommentsWouldBeStripped));
        }

        let status = if formatted == input.as_str() {
            FormatStatus::Unchanged
        } else {
            FormatStatus::Changed
        };
        Ok((formatted, status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FormatterConfig;
    use crate::io::OutputTarget;
    use crate::io::input::InputOrigin;
    use fast_yaml_core::Indent;
    use tempfile::NamedTempFile;

    fn make_cmd(comments: CommentPolicy) -> FormatCommand {
        let config = CommonConfig::new().with_formatter(FormatterConfig::new());
        FormatCommand::new(config, comments)
    }

    #[test]
    fn test_format_simple_yaml() {
        let input = InputSource {
            content: "name:    test\nvalue:   123".to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_file = NamedTempFile::new().unwrap();
        let output = OutputWriter::new(OutputTarget::File(temp_file.path().to_path_buf()));

        assert!(
            make_cmd(CommentPolicy::Reject)
                .run(&input, &WriteMode::Emit(output))
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
                .run(&input, &WriteMode::Emit(output))
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
        let output = OutputWriter::new(OutputTarget::File(temp_file.path().to_path_buf()));

        let config = CommonConfig::new()
            .with_formatter(FormatterConfig::new().with_indent(Indent::new(4).unwrap()));
        assert!(
            FormatCommand::new(config, CommentPolicy::Reject)
                .run(&input, &WriteMode::Emit(output))
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
            .run(&input, &WriteMode::Emit(output))
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
        let output = OutputWriter::new(OutputTarget::File(temp_file.path().to_path_buf()));
        assert!(
            make_cmd(CommentPolicy::Strip)
                .run(&input, &WriteMode::Emit(output))
                .is_ok()
        );
    }
}
