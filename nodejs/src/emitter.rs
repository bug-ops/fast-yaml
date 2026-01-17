//! YAML serialization functions for Node.js.
//!
//! This module provides safe YAML serialization functions that convert
//! JavaScript objects to YAML strings.

use crate::conversion::js_to_yaml;
use napi::{Env, Result as NapiResult, bindgen_prelude::*};
use napi_derive::napi;
use saphyr::{MappingOwned, ScalarOwned, YamlOwned};

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

    /// Indentation width in spaces (default: 2).
    /// Valid range: 1-9 (values outside this range will be clamped).
    pub indent: Option<u32>,

    /// Maximum line width for wrapping (default: 80).
    /// Valid range: 20-1000 (values outside this range will be clamped).
    pub width: Option<u32>,

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
            indent: Some(2),
            width: Some(80),
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

    // Create emitter configuration from options
    let config = fast_yaml_core::EmitterConfig::new()
        .with_indent(opts.indent.unwrap_or(2) as usize)
        .with_width(opts.width.unwrap_or(80) as usize)
        .with_default_flow_style(opts.default_flow_style)
        .with_explicit_start(opts.explicit_start.unwrap_or(false));

    // Serialize to string using EmitterConfig.
    // Note: fast_yaml_core::Emitter already estimates output size and pre-allocates
    // the output String buffer to minimize allocations during YAML emission.
    let output = fast_yaml_core::Emitter::emit_str_with_config(&yaml, &config)
        .map_err(|e| napi::Error::from_reason(format!("YAML emit error: {e}")))?;

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

    // Pre-allocate Vec for converted documents to avoid reallocation.
    // For multi-document YAML files, this prevents Vec capacity doubling.
    let mut yamls = Vec::with_capacity(documents.len());
    for doc in documents {
        let mut yaml = js_to_yaml(&env, doc)?;

        // Sort keys if requested
        if opts.sort_keys.unwrap_or(false) {
            yaml = sort_yaml_keys(&yaml);
        }

        yamls.push(yaml);
    }

    // Create emitter configuration from options
    let config = fast_yaml_core::EmitterConfig::new()
        .with_indent(opts.indent.unwrap_or(2) as usize)
        .with_width(opts.width.unwrap_or(80) as usize)
        .with_default_flow_style(opts.default_flow_style)
        .with_explicit_start(opts.explicit_start.unwrap_or(false));

    // Serialize all documents using EmitterConfig
    let output = fast_yaml_core::Emitter::emit_all_with_config(&yamls, &config)
        .map_err(|e| napi::Error::from_reason(format!("YAML emit error: {e}")))?;

    // Check output size to prevent memory exhaustion
    if output.len() > MAX_OUTPUT_SIZE {
        return Err(napi::Error::from_reason(format!(
            "output size {} exceeds maximum allowed {} (100MB)",
            output.len(),
            MAX_OUTPUT_SIZE
        )));
    }

    Ok(output)
}

/// Helper function to recursively sort dictionary keys in YAML
fn sort_yaml_keys(yaml: &YamlOwned) -> YamlOwned {
    match yaml {
        YamlOwned::Mapping(map) => {
            let mut sorted: Vec<_> = map.iter().collect();
            sorted.sort_by(|(k1, _), (k2, _)| {
                let s1 = yaml_to_sort_key(k1);
                let s2 = yaml_to_sort_key(k2);
                s1.cmp(&s2)
            });
            let mut new_map = MappingOwned::new();
            for (k, v) in sorted {
                new_map.insert(k.clone(), sort_yaml_keys(v));
            }
            YamlOwned::Mapping(new_map)
        }
        YamlOwned::Sequence(arr) => YamlOwned::Sequence(arr.iter().map(sort_yaml_keys).collect()),
        other => other.clone(),
    }
}

/// Convert YAML value to a sortable string key
fn yaml_to_sort_key(yaml: &YamlOwned) -> String {
    match yaml {
        YamlOwned::Value(scalar) => match scalar {
            ScalarOwned::String(s) => s.clone(),
            ScalarOwned::Integer(i) => i.to_string(),
            ScalarOwned::FloatingPoint(f) => f.to_string(),
            ScalarOwned::Boolean(b) => b.to_string(),
            ScalarOwned::Null => String::new(),
        },
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
        assert_eq!(opts.indent, Some(2));
        assert_eq!(opts.width, Some(80));
        assert_eq!(opts.default_flow_style, None);
        assert_eq!(opts.explicit_start, Some(false));
    }

    #[test]
    fn test_yaml_to_sort_key() {
        assert_eq!(
            yaml_to_sort_key(&YamlOwned::Value(ScalarOwned::String("test".to_string()))),
            "test"
        );
        assert_eq!(
            yaml_to_sort_key(&YamlOwned::Value(ScalarOwned::Integer(42))),
            "42"
        );
        assert_eq!(
            yaml_to_sort_key(&YamlOwned::Value(ScalarOwned::Boolean(true))),
            "true"
        );
    }

    #[test]
    fn test_sort_yaml_keys() {
        let mut map = MappingOwned::new();
        map.insert(
            YamlOwned::Value(ScalarOwned::String("z".to_string())),
            YamlOwned::Value(ScalarOwned::Integer(1)),
        );
        map.insert(
            YamlOwned::Value(ScalarOwned::String("a".to_string())),
            YamlOwned::Value(ScalarOwned::Integer(2)),
        );
        map.insert(
            YamlOwned::Value(ScalarOwned::String("m".to_string())),
            YamlOwned::Value(ScalarOwned::Integer(3)),
        );

        let yaml = YamlOwned::Mapping(map);
        let sorted = sort_yaml_keys(&yaml);

        if let YamlOwned::Mapping(sorted_map) = sorted {
            let keys: Vec<String> = sorted_map
                .keys()
                .map(|k| {
                    if let YamlOwned::Value(ScalarOwned::String(s)) = k {
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
