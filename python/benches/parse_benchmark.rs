//! Benchmarks for YAML parsing performance.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::fmt::Write;
use std::hint::black_box;
use yaml_rust2::YamlLoader;

const SMALL_YAML: &str = r"
name: test
value: 123
active: true
";

const MEDIUM_YAML: &str = r"
database:
  host: localhost
  port: 5432
  name: myapp
  credentials:
    username: admin
    password: secret

servers:
  - name: web1
    ip: 192.168.1.1
    roles:
      - web
      - api
  - name: web2
    ip: 192.168.1.2
    roles:
      - web
  - name: db1
    ip: 192.168.1.3
    roles:
      - database
      - backup

settings:
  debug: false
  log_level: info
  features:
    - authentication
    - caching
    - monitoring
";

#[allow(clippy::cast_precision_loss)]
fn generate_large_yaml(num_items: usize) -> String {
    let mut yaml = String::from("items:\n");
    for i in 0..num_items {
        let _ = write!(
            yaml,
            r#"  - id: {i}
    name: item_{i}
    description: This is item number {i} with a longer description
    active: {}
    score: {}
    tags:
      - tag_0
      - tag_1
      - tag_2
      - tag_3
      - tag_4
    metadata:
      created: "2024-01-01"
      updated: "2024-12-01"
      version: {}
"#,
            i % 2 == 0,
            i as f64 * 1.5,
            i % 10
        );
    }
    yaml
}

fn bench_parse_small(c: &mut Criterion) {
    c.bench_function("parse_small", |b| {
        b.iter(|| {
            let docs = YamlLoader::load_from_str(black_box(SMALL_YAML)).unwrap();
            black_box(docs);
        });
    });
}

fn bench_parse_medium(c: &mut Criterion) {
    c.bench_function("parse_medium", |b| {
        b.iter(|| {
            let docs = YamlLoader::load_from_str(black_box(MEDIUM_YAML)).unwrap();
            black_box(docs);
        });
    });
}

fn bench_parse_large(c: &mut Criterion) {
    let large_yaml = generate_large_yaml(1000);

    c.bench_function("parse_large_1000", |b| {
        b.iter(|| {
            let docs = YamlLoader::load_from_str(black_box(&large_yaml)).unwrap();
            black_box(docs);
        });
    });
}

fn bench_parse_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_scaling");

    for size in [10, 50, 100, 500, 1000] {
        let yaml = generate_large_yaml(size);

        group.bench_with_input(BenchmarkId::from_parameter(size), &yaml, |b, yaml| {
            b.iter(|| {
                let docs = YamlLoader::load_from_str(black_box(yaml)).unwrap();
                black_box(docs);
            });
        });
    }

    group.finish();
}

fn bench_parse_multi_document(c: &mut Criterion) {
    let multi_doc = format!("---\n{SMALL_YAML}\n---\n{MEDIUM_YAML}\n---\n{SMALL_YAML}\n");

    c.bench_function("parse_multi_document", |b| {
        b.iter(|| {
            let docs = YamlLoader::load_from_str(black_box(&multi_doc)).unwrap();
            black_box(docs);
        });
    });
}

fn bench_parse_special_values(c: &mut Criterion) {
    let special = r"
null_values:
  - ~
  - null
  - Null
  - NULL

booleans:
  - true
  - false
  - True
  - False

floats:
  - .inf
  - -.inf
  - .nan
  - 1.23e10

integers:
  - 0
  - 42
  - -42
  - 0xFF
  - 0o77
";

    c.bench_function("parse_special_values", |b| {
        b.iter(|| {
            let docs = YamlLoader::load_from_str(black_box(special)).unwrap();
            black_box(docs);
        });
    });
}

fn bench_parse_anchors(c: &mut Criterion) {
    let anchors = r"
defaults: &defaults
  adapter: postgres
  host: localhost
  port: 5432

development:
  <<: *defaults
  database: dev_db

production:
  <<: *defaults
  database: prod_db

staging:
  <<: *defaults
  database: staging_db
";

    c.bench_function("parse_anchors", |b| {
        b.iter(|| {
            let docs = YamlLoader::load_from_str(black_box(anchors)).unwrap();
            black_box(docs);
        });
    });
}

criterion_group!(
    benches,
    bench_parse_small,
    bench_parse_medium,
    bench_parse_large,
    bench_parse_scaling,
    bench_parse_multi_document,
    bench_parse_special_values,
    bench_parse_anchors
);

criterion_main!(benches);
