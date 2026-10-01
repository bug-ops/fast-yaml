//! YAML serialization functions for Node.js.
//!
//! This module provides safe YAML serialization functions that convert
//! JavaScript objects to YAML strings.

use crate::conversion::js_to_yaml;
use crate::options::{emitter_indent, emitter_width};
use fast_yaml_core::{DumpBudget, Mapping, MaxOutputBytes, Value};
use napi::{Env, Result as NapiResult, bindgen_prelude::*};
use napi_derive::napi;

/// Raises `result`'s error as a JS exception, returning a placeholder JS never observes.
///
/// An `Err` returned from a `#[napi]` function reaches JS as a returned value instead of a
/// thrown exception, so every fallible dump path routes through here.
fn throw_or_default<T: Default>(env: Env, result: napi::Result<T>) -> napi::Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(e) => {
            env.throw_error(&e.reason, Some(e.status.as_ref()))?;
            Ok(T::default())
        }
    }
}

/// Fails when `output` is larger than [`MaxOutputBytes::DEFAULT`].
fn check_output_size(output: String) -> napi::Result<String> {
    MaxOutputBytes::DEFAULT
        .check(output.len())
        .map_err(|kind| napi::Error::from_reason(kind.to_string()))?;
    Ok(output)
}

fn emitter_config(opts: &DumpOptions) -> NapiResult<fast_yaml_core::EmitterConfig> {
    Ok(fast_yaml_core::EmitterConfig::new()
        .with_indent(emitter_indent(opts.indent)?)
        .with_width(emitter_width(opts.width)?)
        .with_default_flow_style(opts.default_flow_style)
        .with_explicit_start(opts.explicit_start.unwrap_or(false)))
}

fn emit_error(e: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(format!("YAML emit error: {e}"))
}

/// Options for YAML serialization.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DumpOptions {
    /// If true, sort object keys alphabetically (default: false)
    pub sort_keys: Option<bool>,

    /// Allow unicode characters (default: true).
    /// Note: yaml-rust2 always outputs unicode; this is accepted for API compatibility.
    pub allow_unicode: Option<bool>,

    /// Indentation width in spaces (default: 2).
    /// Must be an integer in 1-9; other values throw.
    pub indent: Option<f64>,

    /// Maximum line width for wrapping (default: 80).
    /// Must be an integer in 20-1000; other values throw.
    pub width: Option<f64>,

    /// Default flow style for collections (default: null).
    /// - null: Use block style (multi-line)
    /// - true: Force flow style (inline: [...], {...})
    /// - false: Force block style (explicit)
    pub default_flow_style: Option<bool>,

    /// Add explicit document start marker `---` (default: false).
    pub explicit_start: Option<bool>,
}

impl Default for DumpOptions {
    fn default() -> Self {
        Self {
            sort_keys: Some(false),
            allow_unicode: Some(true),
            indent: Some(2.0),
            width: Some(80.0),
            default_flow_style: None,
            explicit_start: Some(false),
        }
    }
}

/// Serialize a JavaScript object to a YAML string.
///
/// This is equivalent to js-yaml's `safeDump()` and `PyYAML`'s `safe_dump()`.
///
/// # Arguments
///
/// * `data` - A JavaScript object to serialize (Object, Array, Set, Map, string, number, boolean, null)
/// * `options` - Optional serialization options
///
/// # Returns
///
/// A YAML string representation of the object
///
/// # Sets and maps
///
/// A `Set` is written as a `!!set` and a `Map` as a mapping. `safeLoad` reads a `!!set` back as an
/// object with `null` values (js-yaml's form), so the mapping is one-way.
///
/// # Errors
///
/// Throws an error if the object contains non-serializable types, or if two `Set` members or
/// `Map` keys are the same YAML value.
///
/// # Example
///
/// ```javascript
/// const { safeDump } = require('@fast-yaml/core');
///
/// const yaml = safeDump({ name: 'test', value: 123 });
/// console.log(yaml); // 'name: test\nvalue: 123\n'
/// ```
#[napi(catch_unwind)]
pub fn safe_dump(
    env: Env,
    data: Unknown<'static>,
    options: Option<DumpOptions>,
) -> napi::Result<String> {
    let opts = options.unwrap_or_default();
    throw_or_default(env, dump_one(env, data, &opts))
}

fn dump_one(env: Env, data: Unknown, opts: &DumpOptions) -> napi::Result<String> {
    let mut budget = DumpBudget::default();
    let mut yaml = js_to_yaml(env, data, &mut budget)?;
    if opts.sort_keys.unwrap_or(false) {
        yaml = sort_yaml_keys(&yaml);
    }
    let output = fast_yaml_core::Emitter::emit_str_with_config(&yaml, &emitter_config(opts)?)
        .map_err(emit_error)?;
    check_output_size(output)
}

