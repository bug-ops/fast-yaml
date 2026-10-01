//! File selection driven by the `ignore` and `yaml-files` keys of a lint config file.
//!
//! Without the `linter` feature no config file is read and the filter selects nothing.

use std::path::Path;
#[cfg(feature = "linter")]
use std::sync::Arc;

#[cfg(feature = "linter")]
use fast_yaml_linter::config::{FileSelection, IgnorePatterns, YamlFiles};

/// Config-file rules for which files `fy lint` visits.
#[derive(Debug, Clone, Default)]
pub struct FileFilter {
    #[cfg(feature = "linter")]
    ignore: Option<Arc<IgnorePatterns>>,
    #[cfg(feature = "linter")]
    yaml_files: Option<Arc<YamlFiles>>,
}

impl FileFilter {
    /// Builds a filter from the `ignore` and `yaml-files` settings.
    #[cfg(feature = "linter")]
    #[must_use]
    #[cfg_attr(not(feature = "linter"), expect(clippy::missing_const_for_fn))]
    pub fn new(selection: FileSelection) -> Self {
        Self {
            ignore: selection.ignore.map(Arc::new),
            yaml_files: selection.yaml_files.map(Arc::new),
        }
    }

    /// Returns whether config `ignore` drops `path`, which must be canonical.
    #[must_use]
    #[cfg_attr(
        not(feature = "linter"),
        expect(unused_variables, clippy::missing_const_for_fn)
    )]
    pub fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        #[cfg(feature = "linter")]
        {
            self.ignore
                .as_ref()
                .is_some_and(|ignore| ignore.matches(path, is_dir))
        }
        #[cfg(not(feature = "linter"))]
        {
            false
        }
    }

    /// Returns whether the config has an `ignore` key.
    #[must_use]
    pub const fn has_ignore(&self) -> bool {
        #[cfg(feature = "linter")]
        {
            self.ignore.is_some()
        }
        #[cfg(not(feature = "linter"))]
        {
            false
        }
    }

    /// Returns whether an ignored directory may be skipped without visiting its contents.
    ///
    /// A `!` pattern can re-include a file below an ignored directory, so pruning is off then.
    #[must_use]
    #[cfg_attr(not(feature = "linter"), expect(clippy::missing_const_for_fn))]
    pub fn can_prune(&self) -> bool {
        #[cfg(feature = "linter")]
        {
            self.ignore
                .as_ref()
                .is_some_and(|ignore| !ignore.has_negations())
        }
        #[cfg(not(feature = "linter"))]
        {
            false
        }
    }

    /// Returns whether the file name of `path` matches `yaml-files`, or `None` when the config
    /// has no `yaml-files` key.
    #[must_use]
    #[cfg_attr(
        not(feature = "linter"),
        expect(unused_variables, clippy::missing_const_for_fn)
    )]
    pub fn selects(&self, path: &Path) -> Option<bool> {
        #[cfg(feature = "linter")]
        {
            self.yaml_files.as_ref().map(|files| files.matches(path))
        }
        #[cfg(not(feature = "linter"))]
        {
            None
        }
    }
}
