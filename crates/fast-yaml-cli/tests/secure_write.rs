//! Secure atomic write tests for `fy format -i` / `-o` (single-file and batch paths).

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

#[allow(deprecated)]
fn fy() -> Command {
    Command::cargo_bin("fy").unwrap()
}

const UNFORMATTED: &str = "key:   value\n";
const FORMATTED: &str = "key: value\n";

#[test]
fn in_place_leaves_sibling_tmp_file_untouched() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.yaml");
    let sibling = dir.path().join("data.tmp");
    fs::write(&path, UNFORMATTED).unwrap();
    fs::write(&sibling, "precious").unwrap();

    fy().args(["format", "-i", path.to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&path).unwrap(), FORMATTED);
    assert_eq!(fs::read_to_string(&sibling).unwrap(), "precious");
}

#[test]
fn output_file_leaves_sibling_tmp_file_untouched() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.yaml");
    let out = dir.path().join("out.yaml");
    let sibling = dir.path().join("out.tmp");
    fs::write(&input, UNFORMATTED).unwrap();
    fs::write(&sibling, "precious").unwrap();

    fy().args([
        "format",
        input.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ])
    .assert()
    .success();

    assert_eq!(fs::read_to_string(&out).unwrap(), FORMATTED);
    assert_eq!(fs::read_to_string(&sibling).unwrap(), "precious");
}

#[test]
fn batch_leaves_sibling_tmp_file_untouched() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a.yaml");
    let b = dir.path().join("b.yaml");
    let sibling = dir.path().join("a.tmp");
    fs::write(&a, UNFORMATTED).unwrap();
    fs::write(&b, UNFORMATTED).unwrap();
    fs::write(&sibling, "precious").unwrap();

    fy().args(["format", "-i", dir.path().to_str().unwrap()])
        .assert()
        .success();

    assert_eq!(fs::read_to_string(&a).unwrap(), FORMATTED);
    assert_eq!(fs::read_to_string(&sibling).unwrap(), "precious");
}

