//! Integration tests for the shared secure atomic writer and batch `format_in_place`.

use fast_yaml_parallel::HardLinkPolicy;
use std::io;
use std::path::Path;

fn write_atomic(path: &Path, content: &[u8]) -> io::Result<()> {
    fast_yaml_parallel::write_atomic(path, content, HardLinkPolicy::WriteInPlace)
}
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
    use fast_yaml_core::EmitterConfig;
    use fast_yaml_parallel::{CommentPolicy, FileProcessor, InPlaceTarget};
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
        let Some(fast_yaml_parallel::AtomicWriteError::TempFile { source, .. }) = err
            .get_ref()
            .and_then(|e| e.downcast_ref::<fast_yaml_parallel::AtomicWriteError>())
        else {
            panic!("not TempFile: {err}");
        };
        assert_eq!(source.kind(), std::io::ErrorKind::PermissionDenied);
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
        let only_paths: Vec<_> = paths
            .iter()
            .map(|(p, _)| InPlaceTarget::new(p.clone(), HardLinkPolicy::WriteInPlace))
            .collect();

        let result = FileProcessor::new().format_in_place(
            &only_paths,
            &EmitterConfig::new(),
            CommentPolicy::Reject,
        );

        assert_eq!(result.failed, 0);
        for (path, m) in &paths {
            assert_eq!(fs::read_to_string(path).unwrap(), "key: value\n");
            assert_eq!(mode(path), *m);
        }
    }

    #[test]
    fn format_in_place_applies_the_policy_of_each_target() {
        use fast_yaml_parallel::AtomicWriteError;
        use std::os::unix::fs::MetadataExt;

        let dir = TempDir::new().unwrap();
        let write = |name: &str| {
            let path = dir.path().join(name);
            fs::write(&path, "key:   value\n").unwrap();
            path
        };
        let (refused, refused_alias) = (write("refused.yaml"), dir.path().join("refused2.yaml"));
        fs::hard_link(&refused, &refused_alias).unwrap();
        let (through, through_alias) = (write("through.yaml"), dir.path().join("through2.yaml"));
        fs::hard_link(&through, &through_alias).unwrap();
        let plain = write("plain.yaml");
        let targets = [
            InPlaceTarget::new(refused.clone(), HardLinkPolicy::Refuse),
            InPlaceTarget::new(through.clone(), HardLinkPolicy::WriteInPlace),
            InPlaceTarget::new(plain.clone(), HardLinkPolicy::Refuse),
        ];

        let result = FileProcessor::new().format_in_place(
            &targets,
            &EmitterConfig::new(),
            CommentPolicy::Reject,
        );

        assert_eq!((result.total, result.failed, result.changed), (3, 1, 2));
        let (path, error) = &result.errors[0];
        assert_eq!(path, &refused);
        let io_error = match error {
            fast_yaml_parallel::Error::Write { source, .. } => source,
            other => panic!("unexpected error: {other}"),
        };
        assert!(matches!(
            io_error
                .get_ref()
                .and_then(|e| e.downcast_ref::<AtomicWriteError>()),
            Some(AtomicWriteError::HardLinkRefused(_))
        ));
        for untouched in [&refused, &refused_alias] {
            assert_eq!(fs::read_to_string(untouched).unwrap(), "key:   value\n");
        }
        for formatted in [&through, &through_alias, &plain] {
            assert_eq!(fs::read_to_string(formatted).unwrap(), "key: value\n");
        }
        assert_eq!(fs::metadata(&through).unwrap().nlink(), 2);
    }

    #[test]
    fn format_in_place_writes_two_names_of_one_file_one_after_the_other() {
        use fast_yaml_parallel::{Config, WorkerCount, Workers};
        use std::os::unix::fs::MetadataExt;

        let dir = TempDir::new().unwrap();
        let write = |name: &str| {
            let path = dir.path().join(name);
            fs::write(&path, "key:   value\n").unwrap();
            path
        };
        let first = write("first.yaml");
        let second = dir.path().join("second.yaml");
        fs::hard_link(&first, &second).unwrap();
        let mut paths = vec![first.clone(), second.clone()];
        paths.extend((0..10).map(|i| write(&format!("filler{i}.yaml"))));
        let targets: Vec<_> = paths
            .iter()
            .map(|path| InPlaceTarget::new(path.clone(), HardLinkPolicy::WriteInPlace))
            .collect();
        let config = Config::new().with_workers(Workers::Fixed(WorkerCount::new(4).unwrap()));

        let result = FileProcessor::with_config(config).format_in_place(
            &targets,
            &EmitterConfig::new(),
            CommentPolicy::Reject,
        );

        // The second name finds the file already formatted only when it runs after the first
        assert_eq!((result.total, result.failed, result.changed), (12, 0, 11));
        for path in [&first, &second] {
            assert_eq!(fs::read_to_string(path).unwrap(), "key: value\n");
        }
        assert_eq!(fs::metadata(&first).unwrap().nlink(), 2);
    }
}
