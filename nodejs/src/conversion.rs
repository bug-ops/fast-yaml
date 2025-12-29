//! Type conversion between Rust YAML values and JavaScript objects.
//!
//! This module provides bidirectional conversion utilities for translating
//! between saphyr's `YamlOwned` type and NAPI-RS JavaScript values.

use napi::{Result as NapiResult, bindgen_prelude::*};
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
/// Returns an error if the JavaScript value contains non-serializable types.
pub fn js_to_yaml(env: &Env, js_value: Unknown) -> NapiResult<YamlOwned> {
    let js_type = js_value.get_type()?;

    match js_type {
        ValueType::Null | ValueType::Undefined => Ok(YamlOwned::Value(ScalarOwned::Null)),

        ValueType::Boolean => {
            let b: bool = unsafe { FromNapiValue::from_napi_value(env.raw(), js_value.raw())? };
            Ok(YamlOwned::Value(ScalarOwned::Boolean(b)))
        }

        ValueType::Number => {
            let num: f64 = unsafe { FromNapiValue::from_napi_value(env.raw(), js_value.raw())? };

            // Check if it's an integer that can be represented exactly
            if num.fract() == 0.0 && num.is_finite() {
                // Safe integer range for f64 is -(2^53) to 2^53
                const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_992.0; // 2^53
                #[allow(clippy::cast_possible_truncation)]
                if num.abs() <= MAX_SAFE_INTEGER {
                    let i = num as i64;
                    return Ok(YamlOwned::Value(ScalarOwned::Integer(i)));
                }
            }

            // Float value (including inf, -inf, nan)
            Ok(YamlOwned::Value(ScalarOwned::FloatingPoint(num)))
        }

        ValueType::String => {
            let s: String = unsafe { FromNapiValue::from_napi_value(env.raw(), js_value.raw())? };
            Ok(YamlOwned::Value(ScalarOwned::String(s)))
        }

        ValueType::Object => {
            let js_obj: Object =
                unsafe { FromNapiValue::from_napi_value(env.raw(), js_value.raw())? };

            // Check if it's an array
            if js_obj.is_array()? {
                let len: u32 = js_obj.get_array_length()?;
                let mut arr = Vec::with_capacity(len as usize);

                for i in 0..len {
                    let elem: Unknown = js_obj.get_element(i)?;
                    arr.push(js_to_yaml(env, elem)?);
                }

                return Ok(YamlOwned::Sequence(arr));
            }

            // It's a plain object
            let property_names = js_obj.get_property_names()?;
            let len = property_names.get_array_length()?;

            let mut map = MappingOwned::with_capacity(len as usize);

            for i in 0..len {
                let key: Unknown = property_names.get_element(i)?;
                let key_str: String =
                    unsafe { FromNapiValue::from_napi_value(env.raw(), key.raw())? };

                let value: Unknown = js_obj.get_named_property(&key_str)?;

                map.insert(
                    YamlOwned::Value(ScalarOwned::String(key_str)),
                    js_to_yaml(env, value)?,
                );
            }

            Ok(YamlOwned::Mapping(map))
        }

        _ => Err(napi::Error::from_reason(format!(
            "cannot serialize JavaScript value of type {js_type:?} to YAML"
        ))),
    }
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
