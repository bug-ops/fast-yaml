//! Reading and parsing of the sources of `ignore` and `ignore-from-file`, shared by the top level
//! of a config file and by per-rule entries.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use fast_yaml_core::fs::{DisplayPath, ReadFileError, read_regular_file};
use fast_yaml_core::limits::MaxInputBytes;
use fast_yaml_core::{DecodeError, decode_input_owned};
use serde_norway::Value;

use crate::config::config_file::MAX_CONFIG_FILE_BYTES;
use crate::config::path_patterns::{CacheKey, PatternBudget, PatternState, Shared};
use crate::config::{BudgetKind, InvalidPathPattern, MAX_PATH_PATTERNS, is_valid_pattern};

/// Why a config text file could not be turned into text.
#[derive(Debug, thiserror::Error)]
pub(super) enum TextReadError {
    #[error(transparent)]
    Read(#[from] ReadFileError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
}

/// Most `ignore-from-file` names accepted for one key.
pub(super) const MAX_IGNORE_FILES: usize = 32;

/// Why an `ignore-from-file` source was rejected.
#[derive(Debug, thiserror::Error)]
pub(super) enum IgnoreFileCause {
    #[error(transparent)]
    Text(#[from] TextReadError),
    #[error("more than {MAX_IGNORE_FILES} different files are listed")]
    TooManyFiles,
    #[error("more than {MAX_PATH_PATTERNS} pattern lines")]
    TooManyPatterns,
    /// The line is reported by position only, so the file's text is never echoed.
    #[error("line {line} is not a valid pattern")]
    BadPattern { line: usize },
    #[error("line {line} has too many wildcards")]
    TooComplex { line: usize },
    #[error("the patterns exceed the budget of {0}")]
    OverBudget(BudgetKind),
    #[error("the listed patterns cannot be combined")]
    Uncombinable,
}

/// An `ignore-from-file` file that could not be used.
#[derive(Debug, thiserror::Error)]
#[error("{}: {cause}", DisplayPath::new(.path))]
pub(super) struct IgnoreFileError {
    pub path: PathBuf,
    pub cause: IgnoreFileCause,
}

/// Reads a regular UTF-8 file of at most [`MAX_CONFIG_FILE_BYTES`], the only way config files and
/// the files they name are read.
pub(super) fn read_config_text(path: &Path) -> Result<String, TextReadError> {
    let max = MaxInputBytes::new(MAX_CONFIG_FILE_BYTES).unwrap_or(MaxInputBytes::DEFAULT);
    Ok(decode_input_owned(read_regular_file(path, max)?)?)
}

/// Patterns of the `ignore-from-file` files named relative to `dir`, anchored at `root`.
///
/// The files are read, charged to the budget and compiled once per load: a later call that names
/// the same files for the same root is answered from the cache of `state`.
pub(super) fn ignore_file_patterns(
    dir: &Path,
    root: &Path,
    names: &[String],
    state: &mut PatternState,
) -> Result<Shared, IgnoreFileError> {
    let key = cache_key(dir, root, names);
    if let Some(hit) = key.as_ref().and_then(|key| state.cached(key)) {
        return Ok(hit);
    }
    let lines = ignore_file_lines(dir, names, state.budget_mut())?;
    let shared = Shared::build(root, &lines).map_err(|_| IgnoreFileError {
        path: dir.to_owned(),
        cause: IgnoreFileCause::Uncombinable,
    })?;
    if let Some(key) = key {
        state.store(key, shared.clone());
    }
    Ok(shared)
}

/// The canonical files behind `names` in order, or `None` when one cannot be resolved (the
/// read then reports the failure).
fn cache_key(dir: &Path, root: &Path, names: &[String]) -> Option<CacheKey> {
    let mut distinct: HashSet<&str> = HashSet::new();
    let mut files: Vec<PathBuf> = Vec::new();
    for name in names {
        if !distinct.insert(name) {
            continue;
        }
        // Past the cap the read reports it; resolving more names would only cost syscalls
        if distinct.len() > MAX_IGNORE_FILES {
            return None;
        }
        let canonical = dir.join(name).canonicalize().ok()?;
        if !files.contains(&canonical) {
            files.push(canonical);
        }
    }
    Some((files, root.to_owned()))
}

/// Lines of the `ignore-from-file` files, named relative to `dir`.
///
/// A repeated name is read once. Lines are counted as they are read, so a list of large or
/// repeated files stops at [`MAX_PATH_PATTERNS`] lines instead of accumulating them. A line
/// that is not a valid pattern is reported as `file:line` without its text.
fn ignore_file_lines(
    dir: &Path,
    names: &[String],
    budget: &mut PatternBudget,
) -> Result<Vec<String>, IgnoreFileError> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut lines = Vec::new();
    for name in names {
        if seen.contains(name.as_str()) {
            continue;
        }
        let path = dir.join(name);
        let fail = |cause: IgnoreFileCause| IgnoreFileError {
            path: path.clone(),
            cause,
        };
        if seen.len() == MAX_IGNORE_FILES {
            return Err(fail(IgnoreFileCause::TooManyFiles));
        }
        seen.insert(name);
        let text = read_config_text(&path).map_err(|cause| fail(cause.into()))?;
        for (index, line) in text.lines().enumerate() {
            if lines.len() == MAX_PATH_PATTERNS {
                return Err(fail(IgnoreFileCause::TooManyPatterns));
            }
            budget.charge(line).map_err(|error| {
                fail(match error {
                    InvalidPathPattern::OverBudget { kind } => IgnoreFileCause::OverBudget(kind),
                    _ => IgnoreFileCause::TooComplex { line: index + 1 },
                })
            })?;
            if !is_valid_pattern(line) {
                return Err(fail(IgnoreFileCause::BadPattern { line: index + 1 }));
            }
            lines.push(line.to_owned());
        }
    }
    Ok(lines)
}

/// Why a config value is not a list of strings.
#[derive(Debug, thiserror::Error)]
pub(super) enum ListError {
    #[error("expected a list of {0}")]
    NotAList(&'static str),
    #[error("item {0} is not a string")]
    ItemNotString(usize),
}

pub(super) fn string_items(value: Value, what: &'static str) -> Result<Vec<String>, ListError> {
    let Value::Sequence(items) = value else {
        return Err(ListError::NotAList(what));
    };
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| match item {
            Value::String(text) => Ok(text),
            _ => Err(ListError::ItemNotString(index)),
        })
        .collect()
}

/// Pattern lines of an `ignore` value: a string with one pattern per line, or a list.
pub(super) fn ignore_lines(value: Value) -> Result<Vec<String>, ListError> {
    match value {
        Value::String(text) => Ok(text.lines().map(str::to_owned).collect()),
        other => string_items(other, "patterns or a string with one pattern per line"),
    }
}

/// File names of an `ignore-from-file` value: one name or a list.
pub(super) fn ignore_file_names(value: Value) -> Result<Vec<String>, ListError> {
    match value {
        Value::String(name) => Ok(vec![name]),
        other => string_items(other, "file names or one file name"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns_file(dir: &Path, lines: usize) {
        let text = (0..lines)
            .map(|i| format!("**/p{i}/*.log\n"))
            .collect::<Vec<_>>()
            .concat();
        std::fs::write(dir.join("p.txt"), text).unwrap();
    }

    #[test]
    fn the_same_file_under_different_roots_is_charged_and_compiled_separately() {
        let dir = tempfile::tempdir().unwrap();
        patterns_file(dir.path(), 1024);
        let names = vec!["p.txt".to_owned()];
        let mut state = PatternState::new();

        let first = ignore_file_patterns(dir.path(), Path::new("/r1"), &names, &mut state).unwrap();
        let second =
            ignore_file_patterns(dir.path(), Path::new("/r2"), &names, &mut state).unwrap();

        assert!(first.patterns.matches(Path::new("/r1/p1/a.log"), false));
        assert!(!first.patterns.matches(Path::new("/r2/p1/a.log"), false));
        assert!(second.patterns.matches(Path::new("/r2/p1/a.log"), false));
        // 2 x 2048 wildcards fill the budget, so a third root cannot be charged
        let third = ignore_file_patterns(dir.path(), Path::new("/r3"), &names, &mut state);
        assert!(matches!(
            third.unwrap_err().cause,
            IgnoreFileCause::OverBudget(BudgetKind::Wildcards)
        ));
    }

    #[test]
    fn the_same_file_under_the_same_root_is_charged_once() {
        let dir = tempfile::tempdir().unwrap();
        patterns_file(dir.path(), 1024);
        let names = vec!["p.txt".to_owned()];
        let mut state = PatternState::new();
        for _ in 0..10 {
            ignore_file_patterns(dir.path(), Path::new("/r"), &names, &mut state).unwrap();
        }
    }

    #[test]
    fn a_huge_list_of_repeated_names_is_resolved_quickly() {
        let dir = tempfile::tempdir().unwrap();
        patterns_file(dir.path(), 1);
        let names = vec!["p.txt".to_owned(); 200_000];
        let started = std::time::Instant::now();
        let mut state = PatternState::new();
        ignore_file_patterns(dir.path(), Path::new("/r"), &names, &mut state).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn a_huge_list_of_different_names_stops_at_the_file_cap() {
        let dir = tempfile::tempdir().unwrap();
        let names: Vec<String> = (0..200_000).map(|i| format!("missing{i}.txt")).collect();
        let started = std::time::Instant::now();
        let mut state = PatternState::new();
        let error = ignore_file_patterns(dir.path(), Path::new("/r"), &names, &mut state);
        assert!(error.is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
}
