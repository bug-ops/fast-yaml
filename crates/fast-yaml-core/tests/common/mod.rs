//! Helpers shared by the integration tests.

#![allow(dead_code, reason = "each test binary uses a subset")]

use fast_yaml_core::Value;

/// Compares two values including the entry order of every mapping and set, which `==` ignores.
pub fn same_order(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Sequence(a), Value::Sequence(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_order(a, b))
        }
        (Value::Mapping(a), Value::Mapping(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|((ka, va), (kb, vb))| same_order(ka, kb) && same_order(va, vb))
        }
        (Value::Set(a), Value::Set(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| same_order(a, b))
        }
        (a, b) => a == b,
    }
}

/// [`same_order`] over two lists of documents.
pub fn same_order_all(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_order(a, b))
}
