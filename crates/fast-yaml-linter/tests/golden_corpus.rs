//! Golden snapshot of every rule over every fixture, in `parsable` format.
//!
//! The snapshots in `tests/golden/` pin lint behavior across refactors. Regenerate them after an
//! intended behavior change with `UPDATE_GOLDEN=1 cargo nextest run -p fast-yaml-linter --test
//! golden_corpus` and review the diff.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use fast_yaml_linter::formatter::{FileReport, ReportFormat, ReportSource};
use fast_yaml_linter::{ConfigFile, LintConfig, Linter};

const STRICT: &str = "\
extends: default
rules:
  document-start: {present: required}
  document-end: {present: required}
  quoted-strings: {required: true, check-keys: true}
  truthy: {check-keys: true}
  float-values:
    require-numeral-before-decimal: true
    forbid-scientific-notation: true
    forbid-nan: true
    forbid-inf: true
  empty-values:
    forbid-in-block-mappings: true
    forbid-in-flow-mappings: true
    forbid-in-block-sequences: true
  octal-values: {forbid-implicit-octal: true, forbid-explicit-octal: true}
  key-ordering: enable
  set-values: enable
  line-length: {max: 60}
  comments-indentation: enable
  comments: {require-starting-space: true, min-spaces-from-content: 2}
  braces: {min-spaces-inside: 1, max-spaces-inside: 1}
  brackets: {min-spaces-inside: 1, max-spaces-inside: 1}
";

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, out);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml" | "yml")
        ) {
            out.push(path);
        }
    }
}

fn corpus() -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect(&fixtures_root(), &mut files);
    files.sort();
    files
}

fn render(config: &LintConfig) -> String {
    let linter = Linter::with_config(config.clone());
    let root = fixtures_root();
    let mut out = String::new();
    for path in corpus() {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let source = String::from_utf8_lossy(&fs::read(&path).unwrap()).into_owned();
        match linter.lint(&source) {
            Ok(diagnostics) => {
                let source = ReportSource::Stdin;
                let report = FileReport {
                    source: &source,
                    diagnostics: &diagnostics,
                };
                for line in ReportFormat::Parsable.render(&[report]).lines() {
                    let _ = writeln!(out, "{rel}{}", line.strip_prefix("stdin").unwrap());
                }
            }
            Err(error) => {
                let _ = writeln!(
                    out,
                    "{rel}: lint failed: {}",
                    error.to_string().replace('\n', " ")
                );
            }
        }
    }
    out
}

fn check(name: &str, config: &LintConfig) {
    let actual = render(config);
    let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::write(&golden, &actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&golden)
        .unwrap_or_else(|_| panic!("missing {}; run with UPDATE_GOLDEN=1", golden.display()));
    assert!(
        actual == expected,
        "{name} differs from the golden snapshot; first differing line: {:?}",
        actual
            .lines()
            .zip(expected.lines())
            .find(|(a, e)| a != e)
            .map(|(a, e)| format!("actual {a:?} vs expected {e:?}"))
    );
}

#[test]
fn default_config_matches_golden() {
    check("default.txt", &LintConfig::default());
}

#[test]
fn strict_parity_config_matches_golden() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("strict.yaml");
    fs::write(&path, STRICT).unwrap();
    let (config, _) = ConfigFile::load(&path).unwrap().into_parts();
    check("strict.txt", &config);
}
