//! The diagnostics a [`Formatter`](super::Formatter) prints, with where their excerpts come from.

use std::borrow::Cow;

use crate::{Diagnostic, DiagnosticContext, Excerpt, SourceContext};

/// Source lines shown before and after a diagnostic's span.
const EXCERPT_CONTEXT_LINES: usize = 2;

/// Diagnostics to print, paired with the source lines shown beside them.
///
/// [`FromSource`](Self::FromSource) cuts each excerpt from the source only when a formatter
/// asks for it, so printing a million diagnostics holds one excerpt at a time.
/// [`Given`](Self::Given) carries excerpts cut earlier (a diagnostic list handed over by a
/// binding, or a batch result rendered by a worker) next to their diagnostics, so the two
/// cannot get out of step.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::formatter::Findings;
/// use fast_yaml_linter::{Linter, SourceContext};
///
/// let source = "a:   1\n";
/// let diagnostics = Linter::with_all_rules().lint(source).unwrap();
/// let context = SourceContext::new(source);
/// let findings = Findings::FromSource { diagnostics: &diagnostics, source: &context };
/// assert_eq!(findings.len(), diagnostics.len());
/// ```
#[derive(Debug, Clone, Copy)]
pub enum Findings<'a> {
    /// Diagnostics whose [`Excerpt::SourceLines`] excerpts are cut from `source` when printed.
    ///
    /// `source` must index the text the diagnostics were found in: the BOM-free text of
    /// [`LintSource`](crate::LintSource).
    FromSource {
        /// The diagnostics to print.
        diagnostics: &'a [Diagnostic],
        /// The source the diagnostics refer to.
        source: &'a SourceContext<'a>,
    },
    /// Diagnostics with the excerpts already cut for them.
    Given(&'a [(Diagnostic, Option<DiagnosticContext>)]),
}

impl<'a> Findings<'a> {
    /// No findings.
    pub const EMPTY: Self = Self::Given(&[]);

    /// Cuts the excerpt of every diagnostic from `source` now, for findings that outlive it.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::formatter::Findings;
    /// use fast_yaml_linter::{Linter, SourceContext};
    ///
    /// let source = "a:   1\n";
    /// let diagnostics = Linter::with_all_rules().lint(source).unwrap();
    /// let pairs = Findings::cut(diagnostics, &SourceContext::new(source));
    /// assert!(pairs.iter().all(|(_, context)| context.is_some()));
    /// ```
    #[must_use]
    pub fn cut(
        diagnostics: Vec<Diagnostic>,
        source: &SourceContext<'_>,
    ) -> Vec<(Diagnostic, Option<DiagnosticContext>)> {
        diagnostics
            .into_iter()
            .map(|diagnostic| {
                let context = (diagnostic.excerpt == Excerpt::SourceLines)
                    .then(|| source.extract_context(diagnostic.span, EXCERPT_CONTEXT_LINES));
                (diagnostic, context)
            })
            .collect()
    }

    /// Number of diagnostics.
    #[must_use]
    pub const fn len(&self) -> usize {
        match self {
            Self::FromSource { diagnostics, .. } => diagnostics.len(),
            Self::Given(pairs) => pairs.len(),
        }
    }

    /// Whether there are no diagnostics.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The diagnostics, without their excerpts.
    pub fn diagnostics(self) -> impl Iterator<Item = &'a Diagnostic> + 'a {
        let (from_source, given) = match self {
            Self::FromSource { diagnostics, .. } => (diagnostics, &[][..]),
            Self::Given(pairs) => (&[][..], pairs),
        };
        from_source
            .iter()
            .chain(given.iter().map(|(diagnostic, _)| diagnostic))
    }

    /// The diagnostics with access to their excerpts.
    pub fn iter(self) -> impl Iterator<Item = Finding<'a>> + 'a {
        let (from_source, given) = match self {
            Self::FromSource {
                diagnostics,
                source,
            } => (Some((diagnostics, source)), &[][..]),
            Self::Given(pairs) => (None, pairs),
        };
        from_source
            .into_iter()
            .flat_map(|(diagnostics, source)| {
                diagnostics.iter().map(move |diagnostic| Finding {
                    diagnostic,
                    origin: Origin::Source(source),
                })
            })
            .chain(given.iter().map(|(diagnostic, context)| Finding {
                diagnostic,
                origin: Origin::Given(context.as_ref()),
            }))
    }
}

/// One diagnostic of [`Findings`].
#[derive(Debug, Clone, Copy)]
pub struct Finding<'a> {
    diagnostic: &'a Diagnostic,
    origin: Origin<'a>,
}

#[derive(Debug, Clone, Copy)]
enum Origin<'a> {
    Source(&'a SourceContext<'a>),
    Given(Option<&'a DiagnosticContext>),
}

impl<'a> Finding<'a> {
    /// The diagnostic.
    #[must_use]
    pub const fn diagnostic(&self) -> &'a Diagnostic {
        self.diagnostic
    }

    /// The source lines to show beside the diagnostic, cut now when they come from a source.
    ///
    /// `None` when the diagnostic is [`Excerpt::Omitted`] or no excerpt was given for it.
    #[must_use]
    pub fn context(&self) -> Option<Cow<'a, DiagnosticContext>> {
        match self.origin {
            Origin::Source(source) => {
                (self.diagnostic.excerpt == Excerpt::SourceLines).then(|| {
                    Cow::Owned(source.extract_context(self.diagnostic.span, EXCERPT_CONTEXT_LINES))
                })
            }
            Origin::Given(context) => context.map(Cow::Borrowed),
        }
    }
}

impl<'a> From<&'a [(Diagnostic, Option<DiagnosticContext>)]> for Findings<'a> {
    fn from(pairs: &'a [(Diagnostic, Option<DiagnosticContext>)]) -> Self {
        Self::Given(pairs)
    }
}