#[test]
fn output_to_directory_fails_and_leaves_no_temp_files() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.yaml");
    let target = dir.path().join("outdir");
    fs::write(&input, UNFORMATTED).unwrap();
    fs::create_dir(&target).unwrap();

    fy().args([
        "format",
        input.to_str().unwrap(),
        "-o",
        target.to_str().unwrap(),
    ])
    .assert()
    .failure();

    assert!(target.is_dir());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn set_mode(path: &Path, mode: u32) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn in_place_preserves_restrictive_mode() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("secret.yaml");
        fs::write(&path, UNFORMATTED).unwrap();
        set_mode(&path, 0o600);

        fy().args(["format", "-i", path.to_str().unwrap()])
            .assert()
            .success();

        assert_eq!(fs::read_to_string(&path).unwrap(), FORMATTED);
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn in_place_preserves_executable_mode() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("run.yaml");
        fs::write(&path, UNFORMATTED).unwrap();
        set_mode(&path, 0o755);

        fy().args(["format", "-i", path.to_str().unwrap()])
            .assert()
            .success();

        assert_eq!(mode(&path), 0o755);
    }

    #[test]
    fn output_file_preserves_existing_destination_mode() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.yaml");
        let out = dir.path().join("out.yaml");
        fs::write(&input, UNFORMATTED).unwrap();
        fs::write(&out, "old").unwrap();
        set_mode(&out, 0o600);

        fy().args([
            "format",
            input.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

        assert_eq!(fs::read_to_string(&out).unwrap(), FORMATTED);
        assert_eq!(mode(&out), 0o600);
    }

    #[test]
    fn batch_preserves_modes() {
        let dir = TempDir::new().unwrap();
        let private = dir.path().join("private.yaml");
        let shared = dir.path().join("shared.yaml");
        let exec = dir.path().join("exec.yaml");
        for (path, m) in [(&private, 0o600), (&shared, 0o644), (&exec, 0o755)] {
            fs::write(path, UNFORMATTED).unwrap();
            set_mode(path, m);
        }

        fy().args(["format", "-i", dir.path().to_str().unwrap()])
            .assert()
            .success();

        assert_eq!(fs::read_to_string(&private).unwrap(), FORMATTED);
        assert_eq!(mode(&private), 0o600);
        assert_eq!(mode(&shared), 0o644);
        assert_eq!(mode(&exec), 0o755);
    }

    #[test]
    fn planted_tmp_symlink_does_not_clobber_victim() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cfg.yaml");
        let victim = dir.path().join("victim.txt");
        fs::write(&path, UNFORMATTED).unwrap();
        fs::write(&victim, "victim data").unwrap();
        symlink(&victim, dir.path().join("cfg.tmp")).unwrap();

        fy().args(["format", "-i", path.to_str().unwrap()])
            .assert()
            .success();

        assert_eq!(fs::read_to_string(&path).unwrap(), FORMATTED);
        assert_eq!(fs::read_to_string(&victim).unwrap(), "victim data");
    }

    #[test]
    fn output_new_file_follows_umask_like_a_plain_file() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.yaml");
        let out = dir.path().join("new.yaml");
        let plain = dir.path().join("plain.yaml");
        fs::write(&input, UNFORMATTED).unwrap();
        fs::write(&plain, "x").unwrap();

        fy().args([
            "format",
            input.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

        assert_eq!(fs::read_to_string(&out).unwrap(), FORMATTED);
        assert_eq!(mode(&out), mode(&plain));
    }

    #[test]
    fn batch_nested_directories_preserve_modes() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("a").join("b");
        fs::create_dir_all(&nested).unwrap();
        let top = dir.path().join("top.yaml");
        let deep = nested.join("deep.yaml");
        for (path, m) in [(&top, 0o600), (&deep, 0o640)] {
            fs::write(path, UNFORMATTED).unwrap();
            set_mode(path, m);
        }

        fy().args(["format", "-i", dir.path().to_str().unwrap()])
            .assert()
            .success();

        assert_eq!(fs::read_to_string(&deep).unwrap(), FORMATTED);
        assert_eq!(mode(&top), 0o600);
        assert_eq!(mode(&deep), 0o640);
    }

    #[test]
    fn batch_discovery_skips_symlinked_files() {
        let dir = TempDir::new().unwrap();
        let real_dir = dir.path().join("real");
        let scan_dir = dir.path().join("scan");
        fs::create_dir_all(&real_dir).unwrap();
        fs::create_dir_all(&scan_dir).unwrap();
        let real = real_dir.join("real.yaml");
        let link = scan_dir.join("link.yaml");
        let other = scan_dir.join("other.yaml");
        fs::write(&real, UNFORMATTED).unwrap();
        fs::write(&other, UNFORMATTED).unwrap();
        symlink(&real, &link).unwrap();

        fy().args(["format", "-i", scan_dir.to_str().unwrap()])
            .assert()
            .success();

        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read_to_string(&other).unwrap(), FORMATTED);
        assert_eq!(fs::read_to_string(&real).unwrap(), UNFORMATTED);
    }

    #[test]
    fn in_place_in_unwritable_directory_fails_and_keeps_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.yaml");
        fs::write(&path, UNFORMATTED).unwrap();
        set_mode(dir.path(), 0o555);

        // Root ignores directory write bits, which makes the scenario meaningless.
        let probe = dir.path().join("probe");
        let writable_anyway = fs::File::create(&probe).is_ok();
        if writable_anyway {
            fs::remove_file(&probe).unwrap();
        } else {
            fy().args(["format", "-i", path.to_str().unwrap()])
                .assert()
                .failure();
        }

        set_mode(dir.path(), 0o755);
        if !writable_anyway {
            assert_eq!(fs::read_to_string(&path).unwrap(), UNFORMATTED);
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn in_place_on_symlink_updates_target_and_keeps_link() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("real.yaml");
        let link = dir.path().join("link.yaml");
        fs::write(&real, UNFORMATTED).unwrap();
        set_mode(&real, 0o600);
        symlink(&real, &link).unwrap();

        fy().args(["format", "-i", link.to_str().unwrap()])
            .assert()
            .success();

        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), FORMATTED);
        assert_eq!(mode(&real), 0o600);
    }

    #[test]
    fn output_to_symlink_updates_target_and_keeps_link() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("in.yaml");
        let real = dir.path().join("real.yaml");
        let link = dir.path().join("link.yaml");
        fs::write(&input, UNFORMATTED).unwrap();
        fs::write(&real, "old").unwrap();
        symlink(&real, &link).unwrap();

        fy().args([
            "format",
            input.to_str().unwrap(),
            "-o",
            link.to_str().unwrap(),
        ])
        .assert()
        .success();

        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), FORMATTED);
    }
}
