//! File discovery for batch processing.

use std::collections::HashSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::error::DiscoveryError;

/// Maximum number of paths that can be read from stdin.
const MAX_STDIN_PATHS: usize = 100_000;

/// Maximum line length for stdin input.
const MAX_LINE_LENGTH: usize = 4096;

/// Maximum number of glob matches to prevent memory exhaustion.
const MAX_GLOB_MATCHES: usize = 100_000;

/// Configuration for file discovery.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Glob patterns for files to include (e.g., "*.yaml", "*.yml")
    pub include_patterns: Vec<String>,
    /// Glob patterns for files/directories to exclude (e.g., "**/vendor/**")
    pub exclude_patterns: Vec<String>,
    /// Maximum recursion depth (None = unlimited)
    pub max_depth: Option<usize>,
    /// Whether to include hidden files/directories
    pub include_hidden: bool,
    /// Whether to respect .gitignore files
    pub respect_gitignore: bool,
    /// Whether to follow symbolic links
    pub follow_symlinks: bool,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            include_patterns: vec!["*.yaml".into(), "*.yml".into()],
            exclude_patterns: vec![],
            max_depth: Some(100),
            include_hidden: false,
            respect_gitignore: true,
            follow_symlinks: false,
        }
    }
}

impl DiscoveryConfig {
    /// Create a new configuration with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set include patterns (builder pattern).
    #[must_use]
    pub fn with_include_patterns(mut self, patterns: Vec<String>) -> Self {
        self.include_patterns = patterns;
        self
    }

    /// Set exclude patterns (builder pattern).
    #[must_use]
    pub fn with_exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    /// Set maximum recursion depth.
    #[must_use]
    pub const fn with_max_depth(mut self, depth: Option<usize>) -> Self {
        self.max_depth = depth;
        self
    }

    /// Set unlimited recursion depth (use with caution).
    #[must_use]
    pub const fn with_unlimited_depth(mut self) -> Self {
        self.max_depth = None;
        self
    }

    /// Set whether to include hidden files.
    #[must_use]
    pub const fn with_hidden(mut self, include: bool) -> Self {
        self.include_hidden = include;
        self
    }

    /// Set whether to respect .gitignore.
    #[must_use]
    pub const fn with_gitignore(mut self, respect: bool) -> Self {
        self.respect_gitignore = respect;
        self
    }

    /// Set whether to follow symbolic links.
    #[must_use]
    pub const fn with_follow_symlinks(mut self, follow: bool) -> Self {
        self.follow_symlinks = follow;
        self
    }
}

/// Origin of a discovered file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryOrigin {
    /// File was specified directly as a path argument
    DirectPath,
    /// File was found by walking a directory
    DirectoryWalk,
    /// File was found by expanding a glob pattern
    GlobExpansion,
    /// File path was read from stdin
    StdinList,
}

/// A discovered file with its origin information.
#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    /// Canonical path to the file
    pub path: PathBuf,
    /// How this file was discovered
    pub origin: DiscoveryOrigin,
}

/// File discovery engine.
#[derive(Debug)]
pub struct FileDiscovery {
    config: DiscoveryConfig,
    include_matcher: GlobSet,
    exclude_matcher: GlobSet,
}

impl FileDiscovery {
    /// Create a new file discovery instance.
    pub fn new(config: DiscoveryConfig) -> Result<Self, DiscoveryError> {
        let include_matcher = build_globset(&config.include_patterns)?;
        let exclude_matcher = build_globset(&config.exclude_patterns)?;

        Ok(Self {
            config,
            include_matcher,
            exclude_matcher,
        })
    }

