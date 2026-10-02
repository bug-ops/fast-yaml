//! `ParseLimits::max_documents`: rejected before any document is parsed.

use fast_yaml_core::limits::{MaxDocuments, ParseLimits};
use fast_yaml_core::{LimitKind, ParseError};
use fast_yaml_parallel::{Config, Error, Workers, parse_parallel, parse_parallel_with_config};

fn limited(max: usize) -> Config {
    Config::new().with_parse_limits(ParseLimits {
        max_documents: MaxDocuments::new(max).unwrap(),
        ..ParseLimits::default()
    })
}

fn configs(max: usize) -> [Config; 2] {
    [
        limited(max),
        limited(max)
            .with_workers(Workers::try_from(2).unwrap())
            .with_sequential_threshold(0),
    ]
}

const fn documents_limit(err: &Error) -> Option<(usize, usize)> {
    match err {
        Error::Parse {
            source:
                ParseError::LimitExceeded {
                    kind: LimitKind::Documents(limit),
                    document,
                    ..
                },
            ..
        } => Some((document.get(), limit.get())),
        _ => None,
    }
}

#[test]
fn stream_at_the_limit_is_accepted() {
    for config in configs(3) {
        let docs = parse_parallel_with_config("a: 1\n---\nb: 2\n---\nc: 3\n", &config).unwrap();
        assert_eq!(docs.len(), 3);
    }
}

#[test]
fn stream_over_the_limit_is_rejected_at_limit_plus_one() {
    for config in configs(2) {
        let err = parse_parallel_with_config("a: 1\n---\nb: 2\n---\nc: 3\n---\nd: 4\n", &config)
            .unwrap_err();
        assert_eq!(documents_limit(&err), Some((2, 2)), "{err:?}");
    }
}

#[test]
fn limit_error_is_reported_before_a_later_syntax_error() {
    for config in configs(1) {
        let err = parse_parallel_with_config("a: 1\n---\nb: [\n", &config).unwrap_err();
        assert!(documents_limit(&err).is_some(), "{err:?}");
    }
}

#[test]
fn many_empty_documents_are_rejected_without_materializing_them() {
    let input = "---\n".repeat(2_000_000);
    for config in configs(100) {
        let err = parse_parallel_with_config(&input, &config).unwrap_err();
        assert_eq!(documents_limit(&err), Some((100, 100)), "{err:?}");
    }
}

#[test]
fn default_limit_applies_without_a_config() {
    assert_eq!(
        Config::new().parse_limits().max_documents,
        MaxDocuments::DEFAULT
    );
    let input = "---\n".repeat(1_000);
    assert_eq!(parse_parallel(&input).unwrap().len(), 1_000);
    let over = "---\n".repeat(MaxDocuments::DEFAULT.get() + 1);
    let err = parse_parallel(&over).unwrap_err();
    assert!(documents_limit(&err).is_some(), "{err:?}");
}

#[test]
fn limit_error_names_the_limit() {
    let err = parse_parallel_with_config("a\n---\nb\n", &limited(1)).unwrap_err();
    assert!(
        err.to_string().contains("document count exceeds 1"),
        "{err}"
    );
}
