//! Gitignore-style path patterns behind the `ignore` and `yaml-files` config keys.
//!
//! yamllint matches both keys with `pathspec` (`gitwildmatch`). Unlike yamllint, which matches
//! `ignore` against the path relative to the working directory, `ignore` is anchored at the
//! directory of the config file, so results do not depend on where `fy` runs. `yaml-files` is
//! matched against the file name only. Matching is case-sensitive.

use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::echo::{KEY_LIMIT, MESSAGE_LIMIT, echo};

/// Most pattern lines accepted by `ignore` or `yaml-files`.
pub const MAX_PATH_PATTERNS: usize = 1024;

/// Why pattern lines could not be compiled.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidPathPattern {
    /// A line that gitignore syntax rejects.
    #[error("invalid pattern '{}': {}", echo(.pattern, KEY_LIMIT), echo(.message, MESSAGE_LIMIT))]
    Syntax {
        /// The rejected pattern line.
        pattern: String,
        /// Why it was rejected.
        message: String,
    },

    /// More lines than [`MAX_PATH_PATTERNS`].
    #[error("too many patterns: {count} lines, at most {MAX_PATH_PATTERNS} are allowed")]
    TooMany {
        /// Number of lines given.
        count: usize,
    },

    /// The patterns compile one by one but not as a set.
    #[error("patterns cannot be combined: {}", echo(.message, MESSAGE_LIMIT))]
    Combined {
        /// Why the set was rejected.
        message: String,
    },
}

fn build(root: &Path, lines: &[String]) -> Result<Gitignore, InvalidPathPattern> {
    if lines.len() > MAX_PATH_PATTERNS {
        return Err(InvalidPathPattern::TooMany { count: lines.len() });
    }
    let mut builder = GitignoreBuilder::new(root);
    for line in lines {
        builder
            .add_line(None, line)
            .map_err(|error| InvalidPathPattern::Syntax {
                pattern: line.clone(),
                message: error.to_string(),
            })?;
    }
    builder
        .build()
        .map_err(|error| InvalidPathPattern::Combined {
            message: error.to_string(),
        })
}

/// Returns whether one line is accepted by gitignore syntax.
pub fn is_valid_pattern(line: &str) -> bool {
    GitignoreBuilder::new("").add_line(None, line).is_ok()
}

/// An absolute path with symlinks resolved, the only path form [`Linter::lint_file`] accepts.
///
/// Per-rule `ignore` patterns are matched against it, so the same file is ignored no matter how
/// it was spelled on the command line.
///
/// [`Linter::lint_file`]: crate::Linter::lint_file
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::CanonicalPath;
///
/// let dir = std::env::temp_dir().canonicalize().unwrap();
/// let path = CanonicalPath::new(&dir.join("not-yet-created.yaml")).unwrap();
/// assert_eq!(path.as_path(), dir.join("not-yet-created.yaml"));
/// assert!(CanonicalPath::new(&dir.join("no-such-dir/a.yaml")).is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalPath(PathBuf);

impl CanonicalPath {
    /// Resolves `path` to an absolute path without symlinks.
    ///
    /// A file that does not exist is resolved through its directory, so a path can be named
    /// before the file is written; the directory must exist.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when neither the path nor its directory can be resolved.
    pub fn new(path: &Path) -> std::io::Result<Self> {
        match path.canonicalize() {
            Ok(canonical) => Ok(Self(canonical)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = path.file_name().ok_or(error)?;
                let parent = path
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                Ok(Self(parent.canonicalize()?.join(name)))
            }
            Err(error) => Err(error),
        }
    }

    /// Wraps a path that the caller has already canonicalized, to avoid resolving it twice.
    ///
    /// The caller guarantees that `path` came from [`Path::canonicalize`] or
    /// [`CanonicalPath::as_path`]; a path that is not canonical only makes `ignore` patterns
    /// miss.
    #[must_use]
    pub const fn assume_canonical(path: PathBuf) -> Self {
        Self(path)
    }

    /// Returns the path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// Files that the `ignore` config key excludes from linting.
///
/// Patterns are anchored at the directory of the config file, so the same file is ignored
/// no matter where `fy` runs from.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use fast_yaml_linter::config::IgnorePatterns;
///
/// let root = Path::new("/project");
/// let ignore = IgnorePatterns::new(root, &["vendor/".to_owned()]).unwrap();
/// assert!(ignore.matches(Path::new("/project/vendor/a.yaml"), false));
/// assert!(!ignore.matches(Path::new("/project/src/a.yaml"), false));
/// assert!(!ignore.matches(Path::new("/elsewhere/vendor/a.yaml"), false));
/// ```
#[derive(Debug, Clone)]
pub struct IgnorePatterns {
    root: PathBuf,
    matcher: Gitignore,
}

impl IgnorePatterns {
    /// Compiles `lines` as gitignore patterns anchored at `root`.
    ///
    /// `root` should be canonical so that it is a prefix of canonical file paths. Blank lines
    /// and `#` comments are skipped.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidPathPattern`] naming the first line gitignore syntax rejects, or when
    /// there are more than [`MAX_PATH_PATTERNS`] lines.
    pub fn new(root: &Path, lines: &[String]) -> Result<Self, InvalidPathPattern> {
        Ok(Self {
            root: root.to_owned(),
            matcher: build(root, lines)?,
        })
    }

    /// Returns whether `path` is ignored.
    ///
    /// `path` must be absolute and canonical. A path outside the root is never ignored.
    /// Parent directories are consulted too, so `vendor/` ignores everything below it, and
    /// the last matching pattern wins, so `!vendor/keep.yaml` re-includes a file.
    #[must_use]
    pub fn matches(&self, path: &Path, is_dir: bool) -> bool {
        path.strip_prefix(&self.root).is_ok_and(|relative| {
            !relative.as_os_str().is_empty()
                && self
                    .matcher
                    .matched_path_or_any_parents(relative, is_dir)
                    .is_ignore()
        })
    }

