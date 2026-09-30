//! `Config::with_max_documents`: rejected before any document is parsed.

use std::num::NonZeroUsize;

use fast_yaml_parallel::{Config, Error, parse_parallel_with_config};

const fn limit(max: usize) -> NonZeroUsize {
    NonZeroUsize::new(max).unwrap()
}

fn configs(max: usize) -> [Config; 2] {
    [
        Config::new().with_max_documents(limit(max)),
        Config::new()
            .with_max_documents(limit(max))
            .with_workers(Some(2))
            .with_sequential_threshold(0),
    ]
}

#[test]
fn stream_at_the_limit_is_accepted() {
    for config in configs(3) {
        let docs = parse_parallel_with_config("a: 1\n---\nb: 2\n---\nc: 3\n", &config).unwrap();
        assert_eq!(docs.len(), 3);
    }
}

#[test]
fn stream_over_the_limit_reports_the_first_excess_document() {
    for config in configs(2) {
        let err = parse_parallel_with_config("a: 1\n---\nb: 2\n---\nc: 3\n---\nd: 4\n", &config)
            .unwrap_err();
        let Error::DocumentLimitExceeded { max, index } = err else {
            panic!("expected DocumentLimitExceeded, got {err:?}");
        };
        assert_eq!((max.get(), index), (2, 2));
    }
}

#[test]
fn limit_error_is_reported_before_a_later_syntax_error() {
    for config in configs(1) {
        let err = parse_parallel_with_config("a: 1\n---\nb: [\n", &config).unwrap_err();
        assert!(
            matches!(err, Error::DocumentLimitExceeded { index: 1, .. }),
            "{err:?}"
        );
    }
}

#[test]
fn many_empty_documents_are_rejected_without_materializing_them() {
    let input = "---\n".repeat(2_000_000);
    for config in configs(100) {
        let err = parse_parallel_with_config(&input, &config).unwrap_err();
        assert!(
            matches!(err, Error::DocumentLimitExceeded { index: 100, .. }),
            "{err:?}"
        );
    }
}

#[test]
fn unlimited_by_default() {
    assert_eq!(Config::new().max_documents(), None);
    let input = "---\n".repeat(1_000);
    assert_eq!(
        parse_parallel_with_config(&input, &Config::new())
            .unwrap()
            .len(),
        1_000
    );
}

#[test]
fn limit_error_message_uses_one_based_numbers() {
    let config = Config::new().with_max_documents(limit(1));
    let err = parse_parallel_with_config("a\n---\nb\n", &config).unwrap_err();
    assert_eq!(
        err.to_string(),
        "document 2 exceeds the limit of 1 documents"
    );
}
