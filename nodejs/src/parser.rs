//! YAML parsing functions for Node.js.
//!
//! This module provides safe YAML parsing functions that convert YAML strings
//! to JavaScript objects.

use crate::Schema;
use crate::conversion::yaml_to_js;
use napi::{Env, Result as NapiResult, bindgen_prelude::*};
use napi_derive::napi;
use saphyr::{LoadableYamlNode, ScalarOwned, YamlOwned};

/// Maximum input size in bytes for `safe_load`/`safe_load_all` (100MB).
///
/// This limit prevents denial-of-service attacks via extremely large inputs.
const MAX_INPUT_SIZE: usize = 100 * 1024 * 1024;

/// Options for YAML parsing (js-yaml compatible).
#[napi(object)]
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// YAML schema to use for parsing (default: `SafeSchema`).
    /// Currently all schemas behave as `SafeSchema` (safe by default).
    pub schema: Option<Schema>,

    /// Filename or source name for error messages (default: `<input>`).
    pub filename: Option<String>,

    /// Allow duplicate keys in mappings (default: true).
    /// Note: fast-yaml always allows duplicates; this is for API compatibility.
    pub allow_duplicate_keys: Option<bool>,
}

/// Parse a YAML string and return a JavaScript object.
///
/// This is equivalent to js-yaml's `safeLoad()` and `PyYAML`'s `safe_load()`.
///
/// # Arguments
///
/// * `yaml_str` - A YAML document as a string
///
/// # Returns
///
/// The parsed YAML document as JavaScript objects (Object, Array, string, number, boolean, null)
///
/// # Errors
///
/// Throws an error if:
/// - The YAML is invalid
/// - Input exceeds size limit (100MB)
///
/// # Security
///
/// Maximum input size is limited to 100MB to prevent denial-of-service attacks.
///
/// # Example
///
/// ```javascript
/// const { safeLoad } = require('@fast-yaml/core');
///
/// const data = safeLoad('name: test\nvalue: 123');
/// console.log(data); // { name: 'test', value: 123 }
/// ```
// NAPI-RS requires String by value for proper FFI handling
#[allow(clippy::needless_pass_by_value)]
#[napi]
pub fn safe_load(env: Env, yaml_str: String) -> NapiResult<Unknown<'static>> {
    // Validate input size to prevent DoS attacks
    if yaml_str.len() > MAX_INPUT_SIZE {
        return Err(napi::Error::from_reason(format!(
            "input size {} exceeds maximum allowed {} (100MB)",
            yaml_str.len(),
            MAX_INPUT_SIZE
        )));
    }

    // Parse YAML string
    let docs: Vec<YamlOwned> = YamlOwned::load_from_str(&yaml_str)
        .map_err(|e| napi::Error::from_reason(format!("YAML parse error: {e}")))?;

    // Convert first document to JavaScript (or null if empty)
    let result = if docs.is_empty() {
        yaml_to_js(&env, &YamlOwned::Value(ScalarOwned::Null))
    } else {
        yaml_to_js(&env, &docs[0])
    }?;

    // SAFETY: The Env parameter in #[napi] functions has a 'static lifetime
    // in practice, as it's valid for the entire JavaScript call.
    // We transmute the lifetime to allow returning Unknown<'static>.
    #[allow(clippy::missing_transmute_annotations)]
    Ok(unsafe { std::mem::transmute(result) })
}

/// Parse a YAML string containing multiple documents.
///
/// This is equivalent to js-yaml's `safeLoadAll()` and `PyYAML`'s `safe_load_all()`.
///
/// # Arguments
///
/// * `yaml_str` - A YAML string potentially containing multiple documents
///
/// # Returns
///
/// An array of parsed JavaScript objects
///
/// # Errors
///
/// Throws an error if:
/// - The YAML is invalid
/// - Input exceeds size limit (100MB)
///
/// # Security
///
/// Maximum input size is limited to 100MB to prevent denial-of-service attacks.
///
/// # Example
///
/// ```javascript
/// const { safeLoadAll } = require('@fast-yaml/core');
///
/// const docs = safeLoadAll('---\nfoo: 1\n---\nbar: 2');
/// console.log(docs); // [{ foo: 1 }, { bar: 2 }]
/// ```
// NAPI-RS requires String by value for proper FFI handling
#[allow(clippy::needless_pass_by_value)]
#[napi]
pub fn safe_load_all(env: Env, yaml_str: String) -> NapiResult<Vec<Unknown<'static>>> {
    // Validate input size to prevent DoS attacks
    if yaml_str.len() > MAX_INPUT_SIZE {
        return Err(napi::Error::from_reason(format!(
            "input size {} exceeds maximum allowed {} (100MB)",
            yaml_str.len(),
            MAX_INPUT_SIZE
        )));
    }

    // Parse YAML string
    let docs: Vec<YamlOwned> = YamlOwned::load_from_str(&yaml_str)
        .map_err(|e| napi::Error::from_reason(format!("YAML parse error: {e}")))?;

    // Convert all documents to JavaScript
    let mut js_docs = Vec::with_capacity(docs.len());
    for doc in &docs {
        let result = yaml_to_js(&env, doc)?;
        // SAFETY: The Env parameter in #[napi] functions has a 'static lifetime
        // in practice, as it's valid for the entire JavaScript call.
        // We transmute the lifetime to allow storing Unknown<'static>.
        #[allow(clippy::missing_transmute_annotations)]
        js_docs.push(unsafe { std::mem::transmute(result) });
    }

    Ok(js_docs)
}

