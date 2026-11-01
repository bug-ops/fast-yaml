//! Gitignore-style path patterns behind the `ignore` and `yaml-files` config keys.
//!
//! yamllint matches both keys with `pathspec` (`gitwildmatch`). Unlike yamllint, which matches
//! `ignore` against the path relative to the working directory, `ignore` is anchored at the
//! directory of the config file, so results do not depend on where `fy` runs. `yaml-files` is
//! matched against the file name only. Matching is case-sensitive.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::echo::{KEY_LIMIT, MESSAGE_LIMIT, echo};

/// Most pattern lines accepted by `ignore` or `yaml-files`.
pub const MAX_PATH_PATTERNS: usize = 1024;

/// Most wildcards (`*` runs, `?` and `[`) accepted in one pattern line.
pub const MAX_PATTERN_WILDCARDS: usize = 16;

const BUDGET_LINES: usize = 4096;
const BUDGET_BYTES: usize = 256 * 1024;
const BUDGET_WILDCARDS: usize = 4096;

/// The resource a pattern budget ran out of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BudgetKind {
    /// Pattern lines.
    Lines,
    /// Bytes of pattern text.
    Bytes,
    /// Wildcards over all patterns.
    Wildcards,
}

impl BudgetKind {
    const fn limit(self) -> usize {
        match self {
            Self::Lines => BUDGET_LINES,
            Self::Bytes => BUDGET_BYTES,
            Self::Wildcards => BUDGET_WILDCARDS,
        }
    }

    const fn noun(self) -> &'static str {
        match self {
            Self::Lines => "lines",
            Self::Bytes => "bytes",
            Self::Wildcards => "wildcards",
        }
    }
}

impl std::fmt::Display for BudgetKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.limit(), self.noun())
    }
}

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

    /// A line has more than [`MAX_PATTERN_WILDCARDS`] wildcards.
    #[error("a pattern has more than {MAX_PATTERN_WILDCARDS} wildcards")]
    TooComplex,

    /// The patterns of one config, with those of the files it extends, are over the budget.
    #[error("the patterns exceed the budget of {kind}")]
    OverBudget {
        /// What ran out.
        kind: BudgetKind,
    },
}

/// Number of wildcards in a pattern line: a run of `*` counts once, each `?` and `[` once.
pub fn wildcard_count(line: &str) -> usize {
    if line.starts_with('#') {
        return 0;
    }
    let mut count = 0;
    let mut in_run = false;
    for c in line.chars() {
        match c {
            '*' if in_run => {}
            '*' => {
                count += 1;
                in_run = true;
            }
            '?' | '[' => {
                count += 1;
                in_run = false;
            }
            _ => in_run = false,
        }
    }
    count
}

/// What a config load may still spend on pattern lines, bytes and wildcards.
///
/// Bounds the memory and time that compiling gitignore patterns takes, which the per-key line
/// cap alone does not: lines can be long and each wildcard grows the compiled matcher.
#[derive(Debug, Clone)]
pub struct PatternBudget {
    lines: usize,
    bytes: usize,
    wildcards: usize,
}

impl PatternBudget {
    pub const fn new() -> Self {
        Self {
            lines: BUDGET_LINES,
            bytes: BUDGET_BYTES,
            wildcards: BUDGET_WILDCARDS,
        }
    }

    fn take(slot: &mut usize, amount: usize, kind: BudgetKind) -> Result<(), InvalidPathPattern> {
        *slot = slot
            .checked_sub(amount)
            .ok_or(InvalidPathPattern::OverBudget { kind })?;
        Ok(())
    }

    /// Charges one pattern line; the error never contains the line.
    pub fn charge(&mut self, line: &str) -> Result<(), InvalidPathPattern> {
        let wildcards = wildcard_count(line);
        if wildcards > MAX_PATTERN_WILDCARDS {
            return Err(InvalidPathPattern::TooComplex);
        }
        Self::take(&mut self.lines, 1, BudgetKind::Lines)?;
        Self::take(&mut self.bytes, line.len(), BudgetKind::Bytes)?;
        Self::take(&mut self.wildcards, wildcards, BudgetKind::Wildcards)
    }

    fn charge_all(&mut self, lines: &[String]) -> Result<(), InvalidPathPattern> {
        lines.iter().try_for_each(|line| self.charge(line))
    }
}

/// Compiled ignore patterns with the lines they came from, shared by every key that names the
/// same files.
#[derive(Debug, Clone)]
pub struct Shared {
    pub lines: Arc<[String]>,
    pub patterns: IgnorePatterns,
}

impl Shared {
    /// Compiles lines that have already been charged to a budget.
    pub fn build(root: &Path, lines: &[String]) -> Result<Self, InvalidPathPattern> {
        Ok(Self {
            lines: lines.into(),
            patterns: IgnorePatterns::compile(root, lines)?,
        })
    }
}

/// Canonical ignore files, in order, and the root their patterns are anchored at.
pub type CacheKey = (Vec<PathBuf>, PathBuf);

