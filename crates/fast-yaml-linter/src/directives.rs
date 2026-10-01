//! Inline lint suppression directives.
//!
//! Comments of the form `# fy: disable [rules]`, `# fy: enable [rules]`,
//! `# fy: disable-line [rules]` and `# fy: disable-file` (plus the `# yamllint ...` spelling)
//! suppress diagnostics after the rules have run. Matching is line-granular: a diagnostic is
//! suppressed when the line its span starts on is covered by a directive. Malformed or
//! unknown directives are reported as `lint-directive` diagnostics, which no directive can
//! suppress.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use crate::config::RuleName;
use crate::echo::{KEY_LIMIT, echo};
use crate::rules::RuleRegistry;
use crate::{
    CommentKind, Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Severity,
    SourceContext, Span,
};

/// yamllint rule names that do not match a fast-yaml code 1:1.
const ALIASES: &[(&str, &[&str])] = &[
    ("key-duplicates", &[DiagnosticCode::DUPLICATE_KEY]),
    (
        "anchors",
        &[
            DiagnosticCode::INVALID_ANCHOR,
            DiagnosticCode::UNDEFINED_ALIAS,
        ],
    ),
    ("trailing-spaces", &[DiagnosticCode::TRAILING_WHITESPACE]),
];

/// The rules a directive applies to; an empty `Codes` set never means "all rules".
#[derive(Debug, Clone, PartialEq, Eq)]
enum RuleSelector {
    All,
    Codes(BTreeSet<DiagnosticCode>),
}

impl RuleSelector {
    fn matches(&self, code: &DiagnosticCode) -> bool {
        match self {
            Self::All => true,
            Self::Codes(codes) => codes.contains(code),
        }
    }

