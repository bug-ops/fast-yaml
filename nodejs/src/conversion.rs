//! Type conversion between Rust YAML values and JavaScript objects.
//!
//! This module provides bidirectional conversion utilities for translating
//! between saphyr's `YamlOwned` type and NAPI-RS JavaScript values.

use fast_yaml_core::{DumpBudget, LimitKind, MaxDepth};
use napi::{Result as NapiResult, bindgen_prelude::*};
use ordered_float::OrderedFloat;
use saphyr::{MappingOwned, ScalarOwned, YamlOwned};

/// Convert a YAML value to a JavaScript value.
///
/// Handles all YAML 1.2.2 Core Schema types including special float values.
///
/// # Type Mapping
///
/// - `YamlOwned::Value(ScalarOwned::Null)` → `null`
/// - `YamlOwned::Value(ScalarOwned::Boolean)` → `boolean`
/// - `YamlOwned::Value(ScalarOwned::Integer)` → `number`
/// - `YamlOwned::Value(ScalarOwned::FloatingPoint)` → `number`
/// - `YamlOwned::Value(ScalarOwned::String)` → `string`
/// - `YamlOwned::Sequence` → `Array`
/// - `YamlOwned::Mapping` → `Object`
///
/// # Errors
///
/// Returns an error if conversion fails or encounters invalid YAML values.
pub fn yaml_to_js<'env>(env: &'env Env, yaml: &YamlOwned) -> NapiResult<Unknown<'env>> {
    match yaml {
        YamlOwned::Value(scalar) => match scalar {
            ScalarOwned::Null => Null.into_unknown(env),
            ScalarOwned::Boolean(b) => (*b).into_unknown(env),
            ScalarOwned::Integer(i) => (*i).into_unknown(env),
            ScalarOwned::FloatingPoint(f) => (*f).into_unknown(env),
            ScalarOwned::String(s) => s.as_str().into_unknown(env),
        },

        YamlOwned::Sequence(arr) => {
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

        YamlOwned::Mapping(map) => {
            let mut js_obj = Object::new(env)?;
            for (k, v) in map {
                let key_str = yaml_key_to_string(k)?;
                let js_value = yaml_to_js(env, v)?;
                js_obj.set(&key_str, js_value)?;
            }
            js_obj.into_unknown(env)
        }

        // Aliases are automatically resolved by saphyr
        YamlOwned::Alias(_) => Null.into_unknown(env),

        YamlOwned::BadValue => Err(napi::Error::from_reason("invalid YAML value encountered")),

        // Tagged values - extract the inner value
        YamlOwned::Tagged(_, inner) => yaml_to_js(env, inner),

        // Representation values - the first element is the raw string representation
        YamlOwned::Representation(repr, _, _) => repr.as_str().into_unknown(env),
    }
}

/// Convert a YAML key to a string for use as JavaScript object property.
///
/// YAML keys can be any type, but JavaScript object keys must be strings.
fn yaml_key_to_string(yaml: &YamlOwned) -> NapiResult<String> {
    match yaml {
        YamlOwned::Value(scalar) => match scalar {
            ScalarOwned::String(s) => Ok(s.clone()),
            ScalarOwned::Integer(i) => Ok(i.to_string()),
            ScalarOwned::FloatingPoint(f) => Ok(f.to_string()),
            ScalarOwned::Boolean(b) => Ok(b.to_string()),
            ScalarOwned::Null => Ok("null".to_string()),
        },
        _ => Err(napi::Error::from_reason(format!(
            "unsupported YAML key type: {yaml:?}"
        ))),
    }
}

