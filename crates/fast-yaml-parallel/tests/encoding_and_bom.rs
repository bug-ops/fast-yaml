//! Batch processing reports BOM-less UTF-16/UTF-32 as unsupported and keeps a leading UTF-8 BOM.

use std::fs;
use std::path::PathBuf;

use fast_yaml_core::EmitterConfig;
use fast_yaml_parallel::{CommentPolicy, FileProcessor};
use tempfile::TempDir;

const BOM: &str = "\u{FEFF}";

fn write(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, bytes).unwrap();
    path
}

fn wide(text: &str, width: usize, big_endian: bool) -> Vec<u8> {
    text.bytes()
        .flat_map(|b| {
            let mut unit = vec![0u8; width];
            unit[if big_endian { width - 1 } else { 0 }] = b;
            unit
        })
        .collect()
}

#[test]
fn bom_less_wide_encodings_are_reported_as_unsupported() {
    let dir = TempDir::new().unwrap();
    let processor = FileProcessor::new();
    for (name, bytes) in [
        ("u16le.yaml", wide("a: 1\n", 2, false)),
        ("u16be.yaml", wide("a: 1\n", 2, true)),
        ("u32le.yaml", wide("a: 1\n", 4, false)),
        ("u32be.yaml", wide("a: 1\n", 4, true)),
    ] {
        let path = write(&dir, name, &bytes);
        let parsed = processor.parse_files(std::slice::from_ref(&path));
        assert_eq!(parsed.failed, 1, "{name}");
        let message = parsed.errors[0].1.to_string().to_lowercase();
        assert!(
            message.contains("unsupported encoding"),
            "{name}: {message}"
        );

        let formatted = processor.format_files(
            std::slice::from_ref(&path),
            &EmitterConfig::default(),
            CommentPolicy::Strip,
        );
        let message = formatted[0]
            .1
            .as_ref()
            .unwrap_err()
            .to_string()
            .to_lowercase();
        assert!(
            message.contains("unsupported encoding"),
            "{name}: {message}"
        );
    }
}

#[test]
fn a_leading_bom_survives_formatting() {
    let dir = TempDir::new().unwrap();
    let path = write(&dir, "bom.yaml", format!("{BOM}# c\na:   1\n").as_bytes());
    let results = FileProcessor::new().format_files(
        std::slice::from_ref(&path),
        &EmitterConfig::default(),
        CommentPolicy::Strip,
    );
    let output = results[0].1.as_ref().unwrap();
    assert_eq!(output.formatted, format!("{BOM}a: 1\n"));
}
