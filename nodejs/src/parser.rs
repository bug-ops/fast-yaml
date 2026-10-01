//! YAML parsing functions for Node.js.
//!
//! This module provides safe YAML parsing functions that convert YAML strings
//! to JavaScript objects.

use crate::Schema;
use crate::conversion::yaml_to_js;
use crate::limits::parse_limits;
use fast_yaml_core::limits::MaxInputBytes;
use fast_yaml_core::limits::ParseLimits;
use fast_yaml_core::{KeyDomain, LoadOptions as CoreLoadOptions, ParseResult, Parser, Value};
use napi::{Env, bindgen_prelude::*};
use napi_derive::napi;

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

    /// Maximum collection nesting depth (integer, 1..=512, default: 256); flow collections (`[]`, `{}`) stop at 255 levels whatever this is.
    /// Stack note: the calling thread needs about 1 MiB of stack at depth 512 (roughly 980 KiB measured in release); on stacks of 512 KiB or less (e.g. a worker with stackSizeMb 0.5) the process can abort and the overflow cannot be caught, while the default 256 is safe. The emitter keeps its own fixed depth of 256, so data parsed deeper may fail to dump.
    pub max_depth: Option<f64>,

    /// Maximum estimated bytes produced by alias expansion per call (integer,
    /// 1..=1073741824, default: 67108864). Host objects cost several times the estimate.
    pub max_alias_bytes: Option<f64>,

    /// Maximum characters the parser may read past the last node it reported (integer,
    /// 1..=1073741824, default: 4194304). A flow collection at the root or in a `- ` entry, one
    /// scalar, or a run of comments longer than this is rejected; parser memory is bounded by
    /// about 190 times this value.
    pub max_scan_ahead: Option<f64>,
}

impl LoadOptions {
    fn parse_limits(&self) -> napi::Result<fast_yaml_core::limits::ParseLimits> {
        parse_limits(self.max_depth, self.max_alias_bytes, self.max_scan_ahead)
    }
}

/// Loads every document with JavaScript object keys: keys that differ in YAML but share a
/// property name are a positioned error.
fn load_documents(yaml: &str, limits: &ParseLimits) -> ParseResult<Vec<Value>> {
    let options = CoreLoadOptions::new().with_keys(KeyDomain::StringKeys);
    Parser::parse_all_with_options(yaml, limits, options)
}

/// Return a JS `undefined` sentinel after calling `env.throw_error`.
///
/// After `env.throw_error`, NAPI-RS discards the return value and propagates the pending
/// JS exception. The sentinel `undefined` is never observed by JavaScript callers.
#[inline]
fn throw_and_undefined<'env>(env: &'env Env, msg: &str) -> napi::Result<Unknown<'env>> {
    env.throw_error(msg, None)?;
    ().into_unknown(env)
}

/// Parse a YAML string and return a JavaScript object.
///
/// This is equivalent to js-yaml's `safeLoad()` and `PyYAML`'s `safe_load()`.
///
/// # Arguments
///
/// * `yaml_str` - A YAML document as a string
/// * `options` - Optional parsing options; `maxDepth`, `maxAliasBytes` and `maxScanAhead` raise or lower the resource limits
///
/// # Returns
///
/// The parsed YAML document as JavaScript objects (Object, Array, string, number, boolean, null).
/// A `!!set` loads as an object whose members are keys with `null` values, like js-yaml.
///
/// # Errors
///
/// Throws an error if:
/// - The YAML is invalid
/// - A `!!set` member has a non-null value, a `<<` key is repeated in one mapping, or two keys differ in
///   YAML but share a JavaScript property name (`1` and `"1"`); the message carries the position
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
#[napi(catch_unwind)]
pub fn safe_load(
    env: &Env,
    yaml_str: String,
    options: Option<LoadOptions>,
) -> napi::Result<Unknown<'_>> {
    let limits = match options.unwrap_or_default().parse_limits() {
        Ok(l) => l,
        Err(e) => return throw_and_undefined(env, &e.reason),
    };
    // Validate input size to prevent DoS attacks
    if let Err(e) = MaxInputBytes::DEFAULT.check(yaml_str.len()) {
        return throw_and_undefined(env, &e.to_string());
    }

    // Parse YAML string
    let docs = match load_documents(&yaml_str, &limits) {
        Ok(d) => d,
        Err(e) => return throw_and_undefined(env, &format!("YAML parse error: {e}")),
    };

    // Convert first document to JavaScript (or null if empty)
    let doc = if docs.is_empty() {
        Value::Null
    } else {
        docs.into_iter().next().unwrap_or(Value::Null)
    };

    yaml_to_js(env, &doc).or_else(|e| throw_and_undefined(env, &e.to_string()))
}

