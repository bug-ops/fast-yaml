//! Rule to check indentation consistency.

use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;

/// Rule to check indentation consistency.
pub struct IndentationRule;

impl super::LintRule for IndentationRule {
    fn code(&self) -> &str {
        DiagnosticCode::INDENTATION
    }

    fn name(&self) -> &'static str {
        "Indentation"
    }

    fn description(&self) -> &'static str {
        "Checks for consistent indentation throughout the YAML file"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let ctx = context.source_context();
        let indent_size = config.indent_size;
        let mut diagnostics = Vec::new();

        for line_num in 1..=ctx.line_count() {
            let Some(line) = ctx.get_line(line_num) else {
                continue;
            };

            // Count leading spaces and tabs separately.
            let leading_spaces = line.chars().take_while(|c| *c == ' ').count();
            let leading_tabs = line.chars().take_while(|c| *c == '\t').count();

            // No indentation — nothing to check.
            if leading_spaces == 0 && leading_tabs == 0 {
                continue;
            }

            let line_offset = ctx.get_line_offset(line_num);

            // Detect mixed tabs and spaces: the line starts with spaces and then
            // has a tab, or starts with tabs and then has a space.
            let has_mixed = {
                let mut chars = line.chars();
                let first_ws = chars.next();
                match first_ws {
                    Some(' ') => chars
                        .take_while(char::is_ascii_whitespace)
                        .any(|c| c == '\t'),
                    Some('\t') => chars
                        .take_while(char::is_ascii_whitespace)
                        .any(|c| c == ' '),
                    _ => false,
                }
            };

            if has_mixed {
                let indent_width = line.chars().take_while(char::is_ascii_whitespace).count();
                let span = Span::new(
                    Location::new(line_num, 1, line_offset),
                    Location::new(line_num, indent_width + 1, line_offset + indent_width),
                );
                let diagnostic = DiagnosticBuilder::new(
                    DiagnosticCode::INDENTATION,
                    config.get_effective_severity(self.code(), self.default_severity()),
                    "mixed tabs and spaces in indentation".to_string(),
                    span,
                )
                .build_with_context(context.source_context());
                diagnostics.push(diagnostic);
                continue;
            }

            // Only check space-based indentation for indent-size violations.
            // Tab-only indentation is self-consistent and not flagged.
            if leading_tabs > 0 {
                continue;
            }

            if indent_size > 0 && leading_spaces % indent_size != 0 {
                let span = Span::new(
                    Location::new(line_num, 1, line_offset),
                    Location::new(line_num, leading_spaces + 1, line_offset + leading_spaces),
                );
                let diagnostic = DiagnosticBuilder::new(
                    DiagnosticCode::INDENTATION,
                    config.get_effective_severity(self.code(), self.default_severity()),
                    format!(
                        "wrong indentation: found {leading_spaces} space(s), expected a multiple of {indent_size}"
                    ),
                    span,
                )
                .build_with_context(context.source_context());
                diagnostics.push(diagnostic);
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LintConfig, LintContext, config::RuleConfig, rules::LintRule};
    use fast_yaml_core::Parser;

    fn parse(yaml: &str) -> Value {
        Parser::parse_str(yaml).unwrap().unwrap()
    }

    #[test]
    fn test_correct_2space_indent() {
        let yaml = "parent:\n  child: value\n  nested:\n    deep: ok\n";
        let value = parse(yaml);
        let rule = IndentationRule;
        let config = LintConfig::default();
        let ctx = LintContext::new(yaml);
        assert!(rule.check(&ctx, &value, &config).is_empty());
    }

    #[test]
    fn test_wrong_indent_size() {
        let yaml = "parent:\n   child: value\n";
        let value = parse(yaml);
        let rule = IndentationRule;
        let config = LintConfig::default(); // indent_size = 2
        let ctx = LintContext::new(yaml);
        let diagnostics = rule.check(&ctx, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("wrong indentation"));
        assert_eq!(diagnostics[0].span.start.line, 2);
    }

    #[test]
    fn test_mixed_tabs_and_spaces() {
        // Tab indentation is illegal YAML, so we parse a valid document and pass
        // the invalid source to the rule directly (the rule only reads source text).
        let valid_yaml = "parent:\n  child: value\n";
        let value = parse(valid_yaml);
        // Source with mixed leading whitespace (tab then space).
        let mixed_source = "parent:\n\t child: value\n";
        let rule = IndentationRule;
        let config = LintConfig::default();
        let ctx = LintContext::new(mixed_source);
        let diagnostics = rule.check(&ctx, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("mixed tabs and spaces"));
    }

    #[test]
    fn test_top_level_no_indent() {
        let yaml = "key: value\nother: 42\n";
        let value = parse(yaml);
        let rule = IndentationRule;
        let config = LintConfig::default();
        let ctx = LintContext::new(yaml);
        assert!(rule.check(&ctx, &value, &config).is_empty());
    }

    #[test]
    fn test_indent_size_4_correct() {
        let yaml = "parent:\n    child: value\n";
        let value = parse(yaml);
        let rule = IndentationRule;
        let config = LintConfig::new().with_indent_size(4);
        let ctx = LintContext::new(yaml);
        assert!(rule.check(&ctx, &value, &config).is_empty());
    }

    #[test]
    fn test_indent_size_4_wrong() {
        let yaml = "parent:\n  child: value\n";
        let value = parse(yaml);
        let rule = IndentationRule;
        let config = LintConfig::new().with_indent_size(4);
        let ctx = LintContext::new(yaml);
        let diagnostics = rule.check(&ctx, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("wrong indentation"));
    }

    #[test]
    fn test_tabs_only_no_diagnostic() {
        // Tab indentation is rejected by the YAML parser, but the rule should
        // not emit a wrong-indent-size diagnostic for tab-only leading whitespace.
        let valid_yaml = "parent:\n  child: value\n";
        let value = parse(valid_yaml);
        let tab_source = "parent:\n\tchild: value\n";
        let rule = IndentationRule;
        let config = LintConfig::default();
        let ctx = LintContext::new(tab_source);
        assert!(rule.check(&ctx, &value, &config).is_empty());
    }

    #[test]
    fn test_severity_override() {
        let yaml = "parent:\n   child: value\n";
        let value = parse(yaml);
        let rule = IndentationRule;
        let config = LintConfig::new().with_rule_config(
            "indentation",
            RuleConfig::new().with_severity(Severity::Error),
        );
        let ctx = LintContext::new(yaml);
        let diagnostics = rule.check(&ctx, &value, &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn test_multiple_violations() {
        // Lines 2 and 3 have 3-space indent (not multiple of 2); line 4 has 6-space (ok).
        let yaml = "parent:\n   child: value\n   nested:\n      deep: bad\n";
        let value = parse(yaml);
        let rule = IndentationRule;
        let config = LintConfig::default();
        let ctx = LintContext::new(yaml);
        let diagnostics = rule.check(&ctx, &value, &config);
        assert_eq!(diagnostics.len(), 2);
    }
}