/// Parse a YAML string with options (js-yaml compatible).
///
/// This is the js-yaml compatible `load()` function that accepts an options object.
/// Currently all schemas behave as `SafeSchema` (safe by default).
///
/// # Arguments
///
/// * `yaml_str` - A YAML document as a string
/// * `options` - Optional parsing options (schema, filename, etc.)
///
/// # Returns
///
/// The parsed YAML document as JavaScript objects
///
/// # Errors
///
/// Throws an error if:
/// - The YAML is invalid
/// - Input exceeds size limit (100MB)
///
/// # Example
///
/// ```javascript
/// const { load, SAFE_SCHEMA } = require('@fast-yaml/core');
///
/// const data = load('name: test', { schema: 'SafeSchema' });
/// console.log(data); // { name: 'test' }
/// ```
// NAPI-RS requires String by value for proper FFI handling
#[allow(clippy::needless_pass_by_value)]
#[napi]
pub fn load(
    env: Env,
    yaml_str: String,
    options: Option<LoadOptions>,
) -> NapiResult<Unknown<'static>> {
    // Options are accepted for API compatibility but schema is ignored (safe by default)
    let _opts = options.unwrap_or_default();

    // Delegate to safe_load
    safe_load(env, yaml_str)
}

/// Parse a YAML string containing multiple documents with options (js-yaml compatible).
///
/// This is the js-yaml compatible `loadAll()` function that accepts an options object.
/// Currently all schemas behave as `SafeSchema` (safe by default).
///
/// # Arguments
///
/// * `yaml_str` - A YAML string potentially containing multiple documents
/// * `options` - Optional parsing options (schema, filename, etc.)
///
/// # Returns
///
/// An array of parsed JavaScript objects
///
/// # Errors
///
/// Throws an error if:
/// - The YAML is invalid
/// - Input exceeds size limit (100MB)
///
/// # Example
///
/// ```javascript
/// const { loadAll, SAFE_SCHEMA } = require('@fast-yaml/core');
///
/// const docs = loadAll('---\nfoo: 1\n---\nbar: 2', { schema: 'SafeSchema' });
/// console.log(docs); // [{ foo: 1 }, { bar: 2 }]
/// ```
// NAPI-RS requires String by value for proper FFI handling
#[allow(clippy::needless_pass_by_value)]
#[napi]
pub fn load_all(
    env: Env,
    yaml_str: String,
    options: Option<LoadOptions>,
) -> NapiResult<Vec<Unknown<'static>>> {
    // Options are accepted for API compatibility but schema is ignored (safe by default)
    let _opts = options.unwrap_or_default();

    // Delegate to safe_load_all
    safe_load_all(env, yaml_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_input_size() {
        assert_eq!(MAX_INPUT_SIZE, 100 * 1024 * 1024);
    }

    #[test]
    fn test_parse_simple() {
        let yaml = "name: test\nvalue: 123";
        let docs: Vec<YamlOwned> = YamlOwned::load_from_str(yaml).unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_parse_multi_document() {
        let yaml = "---\nfoo: 1\n---\nbar: 2";
        let docs: Vec<YamlOwned> = YamlOwned::load_from_str(yaml).unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_parse_invalid() {
        let yaml = "invalid: [\n";
        let result: Result<Vec<YamlOwned>, _> = YamlOwned::load_from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_options_default() {
        let opts = LoadOptions::default();
        assert!(opts.schema.is_none());
        assert!(opts.filename.is_none());
        assert!(opts.allow_duplicate_keys.is_none());
    }

    #[test]
    fn test_load_options_with_values() {
        let opts = LoadOptions {
            schema: Some(Schema::SafeSchema),
            filename: Some("test.yaml".to_string()),
            allow_duplicate_keys: Some(true),
        };
        assert_eq!(opts.schema, Some(Schema::SafeSchema));
        assert_eq!(opts.filename, Some("test.yaml".to_string()));
        assert_eq!(opts.allow_duplicate_keys, Some(true));
    }
}
