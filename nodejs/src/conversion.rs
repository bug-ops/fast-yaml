//! Type conversion between Rust YAML values and JavaScript objects.
//!
//! This module provides bidirectional conversion utilities for translating
//! between `fast_yaml_core::Value` and NAPI-RS JavaScript values.

use fast_yaml_core::value::quote_key;
use fast_yaml_core::{DumpBudget, Float, LimitKind, Mapping, MaxDepth, Value};
use napi::{Result as NapiResult, bindgen_prelude::*};

/// Sets `key` on `object`; `__proto__` is defined as an own property so it cannot replace the prototype.
fn set_own(env: Env, object: &mut Object, key: &str, value: Unknown) -> NapiResult<()> {
    if key == "__proto__" {
        return object
            .define_properties(&[Property::new().with_name(&env, key)?.with_value(&value)]);
    }
    object.set(key, value)
}

/// Convert a YAML value to a JavaScript value.
///
/// Handles all YAML 1.2.2 Core Schema types including special float values.
///
/// # Type Mapping
///
/// - `Value::Null` → `null`
/// - `Value::Bool` → `boolean`
/// - `Value::Int` → `number`
/// - `Value::Float` → `number`
/// - `Value::String` → `string`
/// - `Value::Sequence` → `Array`
/// - `Value::Mapping` → `Object`
///
/// # Errors
///
/// Returns an error if conversion fails or encounters invalid YAML values.
pub fn yaml_to_js<'env>(env: &'env Env, yaml: &Value) -> NapiResult<Unknown<'env>> {
    match yaml {
        Value::Null => Null.into_unknown(env),
        Value::Bool(b) => (*b).into_unknown(env),
        Value::Int(i) => (*i).into_unknown(env),
        Value::BigInt(big) => big.canonical().into_unknown(env),
        Value::Float(f) => f.get().into_unknown(env),
        Value::String(s) => s.as_str().into_unknown(env),

        Value::Sequence(arr) => {
            let arr_len = u32::try_from(arr.len()).map_err(|_| {
                napi::Error::from_reason("array too large for JavaScript (max 2^32 elements)")
            })?;

            // Pre-allocate JavaScript array with known capacity to avoid reallocation.
            // NAPI-RS env.create_array(len) hints to V8 the final array size, enabling
            // efficient memory allocation and reducing overhead for large arrays.
            let mut js_array = env.create_array(arr_len)?;
            for (i, item) in arr.iter().enumerate() {
                let js_value = yaml_to_js(env, item)?;
                let idx = u32::try_from(i)
                    .map_err(|_| napi::Error::from_reason("array index too large"))?;
                js_array.set(idx, js_value)?;
            }
            js_array.into_unknown(env)
        }

        Value::Set(set) => {
            let mut js_obj = Object::new(env)?;
            let mut seen = std::collections::HashSet::with_capacity(set.len());
            for member in set {
                let key_str = yaml_key_to_string(member)?;
                if !seen.insert(key_str.clone()) {
                    return Err(napi::Error::from_reason(format!(
                        "distinct YAML keys convert to the same JavaScript property {}",
                        quote_key(&key_str)
                    )));
                }
                set_own(*env, &mut js_obj, &key_str, Null.into_unknown(env)?)?;
            }
            js_obj.into_unknown(env)
        }

        Value::Mapping(map) => {
            let mut js_obj = Object::new(env)?;
            let mut seen = std::collections::HashSet::with_capacity(map.len());
            for (k, v) in map {
                let key_str = yaml_key_to_string(k)?;
                if !seen.insert(key_str.clone()) {
                    return Err(napi::Error::from_reason(format!(
                        "distinct YAML keys convert to the same JavaScript property {}",
                        quote_key(&key_str)
                    )));
                }
                let js_value = yaml_to_js(env, v)?;
                set_own(*env, &mut js_obj, &key_str, js_value)?;
            }
            js_obj.into_unknown(env)
        }
    }
}

