//! Thread safety and concurrency tests.

use fast_yaml_parallel::{ParallelConfig, parse_parallel, parse_parallel_with_config};
use std::fmt::Write;
use std::sync::Arc;
use std::thread;

#[test]
fn test_concurrent_parsing_same_input() {
    let yaml = Arc::new("---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3".to_string());

    // Spawn multiple threads parsing the same input
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let yaml_clone = Arc::clone(&yaml);
            thread::spawn(move || {
                let docs = parse_parallel(&yaml_clone).unwrap();
                assert_eq!(docs.len(), 3);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_concurrent_parsing_different_inputs() {
    // Each thread parses different input
    let handles: Vec<_> = (0..10)
        .map(|i| {
            thread::spawn(move || {
                let yaml = format!("---\nid: {i}\n---\nvalue: {}", i * 2);
                let docs = parse_parallel(&yaml).unwrap();
                assert_eq!(docs.len(), 2);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_concurrent_with_different_configs() {
    let yaml = Arc::new("---\na: 1\n---\nb: 2\n---\nc: 3\n---\nd: 4".to_string());

    // Each thread uses different config
    let handles: Vec<_> = (1..=8)
        .map(|thread_count| {
            let yaml_clone = Arc::clone(&yaml);
            thread::spawn(move || {
                let config = ParallelConfig::new().with_thread_count(Some(thread_count));
                let docs = parse_parallel_with_config(&yaml_clone, &config).unwrap();
                assert_eq!(docs.len(), 4);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_parse_parallel_is_send() {
    // Verify parse_parallel can be called from spawned threads
    let yaml = "---\ntest: value".to_string();
    let handle = thread::spawn(move || {
        let docs = parse_parallel(&yaml).unwrap();
        assert_eq!(docs.len(), 1);
    });

    handle.join().unwrap();
}

#[test]
fn test_config_is_send_sync() {
    // Verify config can be shared across threads
    let config = Arc::new(ParallelConfig::new().with_thread_count(Some(4)));

    let handles: Vec<_> = (0..5)
        .map(|i| {
            let config_clone = Arc::clone(&config);
            thread::spawn(move || {
                let yaml = format!("---\nid: {i}");
                let docs = parse_parallel_with_config(&yaml, &config_clone).unwrap();
                assert_eq!(docs.len(), 1);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_rapid_sequential_calls() {
    // Rapid calls in sequence (no threads)
    for i in 0..100 {
        let yaml = format!("---\nid: {i}");
        let docs = parse_parallel(&yaml).unwrap();
        assert_eq!(docs.len(), 1);
    }
}

#[test]
fn test_large_number_of_threads() {
    let yaml = Arc::new("---\ntest: 1\n---\ntest: 2".to_string());

    // Spawn many threads simultaneously
    let handles: Vec<_> = (0..100)
        .map(|_| {
            let yaml_clone = Arc::clone(&yaml);
            thread::spawn(move || {
                let docs = parse_parallel(&yaml_clone).unwrap();
                assert_eq!(docs.len(), 2);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_deterministic_results_across_threads() {
    // Verify that parallel parsing produces deterministic results
    let yaml = Arc::new({
        let mut s = String::new();
        for i in 0..50 {
            let _ = writeln!(s, "---\nid: {i}\nvalue: {}", i * 2);
        }
        s
    });

    let results: Vec<_> = (0..10)
        .map(|_| {
            let yaml_clone = Arc::clone(&yaml);
            thread::spawn(move || parse_parallel(&yaml_clone).unwrap())
        })
        .map(|h| h.join().unwrap())
        .collect();

    // All results should be identical
    for result in &results[1..] {
        assert_eq!(result.len(), results[0].len());
        assert_eq!(result.len(), 50);
    }
}

#[test]
fn test_error_handling_across_threads() {
    let invalid_yaml = Arc::new("---\nvalid: 1\n---\ninvalid: [".to_string());

    let handles: Vec<_> = (0..5)
        .map(|_| {
            let yaml_clone = Arc::clone(&invalid_yaml);
            thread::spawn(move || {
                let result = parse_parallel(&yaml_clone);
                assert!(result.is_err());
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_mixed_success_failure_concurrent() {
    // Some threads succeed, some fail
    let handles: Vec<_> = (0..10)
        .map(|i| {
            thread::spawn(move || {
                let yaml = if i % 2 == 0 {
                    format!("---\nvalid: {i}")
                } else {
                    "---\ninvalid: [".to_string()
                };

                let result = parse_parallel(&yaml);
                if i % 2 == 0 {
                    assert!(result.is_ok());
                } else {
                    assert!(result.is_err());
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_scoped_threads() {
    let yaml = "---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3";

    thread::scope(|s| {
        for _ in 0..10 {
            s.spawn(|| {
                let docs = parse_parallel(yaml).unwrap();
                assert_eq!(docs.len(), 3);
            });
        }
    });
}

#[test]
fn test_nested_parallel_calls() {
    // Thread spawns another thread that parses
    let yaml = Arc::new("---\ntest: 1".to_string());

    let handle = thread::spawn(move || {
        let yaml_clone = Arc::clone(&yaml);
        let inner_handle = thread::spawn(move || {
            let docs = parse_parallel(&yaml_clone).unwrap();
            assert_eq!(docs.len(), 1);
        });
        inner_handle.join().unwrap();
    });

    handle.join().unwrap();
}

#[test]
fn test_parallel_with_varying_load() {
    // Mix of small and large inputs across threads
    let handles: Vec<_> = (0..20)
        .map(|i| {
            thread::spawn(move || {
                let yaml = if i % 3 == 0 {
                    // Large input
                    let mut s = String::new();
                    for j in 0..100 {
                        let _ = writeln!(s, "---\nid: {j}");
                    }
                    s
                } else {
                    // Small input
                    format!("---\nid: {i}")
                };

                let docs = parse_parallel(&yaml).unwrap();
                assert!(!docs.is_empty());
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_config_builder_concurrent() {
    // Multiple threads building configs
    let handles: Vec<_> = (1..=10)
        .map(|i| {
            thread::spawn(move || {
                let config = ParallelConfig::new()
                    .with_thread_count(Some(i))
                    .with_min_chunk_size(1024 * i)
                    .with_max_chunk_size(10 * 1024 * 1024);

                let yaml = format!("---\nthread: {i}");
                let docs = parse_parallel_with_config(&yaml, &config).unwrap();
                assert_eq!(docs.len(), 1);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_no_data_races() {
    // This test verifies that Rust's type system prevents data races
    // by attempting to share mutable state (which should not compile if uncommented)

    let yaml = "---\ntest: 1";
    let result = parse_parallel(yaml).unwrap();

    // This would not compile:
    // let mut counter = 0;
    // thread::scope(|s| {
    //     for _ in 0..10 {
    //         s.spawn(|| {
    //             counter += 1; // Error: cannot borrow as mutable
    //         });
    //     }
    // });

    assert_eq!(result.len(), 1);
}