/// Per-load state of pattern compilation: the matchers built so far and the budget they drew on.
///
/// A matcher is keyed by the canonical `ignore-from-file` files and the root, so a file named by
/// the top level and by many rules, or by several configs of an `extends` chain, is read,
/// charged and compiled once.
#[derive(Debug)]
pub struct PatternState {
    cache: HashMap<CacheKey, Shared>,
    budget: PatternBudget,
}

impl PatternState {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            budget: PatternBudget::new(),
        }
    }

    pub const fn budget_mut(&mut self) -> &mut PatternBudget {
        &mut self.budget
    }

    pub fn cached(&self, key: &CacheKey) -> Option<Shared> {
        self.cache.get(key).cloned()
    }

    pub fn store(&mut self, key: CacheKey, shared: Shared) {
        self.cache.insert(key, shared);
    }

    /// Charges and compiles the lines of an inline `ignore` key.
    pub fn compile_inline(
        &mut self,
        root: &Path,
        lines: &[String],
    ) -> Result<Shared, InvalidPathPattern> {
        check_count(lines)?;
        self.budget.charge_all(lines)?;
        Shared::build(root, lines)
    }
}

const fn check_count(lines: &[String]) -> Result<(), InvalidPathPattern> {
    if lines.len() > MAX_PATH_PATTERNS {
        return Err(InvalidPathPattern::TooMany { count: lines.len() });
    }
    Ok(())
}

fn build(root: &Path, lines: &[String]) -> Result<Gitignore, InvalidPathPattern> {
    check_count(lines)?;
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
    matcher: Arc<Gitignore>,
}

impl IgnorePatterns {
    /// Compiles `lines` as gitignore patterns anchored at `root`.
    ///
    /// `root` should be canonical so that it is a prefix of canonical file paths. Blank lines
    /// and `#` comments are skipped.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidPathPattern`] naming the first line gitignore syntax rejects, when
    /// there are more than [`MAX_PATH_PATTERNS`] lines, or when the lines have more wildcards
    /// or bytes than one load of a config may use.
    pub fn new(root: &Path, lines: &[String]) -> Result<Self, InvalidPathPattern> {
        PatternState::new()
            .compile_inline(root, lines)
            .map(|shared| shared.patterns)
    }

    fn compile(root: &Path, lines: &[String]) -> Result<Self, InvalidPathPattern> {
        Ok(Self {
            root: root.to_owned(),
            matcher: Arc::new(build(root, lines)?),
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
    /// Returns [`InvalidPathPattern`] naming the first line gitignore syntax rejects, when
    /// there are more than [`MAX_PATH_PATTERNS`] lines, or when the lines have more wildcards
    /// or bytes than one load of a config may use.
    pub fn new(lines: &[String]) -> Result<Self, InvalidPathPattern> {
        Self::with_budget(lines, &mut PatternBudget::new())
    }

    /// Like [`YamlFiles::new`], drawing on the budget of a config load.
    pub fn with_budget(
        lines: &[String],
        budget: &mut PatternBudget,
    ) -> Result<Self, InvalidPathPattern> {
        check_count(lines)?;
        budget.charge_all(lines)?;
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
    fn wildcards_are_counted_by_run_for_stars_and_singly_otherwise() {
        assert_eq!(wildcard_count("plain/"), 0);
        assert_eq!(wildcard_count("*"), 1);
        assert_eq!(wildcard_count("**/a***b"), 2);
        assert_eq!(wildcard_count("a*b*c"), 2);
        assert_eq!(wildcard_count("??"), 2);
        assert_eq!(wildcard_count("[ab][cd]"), 2);
        assert_eq!(wildcard_count("*?*"), 3);
        assert_eq!(wildcard_count("# *?[ comment"), 0);
    }

    #[test]
    fn a_line_is_rejected_above_the_per_pattern_cap_only() {
        let at_cap = "*a".repeat(MAX_PATTERN_WILDCARDS);
        let over = "*a".repeat(MAX_PATTERN_WILDCARDS + 1);
        assert!(PatternBudget::new().charge(&at_cap).is_ok());
        assert_eq!(
            PatternBudget::new().charge(&over),
            Err(InvalidPathPattern::TooComplex)
        );
    }

    #[test]
    fn the_budget_is_exact_for_lines_and_wildcards() {
        let mut lines = PatternBudget::new();
        assert!((0..BUDGET_LINES).all(|_| lines.charge("a/").is_ok()));
        assert_eq!(
            lines.charge("a/"),
            Err(InvalidPathPattern::OverBudget {
                kind: BudgetKind::Lines
            })
        );
        let mut wildcards = PatternBudget::new();
        assert!((0..BUDGET_WILDCARDS / 2).all(|_| wildcards.charge("*a*").is_ok()));
        assert_eq!(
            wildcards.charge("*a*"),
            Err(InvalidPathPattern::OverBudget {
                kind: BudgetKind::Wildcards
            })
        );
    }

    #[test]
    fn yaml_files_are_case_sensitive() {
        let files = YamlFiles::new(&lines("*.yaml")).unwrap();
        assert!(!files.matches(Path::new("A.YAML")));
    }
}
