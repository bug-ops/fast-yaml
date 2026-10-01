//! Behavior of the shared bounded regular-file reader (#593).

use std::path::PathBuf;

use fast_yaml_core::fs::{NotRegularKind, ReadFileError, read_regular_file};
use fast_yaml_core::limits::MaxInputBytes;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fy-core-fs-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn reads_a_regular_file() {
    let dir = scratch("regular");
    let file = dir.join("a.yaml");
    std::fs::write(&file, "a: 1\n").unwrap();
    assert_eq!(
        read_regular_file(&file, MaxInputBytes::DEFAULT).unwrap(),
        b"a: 1\n"
    );
}

#[test]
fn rejects_a_file_over_the_limit_and_accepts_one_at_it() {
    let dir = scratch("limit");
    let file = dir.join("a.yaml");
    std::fs::write(&file, "key: value").unwrap();
    let err = read_regular_file(&file, MaxInputBytes::new(9).unwrap()).unwrap_err();
    assert!(
        matches!(&err, ReadFileError::TooLarge(e) if e.size == 10),
        "{err:?}"
    );
    assert!(read_regular_file(&file, MaxInputBytes::new(10).unwrap()).is_ok());
}

#[test]
fn missing_path_is_an_io_error() {
    let dir = scratch("missing");
    let err = read_regular_file(&dir.join("nope.yaml"), MaxInputBytes::DEFAULT).unwrap_err();
    assert!(matches!(err, ReadFileError::Io(_)), "{err:?}");
}

#[test]
fn directory_is_not_regular() {
    let dir = scratch("dir");
    let err = read_regular_file(&dir, MaxInputBytes::DEFAULT).unwrap_err();
    assert!(
        matches!(err, ReadFileError::NotRegular(NotRegularKind::Directory)),
        "{err:?}"
    );
    assert!(err.to_string().contains("directory"));
}

#[cfg(unix)]
#[test]
fn fifo_is_rejected_without_blocking() {
    let dir = scratch("fifo");
    let fifo = dir.join("pipe.yaml");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let err = read_regular_file(&fifo, MaxInputBytes::DEFAULT).unwrap_err();
    assert!(
        matches!(err, ReadFileError::NotRegular(NotRegularKind::Special)),
        "{err:?}"
    );
}

#[cfg(unix)]
#[test]
fn symlink_to_a_file_is_followed_and_to_a_directory_is_rejected() {
    use std::os::unix::fs::symlink;

    let dir = scratch("symlink");
    let target = dir.join("t.yaml");
    std::fs::write(&target, "x: 1\n").unwrap();
    symlink(&target, dir.join("link.yaml")).unwrap();
    symlink(&dir, dir.join("dirlink")).unwrap();
    assert!(read_regular_file(&dir.join("link.yaml"), MaxInputBytes::DEFAULT).is_ok());
    assert!(matches!(
        read_regular_file(&dir.join("dirlink"), MaxInputBytes::DEFAULT),
        Err(ReadFileError::NotRegular(NotRegularKind::Directory))
    ));
}
