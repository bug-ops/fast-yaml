//! A diagnostic carries the code of the rule that returned it and the severity configured for
//! that rule, whichever rule it is (#625).

use std::fmt::Write as _;

use fast_yaml_linter::config::{CustomRuleCode, NoOptions, RuleName, RuleSettings};
use fast_yaml_linter::rules::{DocumentRule, LintDocument, LintRule, Rule, RuleId, SourceRule};
use fast_yaml_linter::{
    ConfigFile, Excerpt, Finding, LintConfig, LintContext, Linter, Location, Severity, Span,
};

struct Flags(CustomRuleCode);

impl LintRule for Flags {
    fn id(&self) -> RuleId<'_> {
        RuleId::Custom(&self.0)
    }
    fn name(&self) -> &'static str {
        "Flags"
    }
    fn description(&self) -> &'static str {
        "Flags the first line"
    }
    fn default_severity(&self) -> Severity {
        Severity::Hint
    }
}

impl SourceRule for Flags {
    fn check(&self, context: &LintContext, _config: &LintConfig) -> Vec<Finding> {
        vec![Finding::new("flagged", line_span(context, 1))]
    }
}

struct FlagsDocuments(CustomRuleCode);

impl LintRule for FlagsDocuments {
    fn id(&self) -> RuleId<'_> {
        RuleId::Custom(&self.0)
    }
    fn name(&self) -> &'static str {
        "Flags documents"
    }
    fn description(&self) -> &'static str {
        "Flags every document"
    }
    fn default_severity(&self) -> Severity {
        Severity::Info
    }
}

impl DocumentRule for FlagsDocuments {
    fn check(
        &self,
        context: &LintContext,
        document: LintDocument<'_>,
        _config: &LintConfig,
    ) -> Vec<Finding> {
        let span = line_span(context, document.first_line);
        vec![Finding::new("document", span).without_excerpt()]
    }
}

fn line_span(context: &LintContext, line: usize) -> Span {
    let offset = context.source_context().get_line_offset(line);
    Span::new(
        Location::new(line, 1, offset),
        Location::new(line, 2, offset + 1),
    )
}

fn code(text: &str) -> CustomRuleCode {
    CustomRuleCode::new(text).unwrap()
}

fn severity(level: Severity) -> RuleSettings<NoOptions> {
    RuleSettings {
        severity: Some(level),
        ..RuleSettings::default()
    }
}

#[test]
fn a_source_rule_reports_under_its_own_code_and_default_severity() {
    let mut linter = Linter::new();
    linter
        .add_rule(Rule::Source(Box::new(Flags(code("my-rule")))))
        .unwrap();
    let found = linter.lint("a: 1\n").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code.as_str(), "my-rule");
    assert_eq!(found[0].severity, Severity::Hint);
    assert_eq!(found[0].excerpt, Excerpt::SourceLines);
}

#[test]
fn a_source_rule_follows_the_configured_severity() {
    let config = LintConfig::new().with_custom_rule(code("my-rule"), severity(Severity::Error));
    let mut linter = Linter::with_config(config);
    linter
        .add_rule(Rule::Source(Box::new(Flags(code("my-rule")))))
        .unwrap();
    let found = linter.lint("a: 1\n").unwrap();
    assert_eq!(found[0].severity, Severity::Error);
}

#[test]
fn a_document_rule_follows_the_configured_severity_without_reading_it() {
    let make = |config: LintConfig| {
        let mut linter = Linter::with_config(config);
        linter
            .add_rule(Rule::Document(Box::new(FlagsDocuments(code("per-doc")))))
            .unwrap();
        linter.lint("a: 1\n---\nb: 2\n").unwrap()
    };
    let default = make(LintConfig::new());
    assert_eq!(default.len(), 2);
    assert!(default.iter().all(|d| d.code.as_str() == "per-doc"));
    assert!(default.iter().all(|d| d.severity == Severity::Info));
    assert!(default.iter().all(|d| d.excerpt == Excerpt::Omitted));

    let configured =
        make(LintConfig::new().with_custom_rule(code("per-doc"), severity(Severity::Warning)));
    assert!(configured.iter().all(|d| d.severity == Severity::Warning));
}

#[test]
fn diagnose_gives_the_same_result_as_the_linter() {
    let rule = Flags(code("my-rule"));
    let config = LintConfig::new().with_custom_rule(code("my-rule"), severity(Severity::Error));
    let diagnostics = rule.diagnose(&LintContext::new("a: 1\n"), &config);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code.as_str(), "my-rule");
    assert_eq!(diagnostics[0].severity, Severity::Error);
}

/// Source that trips most rules of the default and strict settings.
const MESSY: &str = "a:   1\na: 2\nb: [ 1,2 ]\nc: {x: 1,y: 2 }\nd:  yes\ne: 'quoted'\nf: 0755\n\
g:\nh: .NaN\n #off\n#bad comment\n# fy: nope\nkey :  value   \n\n\n\n\ni: 1";

fn lint_with_every_rule_at(level: &str) -> Vec<fast_yaml_linter::Diagnostic> {
    let mut rules = String::from("rules:\n");
    for name in RuleName::ALL {
        let key = name.as_str();
        writeln!(rules, "  {key}: {{enabled: true, severity: {level}}}").unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, rules).unwrap();
    let (config, _) = ConfigFile::load(&path).unwrap().into_parts();
    Linter::with_config(config).lint(MESSY).unwrap()
}

#[test]
fn every_built_in_rule_reports_under_its_own_code() {
    let found = lint_with_every_rule_at("warning");
    let mut codes: Vec<&str> = found.iter().map(|d| d.code.as_str()).collect();
    codes.sort_unstable();
    codes.dedup();
    assert!(codes.len() >= 12, "only {codes:?} fired");
    for code in &codes {
        assert!(
            code.parse::<RuleName>().is_ok(),
            "{code} is not the code of a built-in rule"
        );
    }
}

#[test]
fn every_built_in_rule_follows_its_configured_severity() {
    for level in ["error", "warning", "info", "hint"] {
        let expected: Severity = level.parse().unwrap();
        let found = lint_with_every_rule_at(level);
        assert_ne!(found.len(), 0, "no diagnostics at severity {level}");
        for diagnostic in &found {
            assert_eq!(
                diagnostic.severity,
                expected,
                "{} ignored severity {level}",
                diagnostic.code.as_str()
            );
        }
    }
}

#[test]
fn lint_directive_reports_under_its_own_code_and_default_severity() {
    let diagnostics = Linter::new().lint("# fy: nope\na: 1\n").unwrap();
    let directive: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_str() == "lint-directive")
        .collect();
    assert_eq!(directive.len(), 1);
    assert_eq!(directive[0].severity, Severity::Warning);
}

#[test]
fn lint_directive_follows_its_configured_severity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, "rules:\n  lint-directive: error\n").unwrap();
    let (config, _) = ConfigFile::load(&path).unwrap().into_parts();
    let diagnostics = Linter::with_config(config)
        .lint("# fy: nope\na: 1\n")
        .unwrap();
    let severities: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_str() == "lint-directive")
        .map(|d| d.severity)
        .collect();
    assert_eq!(severities, [Severity::Error]);
}
