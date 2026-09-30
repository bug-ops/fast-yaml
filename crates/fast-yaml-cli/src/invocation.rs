//! Resolution of positional arguments into a typed execution target.
//!
//! Every path is checked against the filesystem here, so single-file and batch runs fail
//! identically on a missing path.

use std::num::NonZeroUsize;
use std::path::PathBuf;

use crate::cli::BatchArgs;
use crate::discovery::{BatchSource, DiscoveryConfig, InputPath};
use crate::error::DiscoveryError;

/// Everything a batch run needs, resolved once from the command line.
#[derive(Debug)]
pub struct BatchTarget {
    /// Where the files come from
    pub source: BatchSource,
    /// Include/exclude/recursion settings
    pub discovery: DiscoveryConfig,
    /// Explicit worker count, `None` to auto-detect
    pub workers: Option<NonZeroUsize>,
}

impl BatchTarget {
    fn new(source: BatchSource, args: &BatchArgs) -> Self {
        Self {
            source,
            discovery: args.discovery_config(),
            workers: args.workers(),
        }
    }
}

/// What a command operates on.
#[derive(Debug)]
pub enum Target {
    /// Read from stdin
    Stdin,
    /// A single existing file
    File(PathBuf),
    /// Multiple files, directories, or glob patterns
    Batch(BatchTarget),
}

impl Target {
    /// Classifies the positional arguments and batch flags.
    ///
    /// Batch mode is selected by `--stdin-files`, several paths, a directory or glob path, or any
    /// batch-only flag.
    ///
    /// # Errors
    ///
    /// Returns an error if a path does not exist and is not a glob pattern.
    pub fn resolve(
        paths: Vec<PathBuf>,
        stdin_files: bool,
        args: &BatchArgs,
    ) -> Result<Self, DiscoveryError> {
        if stdin_files {
            return Ok(Self::Batch(BatchTarget::new(BatchSource::StdinList, args)));
        }

        let resolved = paths
            .into_iter()
            .map(InputPath::resolve)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(match (args.requests_batch(), resolved.as_slice()) {
            (false, []) => Self::Stdin,
            (false, [InputPath::File(path)]) => Self::File(path.clone()),
            _ => Self::Batch(BatchTarget::new(BatchSource::Paths(resolved), args)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn args(include: &[&str], jobs: usize) -> BatchArgs {
        BatchArgs {
            include: include.iter().map(|s| (*s).to_string()).collect(),
            exclude: vec![],
            no_recursive: false,
            jobs,
        }
    }

    fn touch(dir: &TempDir, name: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, "a: 1\n").unwrap();
        path
    }

    #[test]
    fn test_no_paths_is_stdin() {
        let target = Target::resolve(vec![], false, &args(&[], 0)).unwrap();
        assert!(matches!(target, Target::Stdin));
    }

    #[test]
    fn test_single_file_is_file() {
        let dir = TempDir::new().unwrap();
        let file = touch(&dir, "a.yaml");
        let target = Target::resolve(vec![file.clone()], false, &args(&[], 0)).unwrap();
        assert!(matches!(target, Target::File(p) if p == file));
    }

    #[test]
    fn test_multiple_paths_is_batch() {
        let dir = TempDir::new().unwrap();
        let paths = vec![touch(&dir, "a.yaml"), touch(&dir, "b.yaml")];
        let target = Target::resolve(paths, false, &args(&[], 0)).unwrap();
        assert!(matches!(target, Target::Batch(_)));
    }

    #[test]
    fn test_directory_and_glob_are_batch() {
        let dir = TempDir::new().unwrap();
        let target = Target::resolve(vec![dir.path().to_path_buf()], false, &args(&[], 0)).unwrap();
        assert!(matches!(target, Target::Batch(_)));

        let glob = dir.path().join("*.yaml");
        let target = Target::resolve(vec![glob], false, &args(&[], 0)).unwrap();
        assert!(matches!(target, Target::Batch(_)));
    }

    #[test]
    fn test_batch_flags_force_batch() {
        let dir = TempDir::new().unwrap();
        let file = touch(&dir, "a.yaml");
        let target = Target::resolve(vec![file.clone()], false, &args(&["*.yml"], 0)).unwrap();
        assert!(matches!(target, Target::Batch(_)));

        let target = Target::resolve(vec![file], false, &args(&[], 2)).unwrap();
        assert!(matches!(&target, Target::Batch(b) if b.workers.is_some()));
    }

    #[test]
    fn test_stdin_files_is_batch_stdin_list() {
        let target = Target::resolve(vec![], true, &args(&[], 0)).unwrap();
        assert!(matches!(
            target,
            Target::Batch(BatchTarget {
                source: BatchSource::StdinList,
                ..
            })
        ));
    }

    #[test]
    fn test_missing_path_errors_in_single_and_batch() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("nonexist.yaml");
        let clean = touch(&dir, "clean.yaml");

        let single = Target::resolve(vec![missing.clone()], false, &args(&[], 0)).unwrap_err();
        assert!(single.to_string().contains("path does not exist"));

        let batch = Target::resolve(vec![missing, clean], false, &args(&[], 0)).unwrap_err();
        assert!(batch.to_string().contains("path does not exist"));
    }
}