    /// Discover files from the given paths.
    ///
    /// Paths can be:
    /// - Regular files (included directly if matching patterns)
    /// - Directories (walked recursively)
    /// - Glob patterns (expanded)
    pub fn discover(&self, paths: &[PathBuf]) -> Result<Vec<DiscoveredFile>, DiscoveryError> {
        // Heuristic: estimate 10 files per input path
        let estimated_capacity = paths.len().saturating_mul(10);
        let mut discovered = Vec::with_capacity(estimated_capacity);
        let mut seen = HashSet::new();

        for path in paths {
            if path.exists() {
                if path.is_file() {
                    self.discover_file(
                        path,
                        DiscoveryOrigin::DirectPath,
                        &mut discovered,
                        &mut seen,
                    )?;
                } else if path.is_dir() {
                    self.discover_directory(path, &mut discovered, &mut seen);
                }
            } else {
                // Treat as glob pattern
                self.discover_glob(&path.to_string_lossy(), &mut discovered, &mut seen);
            }
        }

        Ok(discovered)
    }

    /// Discover files from stdin (one path per line).
    pub fn discover_from_stdin(&self) -> Result<Vec<DiscoveredFile>, DiscoveryError> {
        self.discover_from_reader(std::io::stdin().lock())
    }

    /// Discover files from any `BufRead` source (for testing).
    pub fn discover_from_reader<R: BufRead>(
        &self,
        reader: R,
    ) -> Result<Vec<DiscoveredFile>, DiscoveryError> {
        let mut discovered = Vec::new();
        let mut seen = HashSet::new();
        let mut count = 0;

        for line in reader.lines() {
            let line = line.map_err(|e| DiscoveryError::StdinError { source: e })?;

            count += 1;
            if count > MAX_STDIN_PATHS {
                return Err(DiscoveryError::TooManyPaths {
                    max: MAX_STDIN_PATHS,
                });
            }

            let trimmed = line.trim();

            if trimmed.len() > MAX_LINE_LENGTH {
                eprintln!("Warning: skipping line {count} (exceeds {MAX_LINE_LENGTH} chars)");
                continue;
            }

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let path = PathBuf::from(trimmed);
            if path.is_file() {
                self.discover_file(
                    &path,
                    DiscoveryOrigin::StdinList,
                    &mut discovered,
                    &mut seen,
                )?;
            }
        }

        Ok(discovered)
    }

    /// Check if a single path should be included.
    #[must_use]
    pub fn should_include(&self, path: &Path) -> bool {
        // Check exclude patterns first (match against full path)
        if self.exclude_matcher.is_match(path) {
            return false;
        }

        // Check include patterns (match against file name for extension patterns)
        path.file_name()
            .is_some_and(|file_name| self.include_matcher.is_match(file_name))
    }

