//! Benchmarks for input validation, float parsing and float emission.

use criterion::{Criterion, criterion_group, criterion_main};
use fast_yaml_core::{Emitter, Float, NormalizedInput, Parser, Value};
use std::fmt::Write as _;
use std::hint::black_box;

fn ascii_yaml(lines: usize) -> String {
    let mut out = String::new();
    for i in 0..lines {
        writeln!(out, "key{i}: value number {i} with some padding text").unwrap();
    }
    out
}

fn float_texts(count: usize) -> Vec<String> {
    (0..count)
        .map(|i| {
            if i % 2 == 0 {
                format!("{i}.5")
            } else {
                format!("{i}.250")
            }
        })
        .collect()
}

fn benchmark_input(c: &mut Criterion) {
    let ascii = ascii_yaml(200_000);
    let mixed = ascii.replace("padding", "\u{e9}padding");
    let mut group = c.benchmark_group("normalized_input");
    group.bench_function("ascii", |b| {
        b.iter(|| NormalizedInput::new(black_box(&ascii)).unwrap());
    });
    group.bench_function("non_ascii", |b| {
        b.iter(|| NormalizedInput::new(black_box(&mixed)).unwrap());
    });
    group.finish();
}

fn benchmark_float(c: &mut Criterion) {
    let texts = float_texts(200_000);
    let spelled = Value::Sequence(
        texts
            .iter()
            .map(|t| Value::Float(Float::parse(t).unwrap()))
            .collect(),
    );
    let plain = Value::Sequence(
        (0..200_000)
            .map(|i| Value::Float(Float::new(f64::from(i) + 0.5)))
            .collect(),
    );

    let mut group = c.benchmark_group("float");
    group.bench_function("parse", |b| {
        b.iter(|| {
            for text in &texts {
                black_box(Float::parse(black_box(text)));
            }
        });
    });
    group.bench_function("emit_spelled", |b| {
        b.iter(|| Emitter::emit_str(black_box(&spelled)).unwrap());
    });
    group.bench_function("emit_plain", |b| {
        b.iter(|| Emitter::emit_str(black_box(&plain)).unwrap());
    });
    group.finish();
}

fn benchmark_loader(c: &mut Criterion) {
    let plain = ascii_yaml(50_000);
    let seq = "- item\n".repeat(500_000);
    let mut group = c.benchmark_group("loader");
    group.bench_function("plain_mapping", |b| {
        b.iter(|| Parser::parse_str(black_box(&plain)).unwrap());
    });
    group.bench_function("sequence", |b| {
        b.iter(|| Parser::parse_str(black_box(&seq)).unwrap());
    });
    group.finish();
}

criterion_group!(benches, benchmark_input, benchmark_float, benchmark_loader);
criterion_main!(benches);
