//! The BOM-free text a lint run reports locations in.

use fast_yaml_core::NormalizedInput;

use crate::{LintError, SourceContext};

/// Validated source text, in the coordinates every [`Diagnostic`](crate::Diagnostic) uses.
///
/// Locations of diagnostics refer to the input with its document-prefix byte order marks
/// removed. A caller that prints excerpts of the input needs that same text, so it builds a
/// `LintSource` once, lints it with [`Linter::lint_source`](crate::Linter::lint_source) and
/// cuts excerpts from [`context`](Self::context).
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::formatter::Findings;
/// use fast_yaml_linter::{Formatter, LintSource, Linter, TextFormatter};
///
/// let source = LintSource::new("\u{feff}a:   1\n").unwrap();
/// let diagnostics = Linter::with_all_rules().lint_source(&source);
/// let context = source.context();
/// let text = TextFormatter::new().format(Findings::FromSource {
///     diagnostics: &diagnostics.unwrap(),
///     source: &context,
/// });
/// assert!(text.contains("   1 | a:   1"));
/// ```
#[derive(Debug, Clone)]
pub struct LintSource<'a> {
    normalized: NormalizedInput<'a>,
}

impl<'a> LintSource<'a> {
    /// Validates `raw` and strips its prefix byte order marks.
    ///
    /// # Errors
    ///
    /// Returns [`LintError::ParseError`] when `raw` contains a character YAML does not allow.
    pub fn new(raw: &'a str) -> Result<Self, LintError> {
        Ok(Self {
            normalized: NormalizedInput::new(raw)?,
        })
    }

    /// The text locations refer to.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.normalized.as_str()
    }

    /// Length in bytes of the input before normalization, which size limits apply to.
    #[must_use]
    pub const fn original_len(&self) -> usize {
        self.normalized.original_len()
    }

    /// The line index of [`as_str`](Self::as_str), for cutting excerpts.
    #[must_use]
    pub fn context(&self) -> SourceContext<'_> {
        SourceContext::new(self.as_str())
    }

    pub(crate) const fn normalized(&self) -> &NormalizedInput<'a> {
        &self.normalized
    }
}
