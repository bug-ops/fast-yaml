//! Integration tests for the shared secure atomic writer and batch `format_in_place`.

use fast_yaml_core::EmitterConfig;
use fast_yaml_parallel::{FileProcessor, write_atomic};
use std::fs;
use tempfile::TempDir;

#[test]
fn leaves_sibling_tmp_file_untouched() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.yaml");
    let sibling = dir.path().join("data.tmp");
    fs::write(&path, "old").unwrap();
    fs::write(&sibling, "precious").unwrap();

    write_atomic(&path, b"new").unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(fs::read_to_string(&sibling).unwrap(), "precious");
}

#[test]
fn leaves_no_temp_files_behind() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("data.yaml");

    write_atomic(&path, b"one").unwrap();
    write_atomic(&path, b"two").unwrap();

    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn preserves_mode() {
        let dir = TempDir::new().unwrap();
        for m in [0o600, 0o644, 0o755] {
            let path = dir.path().join(format!("f{m:o}.yaml"));
            fs::write(&path, "old").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(m)).unwrap();

            write_atomic(&path, b"new").unwrap();

            assert_eq!(mode(&path), m);
        }
    }

    #[test]
    fn unwritable_directory_fails_without_leaking_temp_name() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.yaml");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();

        // Root ignores directory write bits, which makes the scenario meaningless.
        let probe = dir.path().join("probe");
        if fs::File::create(&probe).is_ok() {
            fs::remove_file(&probe).unwrap();
            return;
        }

        let err = write_atomic(&path, b"new").unwrap_err();

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        let message = err.to_string();
        assert!(message.starts_with("cannot create temporary file in "));
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn new_file_mode_follows_umask() {
        let dir = TempDir::new().unwrap();
        let created = dir.path().join("created.yaml");
        let plain = dir.path().join("plain.yaml");

        write_atomic(&created, b"x").unwrap();
        fs::write(&plain, "x").unwrap();

        assert_eq!(mode(&created), mode(&plain));
    }

    #[test]
    fn chained_and_relative_symlinks_resolve_to_real_file() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let real = sub.join("real.yaml");
        fs::write(&real, "old").unwrap();
        symlink("real.yaml", sub.join("hop1.yaml")).unwrap();
        symlink("sub/hop1.yaml", dir.path().join("hop2.yaml")).unwrap();

        write_atomic(&dir.path().join("hop2.yaml"), b"new").unwrap();

        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
        assert!(
            fs::symlink_metadata(dir.path().join("hop2.yaml"))
                .unwrap()
                .is_symlink()
        );
        assert!(
            fs::symlink_metadata(sub.join("hop1.yaml"))
                .unwrap()
                .is_symlink()
        );
    }

    #[test]
    fn write_error_paths_are_cleaned_up() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("subdir");
        fs::create_dir(&target).unwrap();

        assert!(write_atomic(&target, b"x").is_err());

        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    }

    #[test]
    fn symlink_target_is_updated_in_place_of_link() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("real.yaml");
        let link = dir.path().join("link.yaml");
        fs::write(&real, "old").unwrap();
        symlink(&real, &link).unwrap();

        write_atomic(&link, b"new").unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
    }

    #[test]
    fn format_in_place_preserves_modes() {
        let dir = TempDir::new().unwrap();
        let mut paths = Vec::new();
        for (name, m) in [("a.yaml", 0o600), ("b.yaml", 0o644), ("c.yaml", 0o755)] {
            let path = dir.path().join(name);
            fs::write(&path, "key:   value\n").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(m)).unwrap();
            paths.push((path, m));
        }
        let only_paths: Vec<_> = paths.iter().map(|(p, _)| p.clone()).collect();

        let result = FileProcessor::new().format_in_place(&only_paths, &EmitterConfig::new());

        assert_eq!(result.failed, 0);
        for (path, m) in &paths {
            assert_eq!(fs::read_to_string(path).unwrap(), "key: value\n");
            assert_eq!(mode(path), *m);
        }
    }
}
