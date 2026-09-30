//! Rule to check line ending type.

use serde::{Deserialize, Serialize};

use crate::config::RuleOptions;
use crate::{
    Diagnostic, DiagnosticBuilder, DiagnosticCode, LintConfig, LintContext, Location, Severity,
    Span,
};
use fast_yaml_core::Value;

/// Linting rule for line endings.
///
/// Enforces consistent line ending type:
/// - Unix: `\n` (LF)
/// - DOS: `\r\n` (CRLF)
/// - Platform: OS-dependent (Unix on Unix-like, DOS on Windows)
///
/// Configuration options:
/// - `type`: string - "unix", "dos", or "platform" (default: "unix")
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::{rules::NewLinesRule, rules::LintRule, LintConfig};
/// use fast_yaml_core::Parser;
///
/// let rule = NewLinesRule;
/// let yaml = "key: value\nanother: value";
/// let value = Parser::parse_str(yaml).unwrap().unwrap();
///
/// let config = LintConfig::default();
/// let diagnostics = rule.check(&fast_yaml_linter::LintContext::new(yaml), &value, &config);
/// assert!(diagnostics.is_empty());
/// ```
pub struct NewLinesRule;

/// Line ending style required by the new-lines rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineEndingType {
    /// `\n` line endings.
    #[default]
    Unix,
    /// `\r\n` line endings.
    Dos,
    /// `\r\n` on Windows and `\n` elsewhere.
    Platform,
}

/// Options of the new-lines rule.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
pub struct NewLinesOptions {
    /// Required line ending style.
    #[serde(rename = "type")]
    pub line_ending: LineEndingType,
}

impl RuleOptions for NewLinesOptions {}

impl super::LintRule for NewLinesRule {
    fn code(&self) -> &str {
        DiagnosticCode::NEW_LINES
    }

    fn name(&self) -> &'static str {
        "New Lines"
    }

    fn description(&self) -> &'static str {
        "Enforces line ending type (Unix/DOS)"
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, context: &LintContext, _value: &Value, config: &LintConfig) -> Vec<Diagnostic> {
        let source = context.source();
        let expected = match config.rules.new_lines.options.line_ending {
            LineEndingType::Dos => LineEnding::Dos,
            LineEndingType::Platform => {
                #[cfg(target_os = "windows")]
                {
                    LineEnding::Dos
                }
                #[cfg(not(target_os = "windows"))]
                {
                    LineEnding::Unix
                }
            }
            LineEndingType::Unix => LineEnding::Unix,
        };

        let mut diagnostics = Vec::new();
        let bytes = source.as_bytes();
        let mut offset = 0;
        let mut line_num = 1;

        for (idx, &byte) in bytes.iter().enumerate() {
            if byte == b'\n' {
                // Check if preceded by \r
                let has_cr = idx.checked_sub(1).and_then(|prev| bytes.get(prev)) == Some(&b'\r');
                let actual = if has_cr {
                    LineEnding::Dos
                } else {
                    LineEnding::Unix
                };

                if actual != expected {
                    let severity = config.rules.new_lines.severity_or(self.default_severity());

                    let location = Location::new(line_num, 1, offset);
                    let span = Span::new(location, location);

                    let expected_str = match expected {
                        LineEnding::Unix => "Unix (\\n)",
                        LineEnding::Dos => "DOS (\\r\\n)",
                    };

                    let actual_str = match actual {
                        LineEnding::Unix => "Unix (\\n)",
                        LineEnding::Dos => "DOS (\\r\\n)",
                    };

                    diagnostics.push(
                        DiagnosticBuilder::new(
                            self.code(),
                            severity,
                            format!(
                                "wrong line ending (expected {expected_str}, found {actual_str})"
                            ),
                            span,
                        )
                        .build_with_context(context.source_context()),
                    );
                }

                line_num += 1;
                offset = idx + 1;
            }
        }

        diagnostics
    }
}

/// Line ending type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineEnding {
    /// Unix line ending (\n)
    Unix,
    /// DOS line ending (\r\n)
    Dos,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{RuleName, test_support::config_with_rule},
        rules::LintRule,
    };
    use fast_yaml_core::Parser;

    #[test]
    fn test_new_lines_unix_valid() {
        let yaml = "key: value\nanother: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLinesRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);

        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_new_lines_dos_in_unix() {
        let yaml = "key: value\r\nanother: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLinesRule;
        let config = LintConfig::default();
        let context = LintContext::new(yaml);

        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("wrong line ending"));
        assert!(diagnostics[0].message.contains("DOS"));
    }

    #[test]
    fn test_new_lines_dos_valid() {
        let yaml = "key: value\r\nanother: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLinesRule;
        let config = config_with_rule(RuleName::NewLines, "{type: dos}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_new_lines_unix_in_dos() {
        let yaml = "key: value\nanother: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLinesRule;
        let config = config_with_rule(RuleName::NewLines, "{type: dos}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("wrong line ending"));
    }

    #[test]
    fn test_new_lines_mixed() {
        let yaml = "key: value\nanother: value\r\nthird: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLinesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Should report the DOS line
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_new_lines_platform() {
        let yaml = "key: value\nanother: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLinesRule;
        let config = config_with_rule(RuleName::NewLines, "{type: platform}");

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // On Unix platforms, this should be valid
        #[cfg(not(target_os = "windows"))]
        assert!(diagnostics.is_empty());

        // On Windows, should report error
        #[cfg(target_os = "windows")]
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_new_lines_no_newlines() {
        let yaml = "key: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLinesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_new_lines_multiple_violations() {
        let yaml = "key: value\r\nanother: value\r\nthird: value\r\n";
        let value = Parser::parse_str(yaml).unwrap().unwrap();

        let rule = NewLinesRule;
        let config = LintConfig::default();

        let context = LintContext::new(yaml);
        let diagnostics = rule.check(&context, &value, &config);
        // Should report all DOS line endings
        assert_eq!(diagnostics.len(), 3);
    }

    #[test]
    fn test_line_ending_equality() {
        assert_eq!(LineEnding::Unix, LineEnding::Unix);
        assert_eq!(LineEnding::Dos, LineEnding::Dos);
        assert_ne!(LineEnding::Unix, LineEnding::Dos);
    }
}
