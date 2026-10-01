//! `ScanAheadPolicy::Scaled` gives every worker the same outcome as a single-file run (#577).

use std::fs;
use std::path::PathBuf;

use fast_yaml_core::EmitterConfig;
use fast_yaml_core::limits::MaxScanAhead;
use fast_yaml_core::{LimitKind, ParseError, Parser};
use fast_yaml_parallel::{CommentPolicy, Config, FileProcessor, ScanAheadLane, ScanAheadPolicy};
use tempfile::TempDir;

/// About 1.8 Mi characters: over a 1 Mi scaled limit, under the 4 Mi default.
fn between_limits() -> String {
    format!("[{}1]\n", "1, ".repeat(600_000))
}

fn batch(dir: &TempDir) -> Vec<PathBuf> {
    let mut paths = vec![dir.path().join("adversarial.yaml")];
    fs::write(&paths[0], between_limits()).unwrap();
    for i in 0..11 {
        let path = dir.path().join(format!("small{i}.yaml"));
        fs::write(&path, format!("k: {i}\n")).unwrap();
        paths.push(path);
    }
    paths
}

fn formatted_ok(workers: usize, policy: ScanAheadPolicy, emitter: &EmitterConfig) -> Vec<bool> {
    let dir = TempDir::new().unwrap();
    let paths = batch(&dir);
    let processor = FileProcessor::with_config(
        Config::new()
            .with_workers(Some(workers))
            .with_scan_ahead_policy(policy),
    );
    processor
        .format_files(&paths, emitter, CommentPolicy::Reject)
        .into_iter()
        .map(|(_, result)| result.is_ok())
        .collect()
}

#[test]
fn scaled_batch_matches_single_file_for_every_worker_count() {
    let emitter = EmitterConfig::new();
    for workers in [1, 2, 8, 64] {
        let outcome = formatted_ok(workers, ScanAheadPolicy::Scaled, &emitter);
        assert!(
            outcome.iter().all(|ok| *ok),
            "workers {workers}: {outcome:?}"
        );
    }
}

#[test]
fn a_file_over_the_full_limit_is_rejected_under_both_policies() {
    let mut emitter = EmitterConfig::new();
    emitter.parse_limits.max_scan_ahead = MaxScanAhead::new(1 << 20).unwrap();
    for policy in [ScanAheadPolicy::Fixed, ScanAheadPolicy::Scaled] {
        let outcome = formatted_ok(8, policy, &emitter);
        assert!(!outcome[0], "{policy:?}: the file is over the full limit");
        assert!(outcome[1..].iter().all(|ok| *ok), "{policy:?}");
    }
}

#[test]
fn a_file_between_the_limits_is_rejected_first_and_accepted_through_the_lane() {
    let text = between_limits();
    let lane = ScanAheadLane::scaled(
        MaxScanAhead::DEFAULT,
        std::num::NonZeroUsize::new(8).unwrap(),
    );
    assert!(lane.first_limit().get() < MaxScanAhead::DEFAULT.get());

    let attempts = std::sync::Mutex::new(Vec::new());
    let result = lane.run(
        |limit| {
            attempts.lock().unwrap().push(limit);
            let limits = fast_yaml_core::limits::ParseLimits {
                max_scan_ahead: limit,
                ..fast_yaml_core::limits::ParseLimits::default()
            };
            Parser::parse_str_with_limits(&text, &limits)
        },
        |error| {
            matches!(
                error,
                ParseError::LimitExceeded {
                    kind: LimitKind::ScanAhead(_),
                    ..
                }
            )
        },
    );

    assert!(result.is_ok());
    assert_eq!(
        *attempts.lock().unwrap(),
        [lane.first_limit(), MaxScanAhead::DEFAULT]
    );
}
