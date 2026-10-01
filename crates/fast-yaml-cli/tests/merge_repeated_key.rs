//! Pins a deliberate deviation from `PyYAML`: a repeated `<<` key in one mapping is an error.

use assert_cmd::cargo_bin_cmd;

#[test]
fn repeated_merge_key_in_one_mapping_is_rejected_unlike_pyyaml() {
    let yaml = "a: &a {x: 1}\nb: &b {y: 2}\nm:\n  <<: *a\n  <<: *b\n";
    for subcommand in [&["format"][..], &["convert", "json"][..]] {
        let output = cargo_bin_cmd!("fy")
            .args(subcommand)
            .write_stdin(yaml)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{subcommand:?}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("duplicate merge key"),
            "{subcommand:?}: {stderr}"
        );
    }
}