    fn merge(&mut self, other: Self) {
        match (&mut *self, other) {
            (Self::Codes(a), Self::Codes(b)) => a.extend(b),
            (this, _) => *this = Self::All,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DirectiveKind {
    Disable(RuleSelector),
    Enable(RuleSelector),
    DisableLine(RuleSelector),
    DisableFile,
}

/// Block state closed under `disable` / `enable`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DisabledRules {
    Only(BTreeSet<DiagnosticCode>),
    AllExcept(BTreeSet<DiagnosticCode>),
}

impl DisabledRules {
    fn contains(&self, code: &DiagnosticCode) -> bool {
        match self {
            Self::Only(set) => set.contains(code),
            Self::AllExcept(set) => !set.contains(code),
        }
    }

    fn disable(&mut self, selector: RuleSelector) {
        match (&mut *self, selector) {
            (_, RuleSelector::All) => *self = Self::AllExcept(BTreeSet::new()),
            (Self::Only(set), RuleSelector::Codes(codes)) => set.extend(codes),
            (Self::AllExcept(set), RuleSelector::Codes(codes)) => {
                set.retain(|c| !codes.contains(c));
            }
        }
    }

    fn enable(&mut self, selector: RuleSelector) {
        match (&mut *self, selector) {
            (_, RuleSelector::All) => *self = Self::Only(BTreeSet::new()),
            (Self::Only(set), RuleSelector::Codes(codes)) => {
                set.retain(|c| !codes.contains(c));
            }
            (Self::AllExcept(set), RuleSelector::Codes(codes)) => set.extend(codes),
        }
    }
}

impl Default for DisabledRules {
    fn default() -> Self {
        Self::Only(BTreeSet::new())
    }
}

/// Outcome of parsing one comment.
#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    NotDirective,
    /// Recognised as a directive but unusable; carries the reason.
    Invalid(String),
    /// Usable directive, possibly with a problem worth reporting (bad names, trailing text).
    Directive {
        kind: DirectiveKind,
        problem: Option<String>,
    },
}

/// The directive verb, before rule names are attached.
#[derive(Clone, Copy)]
enum Verb {
    Disable,
    Enable,
    DisableLine,
    DisableFile,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prefix {
    Fy,
    Yamllint,
}

/// Names listed per category in a problem message.
const MAX_LISTED: usize = 5;

/// Problems found among a directive's rule names, reported as one message.
#[derive(Default)]
struct NameIssues {
    unknown: BTreeSet<String>,
    malformed: BTreeSet<String>,
    config_only: bool,
    trailing_text: bool,
}

impl NameIssues {
    fn message(&self) -> Option<String> {
        let mut parts = Vec::new();
        if !self.unknown.is_empty() {
            parts.push(format!("unknown rule {}", listed(&self.unknown)));
        }
        if !self.malformed.is_empty() {
            parts.push(format!("malformed rule name {}", listed(&self.malformed)));
        }
        if self.config_only {
            parts.push(format!(
                "`{}` is config-only and cannot be named in a directive",
                DiagnosticCode::LINT_DIRECTIVE
            ));
        }
        if self.trailing_text {
            parts.push("unexpected trailing text after rule names".to_owned());
        }
        (!parts.is_empty()).then(|| format!("{} in lint directive", parts.join("; ")))
    }
}

fn listed(names: &BTreeSet<String>) -> String {
    let shown: Vec<String> = names
        .iter()
        .take(MAX_LISTED)
        .map(|n| format!("`{n}`"))
        .collect();
    match names.len().saturating_sub(MAX_LISTED) {
        0 => shown.join(", "),
        more => format!("{} and {more} more", shown.join(", ")),
    }
}

fn truncated(name: &str) -> String {
    echo(name, KEY_LIMIT)
}

fn is_rule_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Adds the codes `name` stands for to `codes`, or records why it is not usable.
fn resolve(
    name: &str,
    is_known: &impl Fn(&str) -> bool,
    codes: &mut BTreeSet<DiagnosticCode>,
    issues: &mut NameIssues,
) {
    if !is_rule_name(name) {
        issues.malformed.insert(truncated(name));
    } else if name == DiagnosticCode::LINT_DIRECTIVE {
        issues.config_only = true;
    } else if let Some((_, aliased)) = ALIASES.iter().find(|(alias, _)| *alias == name) {
        codes.extend(aliased.iter().copied().map(DiagnosticCode::from));
    } else if is_known(name) {
        codes.insert(DiagnosticCode::from(name));
    } else {
        issues.unknown.insert(truncated(name));
    }
}

/// Parses one comment (starting at `#`).
fn parse(comment: &str, is_known: impl Fn(&str) -> bool) -> Parsed {
    let Some(body) = comment.strip_prefix('#').map(str::trim_start) else {
        return Parsed::NotDirective;
    };
    let (prefix, rest) = if let Some(rest) = body.strip_prefix("fy:") {
        (Prefix::Fy, rest)
    } else if let Some(rest) = body
        .strip_prefix("yamllint")
        .filter(|rest| rest.starts_with([' ', '\t']))
    {
        (Prefix::Yamllint, rest)
    } else {
        return Parsed::NotDirective;
    };

    let mut tokens = rest.split_whitespace();
    let verb = match tokens.next() {
        Some("disable") => Verb::Disable,
        Some("enable") => Verb::Enable,
        Some("disable-line") => Verb::DisableLine,
        Some("disable-file") => Verb::DisableFile,
        Some(other) if prefix == Prefix::Fy => {
            return Parsed::Invalid(format!(
                "unknown lint directive verb `{}`",
                truncated(other)
            ));
        }
        None if prefix == Prefix::Fy => {
            return Parsed::Invalid("missing lint directive verb".to_owned());
        }
        _ => return Parsed::NotDirective,
    };

    let mut issues = NameIssues::default();
    let mut codes = BTreeSet::new();
    let mut has_names = false;
    for token in tokens {
        if token.starts_with('#') {
            issues.trailing_text = true;
            break;
        }
        has_names = true;
        resolve(
            token.strip_prefix("rule:").unwrap_or(token),
            &is_known,
            &mut codes,
            &mut issues,
        );
    }

    let selector = if has_names {
        RuleSelector::Codes(codes)
    } else if issues.trailing_text && !matches!(verb, Verb::DisableFile) {
        return Parsed::Invalid(
            "unexpected trailing text after directive in lint directive".to_owned(),
        );
    } else {
        RuleSelector::All
    };
    let kind = match verb {
        Verb::Disable => DirectiveKind::Disable(selector),
        Verb::Enable => DirectiveKind::Enable(selector),
        Verb::DisableLine => DirectiveKind::DisableLine(selector),
        Verb::DisableFile if has_names => {
            return Parsed::Invalid("`disable-file` does not take rule names".to_owned());
        }
        Verb::DisableFile => DirectiveKind::DisableFile,
    };
    Parsed::Directive {
        kind,
        problem: issues.message(),
    }
}

/// Suppressions and directive warnings extracted from one source file.
#[derive(Debug, Default)]
pub struct Directives {
    disable_file: bool,
    /// Cumulative block state after each `disable` / `enable`, keyed by directive line.
    blocks: Vec<(usize, DisabledRules)>,
    lines: BTreeMap<usize, RuleSelector>,
    warnings: Vec<Diagnostic>,
}

impl Directives {
    /// Collects directives from the comments of `context` (BOM-stripped, as the rules see it).
    ///
    /// A source whose comments cannot be located has no directives.
    pub fn from_context(
        context: &LintContext<'_>,
        config: &LintConfig,
        registry: &RuleRegistry,
    ) -> Self {
        let source = context.source();
        let mut this = Self::default();
        if !may_contain_directive(source) {
            return this;
        }

        let ctx = context.source_context();
        let is_known = |name: &str| is_known_code(name) || registry.get(name).is_some();
        let mut state = DisabledRules::default();
        let mut problems: Vec<(Span, String)> = Vec::new();
        let mut content_line = None;

        for comment in context.comments() {
            let full_line = comment.is_full_line();
            let Some(text) = source.get(comment.span.start.offset..comment.span.end.offset) else {
                continue;
            };
            let (kind, mut problem) = match parse(text, is_known) {
                Parsed::NotDirective => continue,
                Parsed::Invalid(problem) => {
                    problems.push((comment.span, problem));
                    continue;
                }
                Parsed::Directive { kind, problem } => (kind, problem),
            };
            let line = comment.span.start.line;

            let misplaced = match kind {
                DirectiveKind::Disable(_) | DirectiveKind::Enable(_)
                    if comment.kind == CommentKind::Inline =>
                {
                    Some("`disable` and `enable` must be on their own line")
                }
                DirectiveKind::DisableFile => {
                    let first_content =
                        *content_line.get_or_insert_with(|| first_content_line(ctx));
                    (comment.kind == CommentKind::Inline || line >= first_content)
                        .then_some("`disable-file` must be a comment before any YAML content")
                }
                _ => None,
            };
            if let Some(reason) = misplaced {
                problem = Some(problem.map_or_else(
                    || reason.to_owned(),
                    |existing| format!("{existing}; {reason}"),
                ));
            } else {
                match kind {
                    DirectiveKind::Disable(selector) => {
                        state.disable(selector);
                        this.blocks.push((line, state.clone()));
                    }
                    DirectiveKind::Enable(selector) => {
                        state.enable(selector);
                        this.blocks.push((line, state.clone()));
                    }
                    DirectiveKind::DisableLine(selector) => {
                        let target = if full_line { line + 1 } else { line };
                        match this.lines.entry(target) {
                            Entry::Occupied(mut entry) => entry.get_mut().merge(selector),
                            Entry::Vacant(entry) => {
                                entry.insert(selector);
                            }
                        }
                    }
                    DirectiveKind::DisableFile => this.disable_file = true,
                }
            }
            if let Some(problem) = problem {
                problems.push((comment.span, problem));
            }
        }

        if config.rules.is_enabled(RuleName::LintDirective) {
            let severity = config
                .rules
                .severity(RuleName::LintDirective)
                .unwrap_or(Severity::Warning);
            this.warnings = problems
                .into_iter()
                .map(|(span, message)| {
                    DiagnosticBuilder::new(DiagnosticCode::LINT_DIRECTIVE, severity, message, span)
                        .build_with_context(ctx)
                })
                .collect();
        }
        this
    }

