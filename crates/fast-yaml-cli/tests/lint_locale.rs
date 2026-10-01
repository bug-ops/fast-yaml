//! The top-level `locale` key of the config file (#585).

#![allow(clippy::missing_docs_in_private_items)]

use std::fs;
use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

fn fy(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = assert_cmd::cargo_bin_cmd!("fy");
    cmd.current_dir(dir).args(args);
    cmd
}

fn project(config: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("cfg.yaml"), config).unwrap();
    fs::write(dir.path().join("a.yaml"), "b: 1\na: 2\n").unwrap();
    dir
}

#[test]
fn c_locales_are_accepted_and_order_keys_by_code_point() {
    for locale in ["C", "POSIX", "C.UTF-8"] {
        let dir = project(&format!("locale: {locale}\n"));
        let out = fy(
            dir.path(),
            &[
                "lint", "--config", "cfg.yaml", "--format", "parsable", "a.yaml",
            ],
        )
        .output()
        .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("(key-ordering)"), "{locale}: {stdout}");
    }
}

#[test]
fn another_locale_is_rejected_while_key_ordering_is_enabled() {
    let dir = project("locale: en_US.UTF-8\n");
    let out = fy(dir.path(), &["lint", "--config", "cfg.yaml", "a.yaml"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("en_US.UTF-8") && stderr.contains("key-ordering"),
        "{stderr}"
    );
}

#[test]
fn another_locale_is_inert_while_key_ordering_is_disabled() {
    for config in [
        "locale: en_US.UTF-8\nrules:\n  key-ordering: disable\n",
        "locale: en_US.UTF-8\nextends: default\n",
    ] {
        let dir = project(config);
        let out = fy(dir.path(), &["lint", "--config", "cfg.yaml", "a.yaml"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0), "{config}");
    }
}