/// Convert a JavaScript value to a YAML value.
///
/// Handles JavaScript types including special float values (Infinity, -Infinity, NaN).
///
/// # Type Mapping
///
/// - `null`, `undefined` → `YamlOwned::Value(ScalarOwned::Null)`
/// - `boolean` → `YamlOwned::Value(ScalarOwned::Boolean)`
/// - `number` (integer) → `YamlOwned::Value(ScalarOwned::Integer)`
/// - `number` (float) → `YamlOwned::Value(ScalarOwned::FloatingPoint)`
/// - `string` → `YamlOwned::Value(ScalarOwned::String)`
/// - `Array` → `YamlOwned::Sequence`
/// - `Object` → `YamlOwned::Mapping`
///
/// # Errors
///
/// Returns an error if the JavaScript value contains non-serializable types or converting it
/// would exceed `budget`.
pub fn js_to_yaml(env: Env, js_value: Unknown, budget: &mut DumpBudget) -> NapiResult<YamlOwned> {
    let mut stack: Vec<OpenContainer> = Vec::new();
    match classify(&env, js_value, 0, budget)? {
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
            match classify(&env, child.value, depth, budget)? {
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
    Sequence(Vec<YamlOwned>),
    Mapping(MappingOwned),
}

impl<'a> OpenContainer<'a> {
    fn next_child(&mut self) -> Option<Child<'a>> {
        self.children.next()
    }

    fn accept(&mut self, key: Option<String>, value: YamlOwned) {
        match (&mut self.done, key) {
            (Done::Mapping(map), Some(key)) => {
                map.insert(YamlOwned::Value(ScalarOwned::String(key)), value);
            }
            (Done::Sequence(items), _) => items.push(value),
            (Done::Mapping(_), None) => {}
        }
    }

    fn finish(self) -> YamlOwned {
        match self.done {
            Done::Sequence(items) => YamlOwned::Sequence(items),
            Done::Mapping(map) => YamlOwned::Mapping(map),
        }
    }
}

enum Classified<'a> {
    Scalar(YamlOwned),
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
    env: &Env,
    js_value: Unknown<'a>,
    depth: usize,
    budget: &mut DumpBudget,
) -> NapiResult<Classified<'a>> {
    let js_type = js_value.get_type()?;

    match js_type {
        ValueType::Null | ValueType::Undefined => {
            Ok(Classified::Scalar(YamlOwned::Value(ScalarOwned::Null)))
        }

        ValueType::Boolean => {
            let b: bool = unsafe { FromNapiValue::from_napi_value(env.raw(), js_value.raw())? };
            Ok(Classified::Scalar(YamlOwned::Value(ScalarOwned::Boolean(
                b,
            ))))
        }

        ValueType::Number => {
            let num: f64 = unsafe { FromNapiValue::from_napi_value(env.raw(), js_value.raw())? };
            Ok(Classified::Scalar(YamlOwned::Value(number_to_scalar(num))))
        }

        ValueType::String => {
            let s: String = unsafe { FromNapiValue::from_napi_value(env.raw(), js_value.raw())? };
            budget.charge(s.len()).map_err(limit_error)?;
            Ok(Classified::Scalar(YamlOwned::Value(ScalarOwned::String(s))))
        }

        ValueType::Object => {
            let js_obj: Object =
                unsafe { FromNapiValue::from_napi_value(env.raw(), js_value.raw())? };
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
                let key_str: String =
                    unsafe { FromNapiValue::from_napi_value(env.raw(), key.raw())? };
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
                done: Done::Mapping(MappingOwned::with_capacity(len as usize)),
            }))
        }

        _ => Err(napi::Error::from_reason(format!(
            "cannot serialize JavaScript value of type {js_type:?} to YAML"
        ))),
    }
}

/// Integral numbers within the exact `f64` range become integers; the rest stay floats.
fn number_to_scalar(num: f64) -> ScalarOwned {
    // Safe integer range for f64 is -(2^53) to 2^53
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_992.0; // 2^53
    #[allow(clippy::cast_possible_truncation)]
    if num.fract() == 0.0 && num.is_finite() && num.abs() <= MAX_SAFE_INTEGER {
        return ScalarOwned::Integer(num as i64);
    }
    // Float value (including inf, -inf, nan)
    ScalarOwned::FloatingPoint(OrderedFloat(num))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_key_to_string() {
        assert_eq!(
            yaml_key_to_string(&YamlOwned::Value(ScalarOwned::String("test".to_string()))).unwrap(),
            "test"
        );
        assert_eq!(
            yaml_key_to_string(&YamlOwned::Value(ScalarOwned::Integer(42))).unwrap(),
            "42"
        );
        assert_eq!(
            yaml_key_to_string(&YamlOwned::Value(ScalarOwned::Boolean(true))).unwrap(),
            "true"
        );
        assert_eq!(
            yaml_key_to_string(&YamlOwned::Value(ScalarOwned::Null)).unwrap(),
            "null"
        );
    }
}
