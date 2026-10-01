//! Reading and parsing of the sources of `ignore` and `ignore-from-file`, shared by the top level
//! of a config file and by per-rule entries.

use std::path::{Path, PathBuf};

use fast_yaml_core::fs::{ReadFileError, read_regular_file};
use fast_yaml_core::limits::MaxInputBytes;
use fast_yaml_core::{DecodeError, decode_input_owned};
use serde_norway::Value;

use crate::config::config_file::MAX_CONFIG_FILE_BYTES;

/// Why a config text file could not be turned into text.
#[derive(Debug, thiserror::Error)]
pub(super) enum TextReadError {
    #[error(transparent)]
    Read(#[from] ReadFileError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
}

/// An `ignore-from-file` file that could not be read.
#[derive(Debug, thiserror::Error)]
#[error("{}: {cause}", .path.display())]
pub(super) struct IgnoreFileError {
    pub path: PathBuf,
    pub cause: TextReadError,
}

/// Reads a regular UTF-8 file of at most [`MAX_CONFIG_FILE_BYTES`], the only way config files and
/// the files they name are read.
pub(super) fn read_config_text(path: &Path) -> Result<String, TextReadError> {
    let max = MaxInputBytes::new(MAX_CONFIG_FILE_BYTES).unwrap_or(MaxInputBytes::DEFAULT);
    Ok(decode_input_owned(read_regular_file(path, max)?)?)
}

/// Lines of the `ignore-from-file` files, named relative to `dir`.
pub(super) fn ignore_file_lines(
    dir: &Path,
    names: &[String],
) -> Result<Vec<String>, IgnoreFileError> {
    let mut lines = Vec::new();
    for name in names {
        let path = dir.join(name);
        let text = read_config_text(&path).map_err(|cause| IgnoreFileError {
            path: path.clone(),
            cause,
        })?;
        lines.extend(text.lines().map(str::to_owned));
    }
    Ok(lines)
}

pub(super) fn string_items(value: Value, what: &str) -> Result<Vec<String>, String> {
    let Value::Sequence(items) = value else {
        return Err(format!("expected a list of {what}"));
    };
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| match item {
            Value::String(text) => Ok(text),
            _ => Err(format!("item {index} is not a string")),
        })
        .collect()
}

/// Pattern lines of an `ignore` value: a string with one pattern per line, or a list.
pub(super) fn ignore_lines(value: Value) -> Result<Vec<String>, String> {
    match value {
        Value::String(text) => Ok(text.lines().map(str::to_owned).collect()),
        other => string_items(other, "patterns or a string with one pattern per line"),
    }
}

/// File names of an `ignore-from-file` value: one name or a list.
pub(super) fn ignore_file_names(value: Value) -> Result<Vec<String>, String> {
    match value {
        Value::String(name) => Ok(vec![name]),
        other => string_items(other, "file names or one file name"),
    }
}