/// Parse a YAML string containing multiple documents.
///
/// This is equivalent to js-yaml's `safeLoadAll()` and `PyYAML`'s `safe_load_all()`.
///
/// # Arguments
///
/// * `yaml_str` - A YAML string potentially containing multiple documents
/// * `options` - Optional parsing options; `maxDepth`, `maxAliasBytes` and `maxScanAhead` raise or lower the resource limits
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
/// - `maxDepth`, `maxAliasBytes` or `maxScanAhead` is not an integer within its range
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
#[napi(catch_unwind)]
pub fn safe_load_all(
    env: &Env,
    yaml_str: String,
    options: Option<LoadOptions>,
) -> napi::Result<Vec<Unknown<'_>>> {
    let limits = match options.unwrap_or_default().parse_limits() {
        Ok(l) => l,
        Err(e) => {
            env.throw_error(&e.reason, None)?;
            return Ok(Vec::new());
        }
    };
    // Validate input size to prevent DoS attacks
    if let Err(e) = MaxInputBytes::DEFAULT.check(yaml_str.len()) {
        env.throw_error(&e.to_string(), None)?;
        return Ok(Vec::new());
    }

    if yaml_str.trim().is_empty() {
        return Ok(Vec::new());
    }

    // Parse YAML string
    let docs = match load_documents(&yaml_str, &limits) {
        Ok(d) => d,
        Err(e) => {
            env.throw_error(&format!("YAML parse error: {e}"), None)?;
            return Ok(Vec::new());
        }
    };

    // Convert all documents to JavaScript
    let mut js_docs = Vec::with_capacity(docs.len());
    for doc in docs {
        match yaml_to_js(env, &doc) {
            Ok(v) => js_docs.push(v),
            Err(e) => {
                env.throw_error(&e.to_string(), None)?;
                return Ok(Vec::new());
            }
        }
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
#[napi(catch_unwind)]
pub fn load(
    env: &Env,
    yaml_str: String,
    options: Option<LoadOptions>,
) -> napi::Result<Unknown<'_>> {
    // Schema is ignored (safe by default); limits are honoured
    safe_load(env, yaml_str, options)
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
#[napi(catch_unwind)]
pub fn load_all(
    env: &Env,
    yaml_str: String,
    options: Option<LoadOptions>,
) -> napi::Result<Vec<Unknown<'_>>> {
    // Schema is ignored (safe by default); limits are honoured
    safe_load_all(env, yaml_str, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let yaml = "name: test\nvalue: 123";
        let docs: Vec<Value> = Parser::parse_all(yaml).unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_parse_multi_document() {
        let yaml = "---\nfoo: 1\n---\nbar: 2";
        let docs: Vec<Value> = Parser::parse_all(yaml).unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_parse_invalid() {
        let yaml = "invalid: [\n";
        let result = Parser::parse_all(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_options_default() {
        let opts = LoadOptions::default();
        assert!(opts.schema.is_none());
        assert!(opts.filename.is_none());
        assert!(opts.allow_duplicate_keys.is_none());
        assert!(opts.max_depth.is_none());
        assert!(opts.max_alias_bytes.is_none());
        assert!(opts.max_scan_ahead.is_none());
    }

    #[test]
    fn test_load_options_with_values() {
        let opts = LoadOptions {
            schema: Some(Schema::SafeSchema),
            filename: Some("test.yaml".to_string()),
            allow_duplicate_keys: Some(true),
            ..Default::default()
        };
        assert_eq!(opts.schema, Some(Schema::SafeSchema));
        assert_eq!(opts.filename, Some("test.yaml".to_string()));
        assert_eq!(opts.allow_duplicate_keys, Some(true));
    }
}