    /// Whether a `disable-file` directive makes running the rules pointless.
    pub const fn disables_file(&self) -> bool {
        self.disable_file
    }

    /// Drops suppressed diagnostics and appends the directive warnings.
    pub fn apply(self, diagnostics: &mut Vec<Diagnostic>) {
        if !self.blocks.is_empty() || !self.lines.is_empty() {
            diagnostics.retain(|d| !self.is_suppressed(d));
        }
        diagnostics.extend(self.warnings);
    }

    fn is_suppressed(&self, diagnostic: &Diagnostic) -> bool {
        let line = diagnostic.span.start.line;
        let idx = self.blocks.partition_point(|(l, _)| *l <= line);
        let blocked = idx
            .checked_sub(1)
            .and_then(|i| self.blocks.get(i))
            .is_some_and(|(_, state)| state.contains(&diagnostic.code));
        blocked
            || self
                .lines
                .get(&line)
                .is_some_and(|selector| selector.matches(&diagnostic.code))
    }
}

/// Whether `name` is a built-in rule or a diagnostic code a built-in rule emits.
fn is_known_code(name: &str) -> bool {
    name == DiagnosticCode::UNDEFINED_ALIAS || name.parse::<RuleName>().is_ok()
}

/// Cheap prefilter: some `#` is followed by a directive prefix.
fn may_contain_directive(source: &str) -> bool {
    source.match_indices('#').any(|(i, _)| {
        source.get(i + 1..).is_some_and(|rest| {
            let rest = rest.trim_start();
            rest.starts_with("fy:") || rest.starts_with("yamllint")
        })
    })
}

/// 1-based number of the first line that is not blank, a full-line comment or a `%` directive.
fn first_content_line(ctx: &SourceContext<'_>) -> usize {
    (1..=ctx.line_count())
        .find(|&n| {
            let line = ctx.get_line(n).unwrap_or_default();
            let text = line.trim();
            !text.is_empty() && !text.starts_with('#') && !line.starts_with('%')
        })
        .unwrap_or_else(|| ctx.line_count() + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Linter;

    fn known(name: &str) -> bool {
        is_known_code(name)
    }

    fn codes(names: &[&str]) -> RuleSelector {
        RuleSelector::Codes(names.iter().copied().map(DiagnosticCode::from).collect())
    }

    fn kind(comment: &str) -> Option<DirectiveKind> {
        match parse(comment, known) {
            Parsed::Directive { kind, .. } => Some(kind),
            Parsed::NotDirective | Parsed::Invalid(_) => None,
        }
    }

    fn problem(comment: &str) -> Option<String> {
        match parse(comment, known) {
            Parsed::Directive { problem, .. } => problem,
            Parsed::Invalid(problem) => Some(problem),
            Parsed::NotDirective => None,
        }
    }

    fn is_directive(comment: &str) -> bool {
        parse(comment, known) != Parsed::NotDirective
    }

    fn lint(source: &str) -> Vec<Diagnostic> {
        Linter::with_all_rules().lint(source).unwrap()
    }

    fn lines_of(diagnostics: &[Diagnostic], code: &str) -> Vec<usize> {
        diagnostics
            .iter()
            .filter(|d| d.code.as_str() == code)
            .map(|d| d.span.start.line)
            .collect()
    }

    fn codes_of(diagnostics: &[Diagnostic]) -> Vec<&str> {
        diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn grammar_prefixes_and_optional_rule_keyword() {
        let dup = codes(&[DiagnosticCode::DUPLICATE_KEY]);
        for comment in [
            "# fy: disable duplicate-key",
            "# fy:disable rule:duplicate-key",
            "#fy: disable duplicate-key",
            "# yamllint disable rule:duplicate-key",
            "# yamllint disable duplicate-key",
            "# yamllint disable rule:key-duplicates",
        ] {
            assert_eq!(
                kind(comment),
                Some(DirectiveKind::Disable(dup.clone())),
                "{comment}"
            );
        }
    }

    #[test]
    fn grammar_verbs_and_all_selector() {
        assert_eq!(
            kind("# fy: disable"),
            Some(DirectiveKind::Disable(RuleSelector::All))
        );
        assert_eq!(
            kind("# fy: enable"),
            Some(DirectiveKind::Enable(RuleSelector::All))
        );
        assert_eq!(
            kind("# yamllint disable-line"),
            Some(DirectiveKind::DisableLine(RuleSelector::All))
        );
        assert_eq!(kind("# fy: disable-file"), Some(DirectiveKind::DisableFile));
    }

    #[test]
    fn grammar_aliases() {
        assert_eq!(
            kind("# yamllint disable rule:anchors"),
            Some(DirectiveKind::Disable(codes(&[
                "invalid-anchor",
                "undefined-alias"
            ])))
        );
        assert_eq!(
            kind("# yamllint disable rule:trailing-spaces"),
            Some(DirectiveKind::Disable(codes(&["trailing-whitespace"])))
        );
    }

    #[test]
    fn grammar_bad_verbs() {
        assert!(matches!(parse("# fy: nope", known), Parsed::Invalid(_)));
        assert!(matches!(parse("# fy:", known), Parsed::Invalid(_)));
        assert!(!is_directive("# yamllint nope"));
        assert!(!is_directive("# yamllint is great"));
        assert!(!is_directive("# yamllintdisable"));
        assert!(!is_directive("# note: fy: disable"));
    }

    #[test]
    fn grammar_disable_file_with_args_is_invalid() {
        assert!(matches!(
            parse("# fy: disable-file line-length", known),
            Parsed::Invalid(_)
        ));
    }

    #[test]
    fn grammar_unknown_and_malformed_names_drop_only_the_bad_name() {
        let comment = "# fy: disable bogus line-length";
        assert_eq!(
            kind(comment),
            Some(DirectiveKind::Disable(codes(&["line-length"])))
        );
        assert!(problem(comment).unwrap().contains("`bogus`"));

        assert_eq!(
            kind("# fy: disable bogus"),
            Some(DirectiveKind::Disable(codes(&[])))
        );

        let malformed = "# fy: disable Line_Length line-length";
        assert_eq!(
            kind(malformed),
            Some(DirectiveKind::Disable(codes(&["line-length"])))
        );
        assert!(problem(malformed).unwrap().contains("malformed rule name"));

        let config_only = "# fy: disable lint-directive";
        assert_eq!(kind(config_only), Some(DirectiveKind::Disable(codes(&[]))));
        assert!(problem(config_only).unwrap().contains("config-only"));
    }

    #[test]
    fn grammar_trailing_text_keeps_names_and_reports_it() {
        let named = "# fy: disable-line line-length  # because";
        assert_eq!(
            kind(named),
            Some(DirectiveKind::DisableLine(codes(&["line-length"])))
        );
        assert!(problem(named).unwrap().contains("trailing text"));

        let file = "# fy: disable-file # because";
        assert_eq!(kind(file), Some(DirectiveKind::DisableFile));
        assert!(problem(file).unwrap().contains("trailing text"));
    }

    #[test]
    fn grammar_trailing_text_without_names_never_widens() {
        for comment in [
            "# fy: disable-line  # legacy",
            "# fy: disable # TODO",
            "# fy: enable #x",
        ] {
            assert_eq!(kind(comment), None, "{comment}");
            assert!(
                problem(comment).unwrap().contains("trailing text"),
                "{comment}"
            );
        }
        let diagnostics = lint("a: 1\na: 2 # fy: disable-line  # legacy\n");
        assert!(codes_of(&diagnostics).contains(&"duplicate-key"));
        assert_eq!(lines_of(&diagnostics, "lint-directive"), [2]);
    }

    #[test]
    fn inline_block_directives_are_rejected() {
        let source = "a: 1 # fy: disable line-length\na: 2\n";
        let diagnostics = lint(source);
        assert!(codes_of(&diagnostics).contains(&"duplicate-key"));
        assert_eq!(lines_of(&diagnostics, "lint-directive"), [1]);

        let enable = lint("# fy: disable\na: 1 # fy: enable\na: 2\n");
        assert_eq!(lines_of(&enable, "lint-directive"), [2]);
        assert!(!codes_of(&enable).contains(&"duplicate-key"));
    }

    #[test]
    fn disable_file_after_percent_directives() {
        let source = "%YAML 1.2\n# fy: disable-file\n---\na: 1\na: 2\n";
        assert!(lint(source).is_empty());
    }

    #[test]
    fn late_disable_file_with_trailing_text_is_one_diagnostic() {
        let diagnostics = lint("a: 1\n# fy: disable-file # why\n");
        assert_eq!(lines_of(&diagnostics, "lint-directive"), [2]);
    }

    #[test]
    fn many_unknown_names_produce_one_bounded_diagnostic() {
        let names = (0..50_000)
            .map(|i| format!("bogus{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let names = format!(" {names}");
        let source = format!("# fy: disable{names}\na: 1\n");
        let diagnostics = lint(&source);
        let warnings: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code.as_str() == "lint-directive")
            .collect();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.len() < 400, "{}", warnings[0].message);
        assert!(warnings[0].message.contains("and 49995 more"));
    }

    #[test]
    fn many_disable_file_lines_stay_linear() {
        let source = "# fy: disable-file\n".repeat(20_000) + "a: 1\n";
        assert!(lint(&source).is_empty());
        let late = "a: 1\n".to_owned() + &"# fy: disable-file\n".repeat(20_000);
        let start = std::time::Instant::now();
        let diagnostics = lint(&late);
        assert_eq!(diagnostics.len(), 20_000);
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn prefilter_requires_comment_context() {
        assert!(!may_contain_directive("notify: on\nverify: yes\n"));
        assert!(may_contain_directive("a: 1 #fy: disable\n"));
        assert!(may_contain_directive("# yamllint disable\n"));
    }

    #[test]
    fn alias_targets_are_known_rules() {
        let registry = RuleRegistry::with_default_rules();
        for (_, targets) in ALIASES {
            for target in *targets {
                assert!(is_known_code(target), "{target}");
                // Undefined aliases are parse errors, so no registered rule emits that code.
                if *target != DiagnosticCode::UNDEFINED_ALIAS {
                    assert!(registry.get(target).is_some(), "{target}");
                }
            }
        }
    }

    #[test]
    fn block_state_carries_across_documents() {
        let source = "# fy: disable duplicate-key\na: 1\na: 2\n---\nb: 1\nb: 2\n";
        assert!(!codes_of(&lint(source)).contains(&"duplicate-key"));

        let enabled = "# fy: disable duplicate-key\na: 1\na: 2\n---\n# fy: enable\nb: 1\nb: 2\n";
        assert_eq!(lines_of(&lint(enabled), "duplicate-key"), [7]);
    }

    #[test]
    fn disable_file_before_document_marker() {
        assert!(lint("# fy: disable-file\n---\na: 1\na: 2\n").is_empty());
        let inline = lint("--- # fy: disable-file\na: 1\na: 2\n");
        assert!(codes_of(&inline).contains(&"lint-directive"));
        assert!(codes_of(&inline).contains(&"duplicate-key"));
    }

    #[test]
    fn lint_value_honors_directives() {
        let source = "a: 1\na: 2  # fy: disable-line duplicate-key\n# fy: disable bogus\n";
        let value = fast_yaml_core::Parser::parse_str(source).unwrap().unwrap();
        let diagnostics = Linter::with_all_rules().lint_value(source, &value).unwrap();
        assert!(!codes_of(&diagnostics).contains(&"duplicate-key"));
        assert!(codes_of(&diagnostics).contains(&"lint-directive"));
    }

    #[test]
    fn trailing_text_is_reported_not_malformed_name() {
        let diagnostics = lint("a: 1\na: 2 # fy: disable-line duplicate-key # note\n");
        let warning = diagnostics
            .iter()
            .find(|d| d.code.as_str() == "lint-directive")
            .unwrap();
        assert!(warning.message.contains("trailing text"));
        assert!(!codes_of(&diagnostics).contains(&"duplicate-key"));
    }

    #[test]
    fn disabled_rules_transitions() {
        let mut state = DisabledRules::default();
        assert!(!state.contains(&"a".into()));
        state.disable(codes(&["a"]));
        assert!(state.contains(&"a".into()) && !state.contains(&"b".into()));
        state.disable(RuleSelector::All);
        assert!(state.contains(&"b".into()));
        state.enable(codes(&["a"]));
        assert!(!state.contains(&"a".into()) && state.contains(&"b".into()));
        state.disable(codes(&["a"]));
        assert!(state.contains(&"a".into()));
        state.enable(RuleSelector::All);
        assert!(!state.contains(&"a".into()) && !state.contains(&"b".into()));
    }

    #[test]
    fn block_disable_and_enable_are_line_granular() {
        let source = "a: 1\na: 2\n# fy: disable duplicate-key\nb: 1\nb: 2\n# fy: enable duplicate-key\nc: 1\nc: 2\n";
        let lines: Vec<usize> = lint(source)
            .iter()
            .filter(|d| d.code.as_str() == "duplicate-key")
            .map(|d| d.span.start.line)
            .collect();
        assert_eq!(lines, [2, 8]);
    }

    #[test]
    fn block_directive_suppresses_offset_zero_diagnostic() {
        let config = LintConfig::new().with_max_line_length(std::num::NonZeroUsize::new(10));
        let source = "# fy: disable line-length\na: 1\nkey: a long value here that is long\n";
        let diagnostics = Linter::with_all_rules_and_config(config)
            .lint(source)
            .unwrap();
        assert!(!codes_of(&diagnostics).contains(&"line-length"));
    }

    #[test]
    fn disable_line_targets_by_placement() {
        let inline = "a: 1\na: 2 # fy: disable-line duplicate-key\n";
        assert!(!codes_of(&lint(inline)).contains(&"duplicate-key"));

        let full = "a: 1\n# fy: disable-line duplicate-key\na: 2\n";
        assert!(!codes_of(&lint(full)).contains(&"duplicate-key"));

        let wrong_line = "a: 1\na: 2\n# fy: disable-line duplicate-key\nb: 1\n";
        assert!(codes_of(&lint(wrong_line)).contains(&"duplicate-key"));
    }

    #[test]
    fn disable_line_with_lone_cr_line_endings() {
        let source = "a: 1\r# fy: disable-line duplicate-key\ra: 2\r";
        assert!(!codes_of(&lint(source)).contains(&"duplicate-key"));
    }

    #[test]
    fn disable_file_placement() {
        let accepted = "# header\n\n# fy: disable-file\na: 1\na: 2\n";
        assert!(lint(accepted).is_empty());

        let after_content = "a: 1\n# fy: disable-file\na: 2\na: 3\n";
        let diagnostics = lint(after_content);
        assert!(codes_of(&diagnostics).contains(&"duplicate-key"));
        assert!(codes_of(&diagnostics).contains(&"lint-directive"));

        let after_marker = "---\n# fy: disable-file\na: 1\na: 1\n";
        assert!(codes_of(&lint(after_marker)).contains(&"lint-directive"));

        let inline = "a: 1 # fy: disable-file\na: 2\n";
        assert!(codes_of(&lint(inline)).contains(&"lint-directive"));
    }

    #[test]
    fn unknown_rule_warns_and_does_not_widen() {
        let source = "# fy: disable bogus\na: 1\na: 2\n";
        let diagnostics = lint(source);
        let names = codes_of(&diagnostics);
        assert!(names.contains(&"duplicate-key"));
        let warning = diagnostics
            .iter()
            .find(|d| d.code.as_str() == "lint-directive")
            .unwrap();
        assert_eq!(warning.severity, Severity::Warning);
        assert_eq!(warning.span.start.line, 1);
        assert!(warning.context.is_some());
    }

    #[test]
    fn lint_directive_cannot_be_suppressed_by_directives() {
        let source = "# fy: disable\n# fy: disable bogus\na: 1\n";
        assert!(codes_of(&lint(source)).contains(&"lint-directive"));
    }

    #[test]
    fn lint_directive_respects_config() {
        let source = "# fy: disable bogus\na: 1\n";

        let disabled = LintConfig::new().with_disabled_rule(RuleName::LintDirective);
        let diagnostics = Linter::with_all_rules_and_config(disabled)
            .lint(source)
            .unwrap();
        assert!(!codes_of(&diagnostics).contains(&"lint-directive"));

        let mut escalated = LintConfig::new();
        escalated.rules.lint_directive.severity = Some(Severity::Error);
        let diagnostics = Linter::with_all_rules_and_config(escalated)
            .lint(source)
            .unwrap();
        let warning = diagnostics
            .iter()
            .find(|d| d.code.as_str() == "lint-directive")
            .unwrap();
        assert_eq!(warning.severity, Severity::Error);
    }

    #[test]
    fn hash_inside_scalars_is_not_a_directive() {
        let source = "a: \"x # fy: disable duplicate-key\"\nb: |\n  # fy: disable duplicate-key\nc: 1\nc: 2\n";
        assert!(codes_of(&lint(source)).contains(&"duplicate-key"));
    }

    #[test]
    fn syntax_error_is_not_suppressible() {
        assert!(
            Linter::with_all_rules()
                .lint("# fy: disable-file\na: [\n")
                .is_err()
        );
    }

    #[test]
    fn bom_free_offsets_for_directive_warnings() {
        let source = "\u{FEFF}# fy: disable bogus\na: 1\n";
        let diagnostics = lint(source);
        let warning = diagnostics
            .iter()
            .find(|d| d.code.as_str() == "lint-directive")
            .unwrap();
        assert_eq!(warning.span.start.offset, 0);
        assert_eq!(
            &source[3 + warning.span.start.offset..3 + warning.span.end.offset],
            "# fy: disable bogus"
        );
    }

    #[test]
    fn directive_text_inside_a_multiline_scalar_is_not_a_directive() {
        let diagnostics = lint("a: \"x\n  # fy: disable bogus\n  y\"\n");
        assert!(!codes_of(&diagnostics).contains(&"lint-directive"));
    }
}