/// Convert a YAML key to a string for use as JavaScript object property.
///
/// YAML keys can be any type, but JavaScript object keys must be strings.
fn yaml_key_to_string(yaml: &Value) -> NapiResult<String> {
    yaml.key_text()
        .map(std::borrow::Cow::into_owned)
        .ok_or_else(|| {
            napi::Error::from_reason(
                "YAML complex keys (sequences or mappings as keys) are not supported as JavaScript object keys",
            )
        })
}

/// Convert a JavaScript value to a YAML value.
///
/// Handles JavaScript types including special float values (Infinity, -Infinity, NaN).
///
/// # Type Mapping
///
/// - `null`, `undefined` → `Value::Null`
/// - `boolean` → `Value::Bool`
/// - `number` (integer) → `Value::Int`
/// - `number` (float) → `Value::Float`
/// - `string` → `Value::String`
/// - `Array` → `Value::Sequence`
/// - `Object` → `Value::Mapping`
///
/// # Errors
///
/// Returns an error if the JavaScript value contains non-serializable types or converting it
/// would exceed `budget`.
pub fn js_to_yaml(js_value: Unknown, budget: &mut DumpBudget) -> NapiResult<Value> {
    let mut stack: Vec<OpenContainer> = Vec::new();
    match classify(js_value, 0, budget)? {
        Classified::Scalar(value) => return Ok(value),
        Classified::Container(open) => stack.push(open),
    }
    // The walk is iterative (explicit heap stack), so host stack size does not bound
    // nesting; `MaxDepth::DEFAULT` does, which also catches self-referential values.
    loop {
        let depth = stack.len();
        let Some(top) = stack.last_mut() else {
            return Err(napi::Error::from_reason(
                "internal error: empty conversion stack",
            ));
        };
        if let Some(child) = top.next_child() {
            match classify(child.value, depth, budget)? {
                Classified::Scalar(value) => top.accept(child.key, value),
                Classified::Container(open) => {
                    top.pending_key = child.key;
                    stack.push(open);
                }
            }
        } else if let Some(finished) = stack.pop() {
            let value = finished.finish();
            match stack.last_mut() {
                Some(parent) => {
                    let key = parent.pending_key.take();
                    parent.accept(key, value);
                }
                None => return Ok(value),
            }
        }
    }
}

/// A child value plus, for object members, its property name.
struct Child<'a> {
    key: Option<String>,
    value: Unknown<'a>,
}

/// A container whose children are still being converted.
struct OpenContainer<'a> {
    children: std::vec::IntoIter<Child<'a>>,
    /// Property name of the child container currently being converted.
    pending_key: Option<String>,
    done: Done,
}

enum Done {
    Sequence(Vec<Value>),
    Mapping(Mapping),
}

impl<'a> OpenContainer<'a> {
    fn next_child(&mut self) -> Option<Child<'a>> {
        self.children.next()
    }

    fn accept(&mut self, key: Option<String>, value: Value) {
        match (&mut self.done, key) {
            (Done::Mapping(map), Some(key)) => {
                map.insert(Value::String(key), value);
            }
            (Done::Sequence(items), _) => items.push(value),
            (Done::Mapping(_), None) => {}
        }
    }

    fn finish(self) -> Value {
        match self.done {
            Done::Sequence(items) => Value::Sequence(items),
            Done::Mapping(map) => Value::Mapping(map),
        }
    }
}

enum Classified<'a> {
    Scalar(Value),
    Container(OpenContainer<'a>),
}

/// Maps a dump limit violation to a JS error; depth overruns hint at self-reference.
fn limit_error(kind: LimitKind) -> napi::Error {
    napi::Error::from_reason(match kind {
        LimitKind::Depth(_) => format!("cannot serialize to YAML: {kind} (circular reference?)"),
        _ => format!("cannot serialize to YAML: {kind}"),
    })
}

