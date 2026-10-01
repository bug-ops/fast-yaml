//! Reading and parsing of the sources of `ignore` and `ignore-from-file`, shared by the top level
//! of a config file and by per-rule entries.

use std::path::{Path, PathBuf};

use fast_yaml_core::fs::{DisplayPath, ReadFileError, read_regular_file};
use fast_yaml_core::limits::MaxInputBytes;
use fast_yaml_core::{DecodeError, decode_input_owned};
use serde_norway::Value;

use crate::config::config_file::MAX_CONFIG_FILE_BYTES;
use crate::config::{MAX_PATH_PATTERNS, is_valid_pattern};

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

/// Lines of the `ignore-from-file` files, named relative to `dir`.
///
/// A repeated name is read once. Lines are counted as they are read, so a list of large or
/// repeated files stops at [`MAX_PATH_PATTERNS`] lines instead of accumulating them. A line
/// that is not a valid pattern is reported as `file:line` without its text.
pub(super) fn ignore_file_lines(
    dir: &Path,
    names: &[String],
) -> Result<Vec<String>, IgnoreFileError> {
    let mut seen: Vec<&str> = Vec::new();
    let mut lines = Vec::new();
    for name in names {
        if seen.contains(&name.as_str()) {
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
        seen.push(name);
        let text = read_config_text(&path).map_err(|cause| fail(cause.into()))?;
        for (index, line) in text.lines().enumerate() {
            if lines.len() == MAX_PATH_PATTERNS {
                return Err(fail(IgnoreFileCause::TooManyPatterns));
            }
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
