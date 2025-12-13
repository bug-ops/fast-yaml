//! YAML serialization functions for Node.js.
//!
//! This module provides safe YAML serialization functions that convert
//! JavaScript objects to YAML strings.

use crate::conversion::js_to_yaml;
use napi::{Env, Result as NapiResult, bindgen_prelude::*};
use napi_derive::napi;
use yaml_rust2::{Yaml, YamlEmitter};

/// Maximum output size in bytes for `safe_dump`/`safe_dump_all` (100MB).
///
/// This limit prevents memory exhaustion attacks.
const MAX_OUTPUT_SIZE: usize = 100 * 1024 * 1024;

/// Options for YAML serialization.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DumpOptions {
    /// If true, sort object keys alphabetically (default: false)
    pub sort_keys: Option<bool>,

    /// Allow unicode characters (default: true).
    /// Note: yaml-rust2 always outputs unicode; this is accepted for API compatibility.
    pub allow_unicode: Option<bool>,
}

impl Default for DumpOptions {
    fn default() -> Self {
        Self {
            sort_keys: Some(false),
            allow_unicode: Some(true),
        }
    }
}

/// Serialize a JavaScript object to a YAML string.
///
/// This is equivalent to js-yaml's `safeDump()` and `PyYAML`'s `safe_dump()`.
///
/// # Arguments
///
/// * `data` - A JavaScript object to serialize (Object, Array, string, number, boolean, null)
/// * `options` - Optional serialization options
///
/// # Returns
///
/// A YAML string representation of the object
///
/// # Errors
///
/// Throws an error if the object contains non-serializable types.
///
/// # Example
///
/// ```javascript
/// const { safeDump } = require('@fast-yaml/core');
///
/// const yaml = safeDump({ name: 'test', value: 123 });
/// console.log(yaml); // 'name: test\nvalue: 123\n'
/// ```
#[napi]
pub fn safe_dump(
    env: Env,
    data: Unknown<'static>,
    options: Option<DumpOptions>,
) -> NapiResult<String> {
    let opts = options.unwrap_or_default();

    // Convert JavaScript to YAML
    let mut yaml = js_to_yaml(&env, data)?;

    // Sort keys if requested
    if opts.sort_keys.unwrap_or(false) {
        yaml = sort_yaml_keys(&yaml);
    }

    // Serialize to string
    let mut output = String::new();
    let mut emitter = YamlEmitter::new(&mut output);

    emitter
        .dump(&yaml)
        .map_err(|e| napi::Error::from_reason(format!("YAML emit error: {e}")))?;

    // Remove the leading "---\n" that yaml-rust2 adds
    let output = if let Some(stripped) = output.strip_prefix("---\n") {
        stripped.to_string()
    } else if let Some(stripped) = output.strip_prefix("---") {
        stripped.trim_start_matches('\n').to_string()
    } else {
        output
    };

    Ok(output)
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
#[napi]
pub fn safe_dump_all(
    env: Env,
    documents: Vec<Unknown<'static>>,
    options: Option<DumpOptions>,
) -> NapiResult<String> {
    let opts = options.unwrap_or_default();

    // Convert all JavaScript objects to YAML first
    let mut yamls = Vec::with_capacity(documents.len());
    for doc in documents {
        let mut yaml = js_to_yaml(&env, doc)?;

        // Sort keys if requested
        if opts.sort_keys.unwrap_or(false) {
            yaml = sort_yaml_keys(&yaml);
        }

        yamls.push(yaml);
    }

    // Serialize all documents
    let mut output = String::new();

    for (i, yaml) in yamls.iter().enumerate() {
        if i > 0 {
            output.push_str("---\n");
        }

        let mut doc_output = String::new();
        let mut emitter = YamlEmitter::new(&mut doc_output);
        emitter
            .dump(yaml)
            .map_err(|e| napi::Error::from_reason(format!("YAML emit error: {e}")))?;

        // Remove the leading "---\n" that yaml-rust2 adds
        let doc_output = if let Some(stripped) = doc_output.strip_prefix("---\n") {
            stripped
        } else if let Some(stripped) = doc_output.strip_prefix("---") {
            stripped.trim_start_matches('\n')
        } else {
            &doc_output
        };

        output.push_str(doc_output);

        // Check output size to prevent memory exhaustion
        if output.len() > MAX_OUTPUT_SIZE {
            return Err(napi::Error::from_reason(format!(
                "output size exceeds maximum allowed {MAX_OUTPUT_SIZE} (100MB)"
            )));
        }
    }

    Ok(output)
}

/// Helper function to recursively sort dictionary keys in YAML
fn sort_yaml_keys(yaml: &Yaml) -> Yaml {
    match yaml {
        Yaml::Hash(map) => {
            let mut sorted: Vec<_> = map.iter().collect();
            sorted.sort_by(|(k1, _), (k2, _)| {
                let s1 = yaml_to_sort_key(k1);
                let s2 = yaml_to_sort_key(k2);
                s1.cmp(&s2)
            });
            let mut new_map = yaml_rust2::yaml::Hash::new();
            for (k, v) in sorted {
                new_map.insert(k.clone(), sort_yaml_keys(v));
            }
            Yaml::Hash(new_map)
        }
        Yaml::Array(arr) => Yaml::Array(arr.iter().map(sort_yaml_keys).collect()),
        other => other.clone(),
    }
}

/// Convert YAML value to a sortable string key
fn yaml_to_sort_key(yaml: &Yaml) -> String {
    match yaml {
        Yaml::String(s) | Yaml::Real(s) => s.clone(),
        Yaml::Integer(i) => i.to_string(),
        Yaml::Boolean(b) => b.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_output_size() {
        assert_eq!(MAX_OUTPUT_SIZE, 100 * 1024 * 1024);
    }

    #[test]
    fn test_dump_options_default() {
        let opts = DumpOptions::default();
        assert_eq!(opts.sort_keys, Some(false));
        assert_eq!(opts.allow_unicode, Some(true));
    }

    #[test]
    fn test_yaml_to_sort_key() {
        assert_eq!(yaml_to_sort_key(&Yaml::String("test".to_string())), "test");
        assert_eq!(yaml_to_sort_key(&Yaml::Integer(42)), "42");
        assert_eq!(yaml_to_sort_key(&Yaml::Boolean(true)), "true");
    }

    #[test]
    fn test_sort_yaml_keys() {
        let mut map = yaml_rust2::yaml::Hash::new();
        map.insert(Yaml::String("z".to_string()), Yaml::Integer(1));
        map.insert(Yaml::String("a".to_string()), Yaml::Integer(2));
        map.insert(Yaml::String("m".to_string()), Yaml::Integer(3));

        let yaml = Yaml::Hash(map);
        let sorted = sort_yaml_keys(&yaml);

        if let Yaml::Hash(sorted_map) = sorted {
            let keys: Vec<String> = sorted_map
                .keys()
                .map(|k| {
                    if let Yaml::String(s) = k {
                        s.clone()
                    } else {
                        String::new()
                    }
                })
                .collect();

            assert_eq!(keys, vec!["a", "m", "z"]);
        } else {
            panic!("Expected Hash");
        }
    }
}
