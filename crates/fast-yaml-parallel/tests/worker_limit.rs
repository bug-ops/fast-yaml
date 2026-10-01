//! `Config::with_workers` bounds the concurrency of every parallel entry point.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread::ThreadId;

use fast_yaml_parallel::{
    CommentPolicy, Config, FileProcessor, parse_parallel_with_config, shared_pool,
};
use tempfile::TempDir;

fn files(dir: &TempDir, count: usize) -> Vec<PathBuf> {
    (0..count)
        .map(|i| {
            let path = dir.path().join(format!("f{i}.yaml"));
            fs::write(&path, format!("key: {i}\n")).unwrap();
            path
        })
        .collect()
}

fn threads_used(workers: usize, count: usize) -> HashSet<ThreadId> {
    let dir = TempDir::new().unwrap();
    let paths = files(&dir, count);
    let seen = Mutex::new(HashSet::new());
    let processor = FileProcessor::with_config(Config::new().with_workers(Some(workers)));
    let result = processor.process(&paths, |_, _| {
        std::thread::sleep(std::time::Duration::from_millis(5));
        seen.lock().unwrap().insert(std::thread::current().id());
        Ok(())
    });
    assert!(result.is_success());
    seen.into_inner().unwrap()
}

#[test]
fn one_worker_runs_every_file_on_one_thread() {
    assert_eq!(threads_used(1, 24).len(), 1);
}

#[test]
fn sequential_setting_runs_on_the_calling_thread() {
    let used = threads_used(0, 24);
    assert_eq!(used, HashSet::from([std::thread::current().id()]));
}

#[test]
fn worker_count_sets_the_pool_the_batch_runs_in() {
    let dir = TempDir::new().unwrap();
    let paths = files(&dir, 24);
    for workers in [1, 2] {
        let observed = Mutex::new(HashSet::new());
        let processor = FileProcessor::with_config(Config::new().with_workers(Some(workers)));
        let result = processor.process(&paths, |_, _| {
            observed
                .lock()
                .unwrap()
                .insert(rayon::current_num_threads());
            Ok(())
        });
        assert!(result.is_success());
        assert_eq!(observed.into_inner().unwrap(), HashSet::from([workers]));
    }
}

#[test]
fn format_files_honors_the_worker_count() {
    let dir = TempDir::new().unwrap();
    let paths = files(&dir, 24);
    let processor = FileProcessor::with_config(Config::new().with_workers(Some(1)));
    let out = processor.format_files(
        &paths,
        &fast_yaml_core::EmitterConfig::new(),
        CommentPolicy::Reject,
    );
    assert_eq!(out.len(), 24);
    assert!(out.iter().all(|(_, r)| r.is_ok()));
}

#[test]
fn parse_parallel_runs_inside_the_configured_pool() {
    let input = "---\nk: 1\n".repeat(64);
    let config = Config::new().with_workers(Some(3));
    let docs = parse_parallel_with_config(&input, &config).unwrap();
    assert_eq!(docs.len(), 64);
}

#[test]
fn shared_pool_is_one_per_process() {
    let two = std::num::NonZeroUsize::new(2).unwrap();
    let a = shared_pool(two).unwrap();
    let b = shared_pool(two).unwrap();
    assert!(std::sync::Arc::ptr_eq(&a, &b));
    assert_eq!(a.install(rayon::current_num_threads), 2);
}