    /// Returns whether any pattern starts with `!`.
    ///
    /// Without negations a matching directory can be skipped together with its contents.
    #[must_use]
    pub fn has_negations(&self) -> bool {
        self.matcher.num_whitelists() > 0
    }
}

/// File-name patterns of the `yaml-files` config key.
///
/// Only the final path component is matched, like yamllint: `*.yaml.j2` selects templates
/// anywhere, while a pattern containing a directory such as `sub/*.j2` matches nothing.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use fast_yaml_linter::config::YamlFiles;
///
/// let files = YamlFiles::new(&["*.yaml.j2".to_owned()]).unwrap();
/// assert!(files.matches(Path::new("deploy/app.yaml.j2")));
/// assert!(!files.matches(Path::new("deploy/app.yaml")));
/// ```
#[derive(Debug, Clone)]
pub struct YamlFiles(Gitignore);

impl YamlFiles {
    /// Compiles `lines` as gitignore patterns matched against file names.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidPathPattern`] naming the first line gitignore syntax rejects, or when
    /// there are more than [`MAX_PATH_PATTERNS`] lines.
    pub fn new(lines: &[String]) -> Result<Self, InvalidPathPattern> {
        build(Path::new(""), lines).map(Self)
    }

    /// Returns whether the file name of `path` matches a pattern.
    #[must_use]
    pub fn matches(&self, path: &Path) -> bool {
        path.file_name()
            .is_some_and(|name| self.0.matched(Path::new(name), false).is_ignore())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_owned).collect()
    }

    fn ignore(text: &str) -> IgnorePatterns {
        IgnorePatterns::new(Path::new("/p"), &lines(text)).unwrap()
    }

    #[test]
    fn directory_pattern_ignores_contents() {
        let ignore = ignore("vendor/");
        assert!(ignore.matches(Path::new("/p/vendor/a.yaml"), false));
        assert!(ignore.matches(Path::new("/p/vendor/deep/a.yaml"), false));
        assert!(ignore.matches(Path::new("/p/vendor"), true));
        assert!(!ignore.matches(Path::new("/p/vendor"), false));
        assert!(!ignore.has_negations());
    }

    #[test]
    fn negation_re_includes_and_disables_pruning() {
        let ignore = ignore("vendor/\n!vendor/keep.yaml");
        assert!(ignore.has_negations());
        assert!(ignore.matches(Path::new("/p/vendor/other.yaml"), false));
        assert!(!ignore.matches(Path::new("/p/vendor/keep.yaml"), false));
    }

    #[test]
    fn patterns_are_anchored_at_root() {
        let ignore = ignore("/top.yaml\nnested/*.yaml");
        assert!(ignore.matches(Path::new("/p/top.yaml"), false));
        assert!(!ignore.matches(Path::new("/p/sub/top.yaml"), false));
        assert!(ignore.matches(Path::new("/p/nested/a.yaml"), false));
        assert!(!ignore.matches(Path::new("/q/nested/a.yaml"), false));
        assert!(!ignore.matches(Path::new("/p"), true));
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let ignore = ignore("# note\n\n*.tmp.yaml");
        assert!(ignore.matches(Path::new("/p/a.tmp.yaml"), false));
        assert!(!ignore.matches(Path::new("/p/a.yaml"), false));
    }

    #[test]
    fn matching_is_case_sensitive() {
        assert!(!ignore("Vendor/").matches(Path::new("/p/vendor/a.yaml"), false));
    }

    #[test]
    fn invalid_pattern_names_the_line() {
        let error = IgnorePatterns::new(Path::new("/p"), &lines("ok\n{a")).unwrap_err();
        assert!(matches!(&error, InvalidPathPattern::Syntax { pattern, .. } if pattern == "{a"));
        assert!(error.to_string().contains("'{a'"), "{error}");
    }

    #[test]
    fn line_count_is_capped() {
        let many: Vec<String> = (0..=MAX_PATH_PATTERNS).map(|i| format!("f{i}")).collect();
        assert_eq!(
            YamlFiles::new(&many).unwrap_err(),
            InvalidPathPattern::TooMany { count: many.len() }
        );
        assert!(YamlFiles::new(&many[..MAX_PATH_PATTERNS]).is_ok());
    }

    #[test]
    fn yaml_files_match_the_file_name_only() {
        let files = YamlFiles::new(&lines("*.yaml.j2\n.yamllint")).unwrap();
        assert!(files.matches(Path::new("a/b/app.yaml.j2")));
        assert!(files.matches(Path::new("app.yaml.j2")));
        assert!(files.matches(Path::new("dir/.yamllint")));
        assert!(!files.matches(Path::new("app.yaml")));
    }

    #[test]
    fn yaml_files_with_a_directory_match_nothing() {
        let files = YamlFiles::new(&lines("sub/*.j2")).unwrap();
        assert!(!files.matches(Path::new("sub/a.j2")));
        assert!(!files.matches(Path::new("a.j2")));
    }

    #[test]
    fn yaml_files_slashless_name_does_not_cover_a_directory() {
        let files = YamlFiles::new(&lines("config")).unwrap();
        assert!(files.matches(Path::new("config")));
        assert!(!files.matches(Path::new("config/x.yaml")));
    }

    #[test]
    fn yaml_files_are_case_sensitive() {
        let files = YamlFiles::new(&lines("*.yaml")).unwrap();
        assert!(!files.matches(Path::new("A.YAML")));
    }
}
