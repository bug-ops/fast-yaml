//! `format -i` and hard links: files found by a walk are refused, named files are written through.

#![cfg(unix)]
#![allow(clippy::missing_docs_in_private_items)]

use assert_cmd::Command;
use assert_cmd::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use tempfile::TempDir;

const UNFORMATTED: &str = "key:   value\n";
const FORMATTED: &str = "key: value\n";

fn fy() -> Command {
    cargo_bin_cmd!("fy")
}

/// A directory holding `plain.yaml`, `linked.yaml` and its second name `alias.yaml`.
struct Fixture {
    _temp: TempDir,
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("tree");
        fs::create_dir(&dir).unwrap();
        for name in ["plain.yaml", "linked.yaml"] {
            fs::write(dir.join(name), UNFORMATTED).unwrap();
        }
        fs::hard_link(dir.join("linked.yaml"), dir.join("alias.yaml")).unwrap();
        Self { _temp: temp, dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.path(name)).unwrap()
    }

    fn links(&self, name: &str) -> u64 {
        fs::metadata(self.path(name)).unwrap().nlink()
    }
}

#[test]
fn a_directory_walk_refuses_hard_linked_files_and_formats_the_rest() {
    let fixture = Fixture::new();

    fy().args(["format", "-i"])
        .arg(&fixture.dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "refused to write through 2 hard links",
        ))
        .stderr(predicate::str::contains(
            "name the file as a path argument to write through its links",
        ));

    assert_eq!(fixture.read("plain.yaml"), FORMATTED);
    assert_eq!(fixture.read("linked.yaml"), UNFORMATTED);
    assert_eq!(fixture.read("alias.yaml"), UNFORMATTED);
    assert_eq!(fixture.links("linked.yaml"), 2);
}

#[test]
fn a_glob_refuses_hard_linked_files() {
    let fixture = Fixture::new();
    let pattern = fixture.path("*.yaml");

    fy().args(["format", "-i"])
        .arg(pattern)
        .assert()
        .failure()
        .stderr(predicate::str::contains("hard links"));

    assert_eq!(fixture.read("linked.yaml"), UNFORMATTED);
    assert_eq!(fixture.links("linked.yaml"), 2);
}

#[test]
fn a_path_argument_is_written_through_its_links() {
    let fixture = Fixture::new();

    fy().args(["format", "-i"])
        .arg(fixture.path("linked.yaml"))
        .arg(fixture.path("plain.yaml"))
        .assert()
        .success();

    assert_eq!(fixture.read("linked.yaml"), FORMATTED);
    assert_eq!(fixture.read("alias.yaml"), FORMATTED);
    assert_eq!(fixture.links("linked.yaml"), 2);
}

#[test]
fn a_stdin_listed_file_is_written_through_its_links() {
    let fixture = Fixture::new();
    let list = format!("{}\n", fixture.path("linked.yaml").display());

    fy().args(["format", "-i", "--stdin-files"])
        .write_stdin(list)
        .assert()
        .success();

    assert_eq!(fixture.read("alias.yaml"), FORMATTED);
    assert_eq!(fixture.links("linked.yaml"), 2);
}

#[test]
fn naming_a_file_found_by_the_walk_writes_it_through_in_either_order() {
    for named_first in [false, true] {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("tree");
        fs::create_dir(&dir).unwrap();
        let linked = dir.join("linked.yaml");
        let outside = temp.path().join("outside.yaml");
        fs::write(&linked, UNFORMATTED).unwrap();
        fs::hard_link(&linked, &outside).unwrap();
        let mut command = fy();
        command.args(["format", "-i"]);
        if named_first {
            command.arg(&linked).arg(&dir);
        } else {
            command.arg(&dir).arg(&linked);
        }

        command.assert().success();

        assert_eq!(
            fs::read_to_string(&outside).unwrap(),
            FORMATTED,
            "{named_first}"
        );
        assert_eq!(fs::metadata(&linked).unwrap().nlink(), 2);
    }
}