/// Serialize multiple JavaScript objects to a YAML string with document separators.
///
/// This is equivalent to js-yaml's `safeDumpAll()` and `PyYAML`'s `safe_dump_all()`.
///
/// # Arguments
///
/// * `documents` - An array of JavaScript objects to serialize
/// * `options` - Optional serialization options
///
/// # Returns
///
/// A YAML string with multiple documents separated by "---"
///
/// # Errors
///
/// Throws an error if:
/// - Any object cannot be serialized
/// - Total output size exceeds 100MB limit
///
/// # Security
///
/// Maximum output size is limited to 100MB to prevent memory exhaustion.
///
/// # Example
///
/// ```javascript
/// const { safeDumpAll } = require('@fast-yaml/core');
///
/// const yaml = safeDumpAll([{ a: 1 }, { b: 2 }]);
/// console.log(yaml); // '---\na: 1\n---\nb: 2\n'
/// ```
#[napi(catch_unwind)]
pub fn safe_dump_all(
    env: Env,
    documents: Vec<Unknown<'static>>,
    options: Option<DumpOptions>,
) -> napi::Result<String> {
    let opts = options.unwrap_or_default();
    throw_or_default(env, dump_many(env, documents, &opts))
}

fn dump_many(env: Env, documents: Vec<Unknown>, opts: &DumpOptions) -> napi::Result<String> {
    let mut budget = DumpBudget::default();
    let mut yamls = Vec::with_capacity(documents.len());
    for doc in documents {
        let mut yaml = js_to_yaml(env, doc, &mut budget)?;
        if opts.sort_keys.unwrap_or(false) {
            yaml = sort_yaml_keys(&yaml);
        }
        yamls.push(yaml);
    }
    let output = fast_yaml_core::Emitter::emit_all_with_config(&yamls, &emitter_config(opts)?)
        .map_err(emit_error)?;
    check_output_size(output)
}

/// Helper function to recursively sort dictionary keys in YAML
fn sort_yaml_keys(yaml: &Value) -> Value {
    match yaml {
        Value::Mapping(map) => {
            let mut sorted: Vec<_> = map.iter().collect();
            sorted.sort_by(|(k1, _), (k2, _)| {
                let s1 = yaml_to_sort_key(k1);
                let s2 = yaml_to_sort_key(k2);
                s1.cmp(&s2)
            });
            let mut new_map = Mapping::new();
            for (k, v) in sorted {
                new_map.insert(k.clone(), sort_yaml_keys(v));
            }
            Value::Mapping(new_map)
        }
        Value::Sequence(arr) => Value::Sequence(arr.iter().map(sort_yaml_keys).collect()),
        other => other.clone(),
    }
}

/// Convert YAML value to a sortable string key
fn yaml_to_sort_key(yaml: &Value) -> String {
    match yaml {
        Value::String(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.get().to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null | Value::BigInt(_) | Value::Sequence(_) | Value::Mapping(_) | Value::Set(_) => {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_output_size() {
        assert!(check_output_size("ok".to_string()).is_ok());
        let over = "x".repeat(MaxOutputBytes::DEFAULT.get() + 1);
        assert!(check_output_size(over).is_err());
    }

    #[test]
    fn test_dump_options_default() {
        let opts = DumpOptions::default();
        assert_eq!(opts.sort_keys, Some(false));
        assert_eq!(opts.allow_unicode, Some(true));
        assert_eq!(opts.indent, Some(2.0));
        assert_eq!(opts.width, Some(80.0));
        assert_eq!(opts.default_flow_style, None);
        assert_eq!(opts.explicit_start, Some(false));
    }

    #[test]
    fn test_yaml_to_sort_key() {
        assert_eq!(yaml_to_sort_key(&Value::String("test".to_string())), "test");
        assert_eq!(yaml_to_sort_key(&Value::Int(42)), "42");
        assert_eq!(yaml_to_sort_key(&Value::Bool(true)), "true");
    }

    #[test]
    fn test_sort_yaml_keys() {
        let mut map = Mapping::new();
        map.insert(Value::String("z".to_string()), Value::Int(1));
        map.insert(Value::String("a".to_string()), Value::Int(2));
        map.insert(Value::String("m".to_string()), Value::Int(3));

        let yaml = Value::Mapping(map);
        let sorted = sort_yaml_keys(&yaml);

        if let Value::Mapping(sorted_map) = sorted {
            let keys: Vec<String> = sorted_map
                .keys()
                .map(|k| {
                    if let Value::String(s) = k {
                        s.clone()
                    } else {
                        String::new()
                    }
                })
                .collect();

            assert_eq!(keys, vec!["a", "m", "z"]);
        } else {
            panic!("Expected Mapping");
        }
    }
}