    fn discover_file(
        &self,
        path: &Path,
        origin: DiscoveryOrigin,
        discovered: &mut Vec<DiscoveredFile>,
        seen: &mut HashSet<PathBuf>,
    ) -> Result<(), DiscoveryError> {
        if !self.should_include(path) {
            return Ok(());
        }

        // Canonicalize for deduplication
        let canonical = path.canonicalize().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                // Check if it's a broken symlink
                if path.symlink_metadata().is_ok() {
                    DiscoveryError::BrokenSymlink {
                        path: path.to_path_buf(),
                    }
                } else {
                    DiscoveryError::PathNotFound {
                        path: path.to_path_buf(),
                    }
                }
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                DiscoveryError::PermissionDenied {
                    path: path.to_path_buf(),
                }
            } else {
                DiscoveryError::IoError {
                    path: path.to_path_buf(),
                    source: e,
                }
            }
        })?;

        // Dedup by canonical path
        if seen.insert(canonical.clone()) {
            discovered.push(DiscoveredFile {
                path: canonical,
                origin,
            });
        }

        Ok(())
    }

    fn discover_directory(
        &self,
        dir: &Path,
        discovered: &mut Vec<DiscoveredFile>,
        seen: &mut HashSet<PathBuf>,
    ) {
        let mut builder = ignore::WalkBuilder::new(dir);
        builder
            .hidden(!self.config.include_hidden)
            .git_ignore(self.config.respect_gitignore)
            .git_global(self.config.respect_gitignore)
            .git_exclude(self.config.respect_gitignore)
            .follow_links(self.config.follow_symlinks);

        if let Some(depth) = self.config.max_depth {
            builder.max_depth(Some(depth));
        }

        for entry in builder.build() {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    // Log warning but continue processing
                    eprintln!("Warning: failed to read entry: {e}");
                    continue;
                }
            };

            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                let path = entry.path();
                // Ignore errors for individual files during directory walk
                let _ = self.discover_file(path, DiscoveryOrigin::DirectoryWalk, discovered, seen);
            }
        }
    }

    fn discover_glob(
        &self,
        pattern: &str,
        discovered: &mut Vec<DiscoveredFile>,
        seen: &mut HashSet<PathBuf>,
    ) {
        let Ok(glob) = glob::glob(pattern) else {
            eprintln!("Warning: invalid glob pattern: {pattern}");
            return;
        };

        let mut match_count = 0;
        for entry in glob {
            match_count += 1;
            if match_count > MAX_GLOB_MATCHES {
                eprintln!(
                    "Warning: glob pattern '{pattern}' exceeded {MAX_GLOB_MATCHES} matches, stopping"
                );
                break;
            }

            match entry {
                Ok(path) => {
                    if path.is_file() {
                        // Ignore errors for individual files during glob expansion
                        let _ = self.discover_file(
                            &path,
                            DiscoveryOrigin::GlobExpansion,
                            discovered,
                            seen,
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Warning: glob error: {e}");
                }
            }
        }
    }
}

fn build_globset(patterns: &[String]) -> Result<GlobSet, DiscoveryError> {
    let mut builder = GlobSetBuilder::new();

    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|e| DiscoveryError::InvalidPattern {
            pattern: pattern.clone(),
            source: e,
        })?;
        builder.add(glob);
    }

    builder.build().map_err(|e| DiscoveryError::InvalidPattern {
        pattern: "<combined>".to_string(),
        source: e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn default_config() -> DiscoveryConfig {
        DiscoveryConfig::new()
    }

    #[test]
    fn test_config_default() {
        let config = DiscoveryConfig::default();
        assert_eq!(config.include_patterns, vec!["*.yaml", "*.yml"]);
        assert!(config.exclude_patterns.is_empty());
        assert_eq!(config.max_depth, Some(100));
        assert!(!config.include_hidden);
        assert!(config.respect_gitignore);
        assert!(!config.follow_symlinks);
    }

    #[test]
    fn test_config_builder() {
        let config = DiscoveryConfig::new()
            .with_include_patterns(vec!["*.yml".to_string()])
            .with_exclude_patterns(vec!["**/vendor/**".to_string()])
            .with_max_depth(Some(5))
            .with_hidden(true)
            .with_gitignore(false)
            .with_follow_symlinks(true);

        assert_eq!(config.include_patterns, vec!["*.yml"]);
        assert_eq!(config.exclude_patterns, vec!["**/vendor/**"]);
        assert_eq!(config.max_depth, Some(5));
        assert!(config.include_hidden);
        assert!(!config.respect_gitignore);
        assert!(config.follow_symlinks);
    }

    #[test]
    fn test_include_pattern_yaml() {
        let config = default_config();
        let discovery = FileDiscovery::new(config).unwrap();

        assert!(discovery.should_include(Path::new("test.yaml")));
        assert!(discovery.should_include(Path::new("/path/to/test.yaml")));
    }

    #[test]
    fn test_include_pattern_yml() {
        let config = default_config();
        let discovery = FileDiscovery::new(config).unwrap();

        assert!(discovery.should_include(Path::new("test.yml")));
        assert!(discovery.should_include(Path::new("/path/to/test.yml")));
    }

    #[test]
    fn test_exclude_pattern() {
        let config = default_config().with_exclude_patterns(vec!["**/vendor/**".to_string()]);
        let discovery = FileDiscovery::new(config).unwrap();

        assert!(!discovery.should_include(Path::new("vendor/test.yaml")));
        assert!(!discovery.should_include(Path::new("path/vendor/test.yaml")));
        assert!(discovery.should_include(Path::new("test.yaml")));
    }

    #[test]
    fn test_exclude_vendor() {
        let config = default_config().with_exclude_patterns(vec!["**/vendor/**".to_string()]);
        let discovery = FileDiscovery::new(config).unwrap();

        assert!(!discovery.should_include(Path::new("vendor/lib/config.yaml")));
        assert!(discovery.should_include(Path::new("src/config.yaml")));
    }

    #[test]
    fn test_discover_single_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.yaml");
        fs::write(&file, "key: value").unwrap();

        let config = default_config();
        let discovery = FileDiscovery::new(config).unwrap();
        let files = discovery.discover(std::slice::from_ref(&file)).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].origin, DiscoveryOrigin::DirectPath);
    }

    #[test]
    fn test_discover_directory() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("root.yaml"), "a: 1").unwrap();
        fs::create_dir(temp.path().join("subdir")).unwrap();
        fs::write(temp.path().join("subdir/nested.yaml"), "b: 2").unwrap();
        fs::write(temp.path().join("subdir/skip.txt"), "c: 3").unwrap();

        let config = default_config();
        let discovery = FileDiscovery::new(config).unwrap();
        let files = discovery.discover(&[temp.path().to_path_buf()]).unwrap();

        assert_eq!(files.len(), 2);
        assert!(
            files
                .iter()
                .all(|f| f.origin == DiscoveryOrigin::DirectoryWalk)
        );
    }

    #[test]
    fn test_discover_glob() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("config.yaml"), "a: 1").unwrap();
        fs::write(temp.path().join("data.yml"), "b: 2").unwrap();
        fs::write(temp.path().join("readme.md"), "# README").unwrap();

        let pattern = format!("{}/*.yaml", temp.path().display());
        let config = default_config();
        let discovery = FileDiscovery::new(config).unwrap();
        let files = discovery.discover(&[PathBuf::from(pattern)]).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].origin, DiscoveryOrigin::GlobExpansion);
    }

    #[test]
    fn test_discover_mixed_paths() {
        let temp = TempDir::new().unwrap();

        // Direct file
        let file = temp.path().join("direct.yaml");
        fs::write(&file, "a: 1").unwrap();

        // Directory
        let dir = temp.path().join("dir");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("in_dir.yaml"), "b: 2").unwrap();

        let config = default_config();
        let discovery = FileDiscovery::new(config).unwrap();
        let files = discovery.discover(&[file, dir]).unwrap();

        assert_eq!(files.len(), 2);
        assert!(
            files
                .iter()
                .any(|f| f.origin == DiscoveryOrigin::DirectPath)
        );
        assert!(
            files
                .iter()
                .any(|f| f.origin == DiscoveryOrigin::DirectoryWalk)
        );
    }

    #[test]
    fn test_hidden_files_excluded() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".hidden.yaml"), "a: 1").unwrap();
        fs::write(temp.path().join("visible.yaml"), "b: 2").unwrap();

        let config = default_config(); // include_hidden = false
        let discovery = FileDiscovery::new(config).unwrap();
        let files = discovery.discover(&[temp.path().to_path_buf()]).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].path.ends_with("visible.yaml"));
    }

    #[test]
    fn test_hidden_files_included() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join(".hidden.yaml"), "a: 1").unwrap();
        fs::write(temp.path().join("visible.yaml"), "b: 2").unwrap();

        let config = default_config().with_hidden(true);
        let discovery = FileDiscovery::new(config).unwrap();
        let files = discovery.discover(&[temp.path().to_path_buf()]).unwrap();

        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_gitignore_respected() {
        // Skip if git is not available
        if std::process::Command::new("git")
            .args(["--version"])
            .output()
            .is_err()
        {
            eprintln!("Skipping test_gitignore_respected: git not available");
            return;
        }

        let temp = TempDir::new().unwrap();

        // Initialize a git repo - required for ignore crate to respect .gitignore
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        fs::write(temp.path().join(".gitignore"), "ignored.yaml\n").unwrap();
        fs::write(temp.path().join("ignored.yaml"), "a: 1").unwrap();
        fs::write(temp.path().join("included.yaml"), "b: 2").unwrap();

        let config = default_config(); // respect_gitignore = true
        let discovery = FileDiscovery::new(config).unwrap();
        let files = discovery.discover(&[temp.path().to_path_buf()]).unwrap();

        // Only included.yaml should be found (ignored.yaml is gitignored)
        assert_eq!(files.len(), 1);
        assert!(files[0].path.ends_with("included.yaml"));
    }

    #[test]
    fn test_max_depth() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("root.yaml"), "a: 1").unwrap();

        let level1 = temp.path().join("level1");
        fs::create_dir(&level1).unwrap();
        fs::write(level1.join("l1.yaml"), "b: 2").unwrap();

        let level2 = level1.join("level2");
        fs::create_dir(&level2).unwrap();
        fs::write(level2.join("l2.yaml"), "c: 3").unwrap();

        // max_depth = 1 should only find root.yaml
        let config = default_config().with_max_depth(Some(1));
        let discovery = FileDiscovery::new(config).unwrap();
        let files = discovery.discover(&[temp.path().to_path_buf()]).unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].path.ends_with("root.yaml"));
    }

    #[test]
    fn test_deduplication() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.yaml");
        fs::write(&file, "key: value").unwrap();

        let config = default_config();
        let discovery = FileDiscovery::new(config).unwrap();

        // Provide the same file twice
        let files = discovery.discover(&[file.clone(), file]).unwrap();

        // Should only be discovered once
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_invalid_pattern_error() {
        let config = default_config().with_include_patterns(vec!["[invalid".to_string()]);

        let result = FileDiscovery::new(config);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid glob pattern")
        );
    }

    #[test]
    fn test_discover_from_reader_valid_paths() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.yaml");
        fs::write(&file, "key: value").unwrap();

        let input = format!("{}\n", file.display());
        let reader = std::io::Cursor::new(input);

        let config = default_config();
        let discovery = FileDiscovery::new(config).unwrap();
        let files = discovery.discover_from_reader(reader).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].origin, DiscoveryOrigin::StdinList);
    }

    #[test]
    fn test_discover_from_reader_comments_and_empty_lines() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.yaml");
        fs::write(&file, "key: value").unwrap();

        let input = format!("# comment\n\n{}\n# another comment\n", file.display());
        let reader = std::io::Cursor::new(input);

        let config = default_config();
        let discovery = FileDiscovery::new(config).unwrap();
        let files = discovery.discover_from_reader(reader).unwrap();

        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_discover_from_reader_too_many_paths() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.yaml");
        fs::write(&file, "key: value").unwrap();

        let mut input = String::new();
        for _ in 0..=MAX_STDIN_PATHS {
            use std::fmt::Write;
            writeln!(&mut input, "{}", file.display()).unwrap();
        }
        let reader = std::io::Cursor::new(input);

        let config = default_config();
        let discovery = FileDiscovery::new(config).unwrap();
        let result = discovery.discover_from_reader(reader);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("exceeded maximum"));
    }

    #[test]
    fn test_discover_from_reader_long_line_skipped() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.yaml");
        fs::write(&file, "key: value").unwrap();

        let long_line = "x".repeat(MAX_LINE_LENGTH + 1);
        let input = format!("{}\n{}\n", long_line, file.display());
        let reader = std::io::Cursor::new(input);

        let config = default_config();
        let discovery = FileDiscovery::new(config).unwrap();
        let files = discovery.discover_from_reader(reader).unwrap();

        // Long line should be skipped, only valid file should be found
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_permission_denied_continues() {
        // Testing permission errors requires platform-specific setup
        // This is better suited for integration tests
    }
}
