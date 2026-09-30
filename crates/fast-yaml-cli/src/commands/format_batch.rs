//! Batch format command execution.

use std::num::NonZeroUsize;
use std::path::PathBuf;

use anyhow::{Context, Result};
use fast_yaml_parallel::{
    BatchResult as ParallelBatchResult, CommentPolicy, FileProcessor, FormatOutput,
};

use crate::commands::format::error_message;
use crate::config::{CommonConfig, ParallelConfig};
use crate::discovery::FileDiscovery;
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

/// Execute batch formatting on multiple files.
pub fn execute_batch(
    common: &CommonConfig,
    target: &BatchTarget,
    write: BatchWrite,
    comments: CommentPolicy,
    max_input: MaxInputBytes,
) -> Result<ExitCode> {
    let discovery = FileDiscovery::new(target.discovery.clone())
        .context("Failed to initialize file discovery")?;

    let files = discovery
        .discover_source(&target.source)
        .context("Failed to discover files")?;

    // Only an empty --stdin-files list gets here
    if files.is_empty() {
        return Ok(ExitCode::Success);
    }

    // Create reporter
    let reporter = Reporter::new(common.output.clone());

    let file_paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();

    let emitter_config = common.formatter.to_emitter_config();

    let processor = FileProcessor::with_config(
        ParallelConfig::new()
            .with_workers(target.workers.map(NonZeroUsize::get))
            .with_max_input_size(max_input.get()),
    );

    let result = match write {
        BatchWrite::DryRun => {
            let formatted = processor.format_files(&file_paths, &emitter_config, comments);
            convert_format_results_to_batch_result(formatted)
        }
        BatchWrite::InPlace => processor.format_in_place(&file_paths, &emitter_config, comments),
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
            message: &error_message(error),
        })?;
    }

    // Return appropriate exit code
    Ok(if result.failed > 0 {
        ExitCode::ParseError
    } else if would_change > 0 {
        ExitCode::WouldChange
    } else {
        ExitCode::Success
    })
}

/// Convert `format_files` results to `BatchResult` for dry-run reporting
fn convert_format_results_to_batch_result(
    results: Vec<(PathBuf, Result<FormatOutput, fast_yaml_parallel::Error>)>,
) -> ParallelBatchResult {
    use fast_yaml_parallel::{FileOutcome, FileResult};
    use std::time::{Duration, Instant};

    let start = Instant::now();
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
    batch.duration = start.elapsed();
    batch
}
