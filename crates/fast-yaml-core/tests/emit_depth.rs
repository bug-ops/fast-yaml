//! Emission keeps its nesting on the heap: the deepest accepted value fits a tiny stack (#546).

use fast_yaml_core::{Emitter, EmitterConfig, MaxDepth, ParseLimits, Parser, Value};

const SMALL_STACK: usize = 256 * 1024;

fn nested_sequences(depth: usize) -> Value {
    let mut doc = Value::Int(1);
    for _ in 0..depth {
        doc = Value::Sequence(vec![doc]);
    }
    doc
}

fn nested_mappings(depth: usize) -> Value {
    let mut doc = Value::Int(1);
    for _ in 0..depth {
        let mut map = fast_yaml_core::Mapping::new();
        map.insert(Value::String("k".into()), doc);
        doc = Value::Mapping(map);
    }
    doc
}

fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn deepest_value_emits_in_block_and_flow_on_a_small_stack() {
    on_small_stack(|| {
        for doc in [nested_sequences(512), nested_mappings(512)] {
            for flow in [None, Some(true)] {
                let config = EmitterConfig::new().with_default_flow_style(flow);
                let out = Emitter::emit_str_with_config(&doc, &config).unwrap();
                assert!(out.contains('1'), "{flow:?}");
            }
            // dropping a 512-deep value recurses; leak it so the test is about emission
            std::mem::forget(doc);
        }
    });
}

#[test]
fn parse_at_the_depth_ceiling_then_emit_with_the_default_config() {
    let yaml = format!("{}x\n", "- ".repeat(512));
    let limits = ParseLimits {
        max_depth: MaxDepth::MAX,
        ..ParseLimits::default()
    };
    let doc = Parser::parse_str_with_limits(&yaml, &limits)
        .unwrap()
        .unwrap();
    let out = Emitter::emit_str(&doc).unwrap();
    let back = Parser::parse_str_with_limits(&out, &limits)
        .unwrap()
        .unwrap();
    assert_eq!(back, doc);
    std::mem::forget(doc);
    std::mem::forget(back);
}
