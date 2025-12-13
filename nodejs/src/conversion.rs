//! Type conversion between Rust YAML values and JavaScript objects.
//!
//! This module provides bidirectional conversion utilities for translating
//! between yaml-rust2's `Yaml` type and NAPI-RS JavaScript values.

use napi::{Result as NapiResult, bindgen_prelude::*};
use yaml_rust2::Yaml;

/// Convert a YAML value to a JavaScript value.
///
/// Handles all YAML 1.2.2 Core Schema types including special float values.
///
/// # Type Mapping
///
/// - `Yaml::Null` → `null`
/// - `Yaml::Boolean` → `boolean`
/// - `Yaml::Integer` → `number`
/// - `Yaml::Real` → `number` (handles `.inf`, `-.inf`, `.nan`)
/// - `Yaml::String` → `string`
/// - `Yaml::Array` → `Array`
/// - `Yaml::Hash` → `Object`
///
/// # Errors
///
/// Returns an error if conversion fails or encounters invalid YAML values.
pub fn yaml_to_js<'env>(env: &'env Env, yaml: &Yaml) -> NapiResult<Unknown<'env>> {
    match yaml {
        Yaml::Null => Null.into_unknown(env),

        Yaml::Boolean(b) => (*b).into_unknown(env),

        Yaml::Integer(i) => (*i).into_unknown(env),

        Yaml::Real(s) => {
            // YAML 1.2.2 special float values (Section 10.2.1.4)
            let f: f64 = if s.eq_ignore_ascii_case(".inf") || s.eq_ignore_ascii_case("+.inf") {
                f64::INFINITY
            } else if s.eq_ignore_ascii_case("-.inf") {
                f64::NEG_INFINITY
            } else if s.eq_ignore_ascii_case(".nan") {
                f64::NAN
            } else {
                s.parse().map_err(|e| {
                    napi::Error::from_reason(format!("invalid float value '{s}': {e}"))
                })?
            };
            f.into_unknown(env)
        }

        Yaml::String(s) => s.as_str().into_unknown(env),

        Yaml::Array(arr) => {
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

        Yaml::Hash(map) => {
            let mut js_obj = Object::new(env)?;
            for (k, v) in map {
                let key_str = yaml_key_to_string(k)?;
                let js_value = yaml_to_js(env, v)?;
                js_obj.set(&key_str, js_value)?;
            }
            js_obj.into_unknown(env)
        }

        // Aliases are automatically resolved by yaml-rust2
        Yaml::Alias(_) => yaml_to_js(env, &Yaml::Null),

        Yaml::BadValue => Err(napi::Error::from_reason("invalid YAML value encountered")),
    }
}

/// Convert a YAML key to a string for use as JavaScript object property.
///
/// YAML keys can be any type, but JavaScript object keys must be strings.
fn yaml_key_to_string(yaml: &Yaml) -> NapiResult<String> {
    match yaml {
        Yaml::String(s) | Yaml::Real(s) => Ok(s.clone()),
        Yaml::Integer(i) => Ok(i.to_string()),
        Yaml::Boolean(b) => Ok(b.to_string()),
        Yaml::Null => Ok("null".to_string()),
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
/// - `null`, `undefined` → `Yaml::Null`
/// - `boolean` → `Yaml::Boolean`
/// - `number` (integer) → `Yaml::Integer`
/// - `number` (float) → `Yaml::Real`
/// - `string` → `Yaml::String`
/// - `Array` → `Yaml::Array`
/// - `Object` → `Yaml::Hash`
///
/// # Errors
///
/// Returns an error if the JavaScript value contains non-serializable types.
pub fn js_to_yaml(env: &Env, js_value: Unknown) -> NapiResult<Yaml> {
    let js_type = js_value.get_type()?;

    match js_type {
        ValueType::Null | ValueType::Undefined => Ok(Yaml::Null),

        ValueType::Boolean => {
            let b: bool = unsafe { FromNapiValue::from_napi_value(env.raw(), js_value.raw())? };
            Ok(Yaml::Boolean(b))
        }

        ValueType::Number => {
            let num: f64 = unsafe { FromNapiValue::from_napi_value(env.raw(), js_value.raw())? };

            // Handle special float values per YAML 1.2.2 spec
            if num.is_infinite() {
                let s = if num.is_sign_positive() {
                    ".inf".to_string()
                } else {
                    "-.inf".to_string()
                };
                return Ok(Yaml::Real(s));
            }

            if num.is_nan() {
                return Ok(Yaml::Real(".nan".to_string()));
            }

            // Check if it's an integer that can be represented exactly
            if num.fract() == 0.0 && num.is_finite() {
                // Safe integer range for f64 is -(2^53) to 2^53
                const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_992.0; // 2^53
                #[allow(clippy::cast_possible_truncation)]
                if num.abs() <= MAX_SAFE_INTEGER {
                    let i = num as i64;
                    return Ok(Yaml::Integer(i));
                }
            }

            // Float value
            let formatted = format!("{num}");
            let s = if !formatted.contains('.') && !formatted.contains('e') {
                format!("{formatted}.0")
            } else {
                formatted
            };
            Ok(Yaml::Real(s))
        }

        ValueType::String => {
            let s: String = unsafe { FromNapiValue::from_napi_value(env.raw(), js_value.raw())? };
            Ok(Yaml::String(s))
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

                return Ok(Yaml::Array(arr));
            }

            // It's a plain object
            let property_names = js_obj.get_property_names()?;
            let len = property_names.get_array_length()?;

            let mut map = yaml_rust2::yaml::Hash::with_capacity(len as usize);

            for i in 0..len {
                let key: Unknown = property_names.get_element(i)?;
                let key_str: String =
                    unsafe { FromNapiValue::from_napi_value(env.raw(), key.raw())? };

                let value: Unknown = js_obj.get_named_property(&key_str)?;

                map.insert(Yaml::String(key_str), js_to_yaml(env, value)?);
            }

            Ok(Yaml::Hash(map))
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
            yaml_key_to_string(&Yaml::String("test".to_string())).unwrap(),
            "test"
        );
        assert_eq!(yaml_key_to_string(&Yaml::Integer(42)).unwrap(), "42");
        assert_eq!(yaml_key_to_string(&Yaml::Boolean(true)).unwrap(), "true");
        assert_eq!(yaml_key_to_string(&Yaml::Null).unwrap(), "null");
    }
}