/// Enter one more container level, failing past [`MaxDepth::DEFAULT`].
///
/// The bound also catches self-referential containers, which would recurse forever.
fn enter_container(depth: usize) -> NapiResult<usize> {
    MaxDepth::DEFAULT.descend(depth).map_err(limit_error)
}

/// Convert a scalar, or open a container whose children are `depth` levels deep.
///
/// A node's fixed cost is charged by its parent, before any storage for the children is
/// allocated; the root is free because its output is covered by its children and text.
fn classify<'a>(
    js_value: Unknown<'a>,
    depth: usize,
    budget: &mut DumpBudget,
) -> NapiResult<Classified<'a>> {
    let js_type = js_value.get_type()?;

    match js_type {
        ValueType::Null | ValueType::Undefined => Ok(Classified::Scalar(Value::Null)),

        ValueType::Boolean => {
            let b: bool = FromNapiValue::from_unknown(js_value)?;
            Ok(Classified::Scalar(Value::Bool(b)))
        }

        ValueType::Number => {
            let num: f64 = FromNapiValue::from_unknown(js_value)?;
            Ok(Classified::Scalar(number_to_scalar(num)))
        }

        ValueType::String => {
            let s: String = FromNapiValue::from_unknown(js_value)?;
            budget.charge(s.len()).map_err(limit_error)?;
            Ok(Classified::Scalar(Value::String(s)))
        }

        ValueType::Object => {
            let js_obj: Object = FromNapiValue::from_unknown(js_value)?;
            enter_container(depth)?;

            if js_obj.is_array()? {
                let len: u32 = js_obj.get_array_length()?;
                budget.charge_nodes(len as usize).map_err(limit_error)?;
                let mut children = Vec::with_capacity(len as usize);
                for i in 0..len {
                    children.push(Child {
                        key: None,
                        value: js_obj.get_element(i)?,
                    });
                }
                return Ok(Classified::Container(OpenContainer {
                    children: children.into_iter(),
                    pending_key: None,
                    done: Done::Sequence(Vec::with_capacity(len as usize)),
                }));
            }

            let property_names = js_obj.get_property_names()?;
            let len = property_names.get_array_length()?;
            // Each member costs a key node and a value node.
            budget
                .charge_nodes((len as usize).saturating_mul(2))
                .map_err(limit_error)?;
            let mut children = Vec::with_capacity(len as usize);
            for i in 0..len {
                let key: Unknown = property_names.get_element(i)?;
                let key_str: String = FromNapiValue::from_unknown(key)?;
                budget.charge(key_str.len()).map_err(limit_error)?;
                let value: Unknown = js_obj.get_named_property(&key_str)?;
                children.push(Child {
                    key: Some(key_str),
                    value,
                });
            }
            Ok(Classified::Container(OpenContainer {
                children: children.into_iter(),
                pending_key: None,
                done: Done::Mapping(Mapping::with_capacity(len as usize)),
            }))
        }

        _ => Err(napi::Error::from_reason(format!(
            "cannot serialize JavaScript value of type {js_type:?} to YAML"
        ))),
    }
}

/// Integral numbers within the exact `f64` range become integers; the rest stay floats.
fn number_to_scalar(num: f64) -> Value {
    // Safe integer range for f64 is -(2^53) to 2^53
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_992.0; // 2^53
    #[allow(clippy::cast_possible_truncation)]
    if num.fract() == 0.0 && num.is_finite() && num.abs() <= MAX_SAFE_INTEGER {
        return Value::Int(num as i64);
    }
    // Float value (including inf, -inf, nan)
    Value::Float(Float::new(num))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_key_to_string() {
        assert_eq!(
            yaml_key_to_string(&Value::String("test".to_string())).unwrap(),
            "test"
        );
        assert_eq!(yaml_key_to_string(&Value::Int(42)).unwrap(), "42");
        assert_eq!(yaml_key_to_string(&Value::Bool(true)).unwrap(), "true");
        assert_eq!(yaml_key_to_string(&Value::Null).unwrap(), "null");
    }
}
