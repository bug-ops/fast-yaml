//! Batch format command execution.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use fast_yaml_parallel::{
    AtomicWriteError, BatchResult as ParallelBatchResult, CommentPolicy, Error as ParallelError,
    FileProcessor, FormatOutput, HardLinkPolicy, InPlaceTarget, ScanAheadPolicy,
};

use crate::commands::format::error_message;
use crate::config::{CommonConfig, FormatterConfig, ParallelConfig};
use crate::discovery::{DiscoveryOrigin, FileDiscovery};
use crate::error::ExitCode;
use crate::invocation::BatchTarget;
use crate::reporter::{BatchStats, ReportEvent, Reporter};
use fast_yaml_core::limits::MaxInputBytes;

/// What a batch format run does with the formatted files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchWrite {
    /// Rewrite files that change
    InPlace,
    /// Report what would change without writing
    DryRun,
}

const HARD_LINK_HINT: &str = "found by a directory walk or glob; name the file as a path argument to write through its links";

/// Files named by the user are written through their hard links; files found by a walk or glob
/// are refused so that an unrelated name for the same inode is never rewritten silently.
const fn hard_link_policy(origin: DiscoveryOrigin) -> HardLinkPolicy {
    match origin {
        DiscoveryOrigin::DirectPath | DiscoveryOrigin::StdinList => HardLinkPolicy::WriteInPlace,
        DiscoveryOrigin::DirectoryWalk | DiscoveryOrigin::GlobExpansion => HardLinkPolicy::Refuse,
    }
}

fn batch_error_message(error: &ParallelError) -> String {
    let message = error_message(error);
    match error {
        ParallelError::Write { source, .. }
            if matches!(
                source
                    .get_ref()
                    .and_then(|e| e.downcast_ref::<AtomicWriteError>()),
                Some(AtomicWriteError::HardLinkRefused(_))
            ) =>
        {
            format!("{message}; {HARD_LINK_HINT}")
        }
        _ => message,
    }
}

/// Execute batch formatting on multiple files.
pub fn execute_batch(
    common: &CommonConfig,
    format_config: &FormatterConfig,
    target: &BatchTarget,
    write: BatchWrite,
    comments: CommentPolicy,
    max_input: MaxInputBytes,
    scan_ahead: ScanAheadPolicy,
) -> Result<ExitCode> {
    let discovery = FileDiscovery::new(target.discovery.clone())
        .context("Failed to initialize file discovery")?;

    let files = discovery
        .discover_source(&target.source)
        .context("Failed to discover files")?;
    tracing::debug!(
        files = files.len(),
        workers = target.workers.threads().get(),
        "discovered files for a format batch"
    );

    // Only an empty --stdin-files list gets here
    if files.is_empty() {
        return Ok(ExitCode::Success);
    }

    // Create reporter
    let reporter = Reporter::new(common.output.clone());

    let file_paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();

    let emitter_config = format_config.to_emitter_config();

    let processor = FileProcessor::with_config(
        ParallelConfig::new()
            .with_workers(target.workers)
            .with_max_input_bytes(max_input)
            .with_scan_ahead_policy(scan_ahead),
    );

    let result = match write {
        BatchWrite::DryRun => {
            let start = Instant::now();
            let formatted = processor.format_files(&file_paths, &emitter_config, comments);
            convert_format_results_to_batch_result(formatted, start.elapsed())
        }
        BatchWrite::InPlace => {
            let targets: Vec<InPlaceTarget> = files
                .iter()
                .map(|file| InPlaceTarget::new(file.path.clone(), hard_link_policy(file.origin)))
                .collect();
            processor.format_in_place(&targets, &emitter_config, comments)
        }
    };

    // In dry-run mode, 'changed' means "would change"; in in-place mode it means "formatted".
    let (would_change, formatted) = match write {
        BatchWrite::DryRun => (result.changed, 0),
        BatchWrite::InPlace => (0, result.changed),
    };

    reporter.report(ReportEvent::BatchSummary(BatchStats {
        total: result.total,
        formatted,
        unchanged: result.success - result.changed,
        would_change,
        failed: result.failed,
        duration: result.duration,
    }))?;

    // Report errors
    for (path, error) in &result.errors {
        reporter.report(ReportEvent::Error {
            path: Some(path),
            message: &batch_error_message(error),
        })?;
    }

    // Return appropriate exit code
    Ok(if result.failed > 0 {
        ExitCode::Failure
    } else if would_change > 0 {
        ExitCode::WouldChange
    } else {
        ExitCode::Success
    })
}

/// Convert `format_files` results to `BatchResult` for dry-run reporting
fn convert_format_results_to_batch_result(
    results: Vec<(PathBuf, Result<FormatOutput, fast_yaml_parallel::Error>)>,
    duration: Duration,
) -> ParallelBatchResult {
    use fast_yaml_parallel::{FileOutcome, FileResult};

    let mut file_results = Vec::with_capacity(results.len());

    for (path, result) in results {
        let outcome = match result {
            Ok(output) if output.changed => FileOutcome::Changed {
                duration: Duration::ZERO,
            },
            Ok(_) => FileOutcome::Unchanged {
                duration: Duration::ZERO,
            },
            Err(error) => FileOutcome::Error {
                error,
                duration: Duration::ZERO,
            },
        };
        file_results.push(FileResult::new(path, outcome));
    }

    let mut batch = ParallelBatchResult::from_results(file_results);
    batch.duration = duration;
    batch
}
