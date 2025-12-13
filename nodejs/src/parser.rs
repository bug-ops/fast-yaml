//! YAML parsing functions for Node.js.
//!
//! This module provides safe YAML parsing functions that convert YAML strings
//! to JavaScript objects.

use crate::conversion::yaml_to_js;
use napi::{Env, Result as NapiResult, bindgen_prelude::*};
use napi_derive::napi;
use yaml_rust2::YamlLoader;

/// Maximum input size in bytes for `safe_load`/`safe_load_all` (100MB).
///
/// This limit prevents denial-of-service attacks via extremely large inputs.
const MAX_INPUT_SIZE: usize = 100 * 1024 * 1024;

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
    let docs = YamlLoader::load_from_str(&yaml_str)
        .map_err(|e| napi::Error::from_reason(format!("YAML parse error: {e}")))?;

    // Convert first document to JavaScript (or null if empty)
    let result = if docs.is_empty() {
        yaml_to_js(&env, &yaml_rust2::Yaml::Null)
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
    let docs = YamlLoader::load_from_str(&yaml_str)
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
        let docs = YamlLoader::load_from_str(yaml).unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_parse_multi_document() {
        let yaml = "---\nfoo: 1\n---\nbar: 2";
        let docs = YamlLoader::load_from_str(yaml).unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_parse_invalid() {
        let yaml = "invalid: [\n";
        let result = YamlLoader::load_from_str(yaml);
        assert!(result.is_err());
    }
}
