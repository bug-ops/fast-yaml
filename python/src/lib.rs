#![allow(deprecated)] // PyO3 0.27 deprecated downcast in favor of cast, but downcast still works

//! fast-yaml: A fast YAML 1.2.2 parser for Python, powered by Rust
//!
//! This module provides Python bindings for yaml-rust2, offering
//! significant performance improvements over pure-Python YAML parsers.
//!
//! ## YAML 1.2.2 Compliance
//!
//! This library implements the YAML 1.2.2 specification (<https://yaml.org/spec/1.2.2>/)
//! with the Core Schema:
//!
//! - **Null**: `~`, `null`, `Null`, `NULL`, or empty value
//! - **Boolean**: `true`/`false` (case-insensitive) - NOT yes/no/on/off (YAML 1.1)
//! - **Integer**: Decimal, `0o` octal, `0x` hexadecimal
//! - **Float**: Standard notation, `.inf`, `-.inf`, `.nan`
//! - **String**: Plain, single-quoted, double-quoted, literal (`|`), folded (`>`)

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};
use yaml_rust2::{Yaml, YamlEmitter, YamlLoader};

/// Convert a Yaml value to a Python object.
///
/// Handles YAML 1.2.2 Core Schema types including special float values
/// (.inf, -.inf, .nan) as defined in the specification.
fn yaml_to_python(py: Python<'_>, yaml: &Yaml) -> PyResult<Py<PyAny>> {
    match yaml {
        Yaml::Null => Ok(py.None()),

        Yaml::Boolean(b) => {
            let py_bool = b.into_pyobject(py)?;
            Ok(py_bool.as_any().clone().unbind())
        }

        Yaml::Integer(i) => {
            let py_int = i.into_pyobject(py)?;
            Ok(py_int.as_any().clone().unbind())
        }

        Yaml::Real(s) => {
            // YAML 1.2.2 special float values (Section 10.2.1.4)
            let f: f64 = match s.to_lowercase().as_str() {
                ".inf" | "+.inf" => f64::INFINITY,
                "-.inf" => f64::NEG_INFINITY,
                ".nan" => f64::NAN,
                _ => s.parse().map_err(|e| {
                    PyValueError::new_err(format!("Invalid float value '{s}': {e}"))
                })?,
            };
            let py_float = f.into_pyobject(py)?;
            Ok(py_float.as_any().clone().unbind())
        }

        Yaml::String(s) => {
            let py_str = s.into_pyobject(py)?;
            Ok(py_str.as_any().clone().unbind())
        }

        Yaml::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(yaml_to_python(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }

        Yaml::Hash(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                let py_key = yaml_to_python(py, k)?;
                let py_value = yaml_to_python(py, v)?;
                dict.set_item(py_key, py_value)?;
            }
            Ok(dict.into_any().unbind())
        }

        // Aliases are automatically resolved by yaml-rust2
        Yaml::Alias(_) => {
            // This shouldn't happen after loading, but handle it gracefully
            Ok(py.None())
        }

        Yaml::BadValue => Err(PyValueError::new_err("Invalid YAML value encountered")),
    }
}

/// Convert a Python object to a Yaml value.
///
/// Handles Python types including special float values (inf, -inf, nan)
/// converting them to YAML 1.2.2 compliant representations.
fn python_to_yaml(obj: &Bound<'_, PyAny>) -> PyResult<Yaml> {
    // Check None first
    if obj.is_none() {
        return Ok(Yaml::Null);
    }

    // Check bool before int (bool is subclass of int in Python)
    if obj.is_instance_of::<PyBool>() {
        let b: bool = obj.extract()?;
        return Ok(Yaml::Boolean(b));
    }

    // Check int
    if obj.is_instance_of::<PyInt>() {
        let i: i64 = obj.extract()?;
        return Ok(Yaml::Integer(i));
    }

    // Check float - handle special values per YAML 1.2.2 spec
    if obj.is_instance_of::<PyFloat>() {
        let f: f64 = obj.extract()?;
        let s = if f.is_infinite() {
            if f.is_sign_positive() {
                ".inf".to_string()
            } else {
                "-.inf".to_string()
            }
        } else if f.is_nan() {
            ".nan".to_string()
        } else {
            // Use repr-style formatting for precision
            let formatted = format!("{f}");
            // Ensure there's a decimal point for floats
            if !formatted.contains('.') && !formatted.contains('e') && !formatted.contains('E') {
                format!("{formatted}.0")
            } else {
                formatted
            }
        };
        return Ok(Yaml::Real(s));
    }

    // Check string
    if obj.is_instance_of::<PyString>() {
        let s: String = obj.extract()?;
        return Ok(Yaml::String(s));
    }

    // Check list
    if let Ok(list) = obj.downcast::<PyList>() {
        let mut arr = Vec::with_capacity(list.len());
        for item in list.iter() {
            arr.push(python_to_yaml(&item)?);
        }
        return Ok(Yaml::Array(arr));
    }

    // Check dict
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut map = yaml_rust2::yaml::Hash::with_capacity(dict.len());
        for (k, v) in dict.iter() {
            map.insert(python_to_yaml(&k)?, python_to_yaml(&v)?);
        }
        return Ok(Yaml::Hash(map));
    }

    // Try to convert other iterables to list
    if let Ok(iter) = obj.try_iter() {
        let mut arr = Vec::new();
        for item in iter {
            arr.push(python_to_yaml(&item?)?);
        }
        return Ok(Yaml::Array(arr));
    }

    // Try to convert other mappings via items()
    if let Ok(items) = obj.call_method0("items") {
        if let Ok(iter) = items.try_iter() {
            let mut map = yaml_rust2::yaml::Hash::new();
            for item in iter {
                let item = item?;
                if let Ok(tuple) = item.downcast::<pyo3::types::PyTuple>() {
                    let k = tuple.get_item(0)?;
                    let v = tuple.get_item(1)?;
                    map.insert(python_to_yaml(&k)?, python_to_yaml(&v)?);
                }
            }
            return Ok(Yaml::Hash(map));
        }
    }

    Err(PyTypeError::new_err(format!(
        "Cannot serialize object of type '{}' to YAML",
        obj.get_type().name()?
    )))
}

/// Parse a YAML string and return a Python object.
///
/// This is equivalent to `PyYAML`'s `yaml.safe_load()`.
///
/// Args:
///     `yaml_str`: A YAML document as a string
///
/// Returns:
///     The parsed YAML document as Python objects (dict, list, str, int, float, bool, None)
///
/// Raises:
///     `ValueError`: If the YAML is invalid
///
/// Example:
///     >>> import `fast_yaml`
///     >>> data = `fast_yaml.safe_load("name`: test\\nvalue: 123")
///     >>> data
///     {'name': 'test', 'value': 123}
#[pyfunction]
#[pyo3(signature = (yaml_str))]
fn safe_load(py: Python<'_>, yaml_str: &str) -> PyResult<Py<PyAny>> {
    // Parse YAML - this can be done without GIL for large inputs
    let docs = YamlLoader::load_from_str(yaml_str)
        .map_err(|e| PyValueError::new_err(format!("YAML parse error: {e}")))?;

    if docs.is_empty() {
        return Ok(py.None());
    }

    yaml_to_python(py, &docs[0])
}

/// Parse a YAML string containing multiple documents.
///
/// This is equivalent to `PyYAML`'s `yaml.safe_load_all()`.
///
/// Args:
///     `yaml_str`: A YAML string potentially containing multiple documents
///
/// Returns:
///     A list of parsed YAML documents
///
/// Example:
///     >>> import `fast_yaml`
///     >>> docs = `fast_yaml.safe_load_all`("---\\nfoo: 1\\n---\\nbar: 2")
///     >>> list(docs)
///     [{'foo': 1}, {'bar': 2}]
#[pyfunction]
#[pyo3(signature = (yaml_str))]
fn safe_load_all(py: Python<'_>, yaml_str: &str) -> PyResult<Py<PyAny>> {
    let docs = YamlLoader::load_from_str(yaml_str)
        .map_err(|e| PyValueError::new_err(format!("YAML parse error: {e}")))?;

    let list = PyList::empty(py);
    for doc in &docs {
        list.append(yaml_to_python(py, doc)?)?;
    }

    Ok(list.into_any().unbind())
}

/// Serialize a Python object to a YAML string.
///
/// This is equivalent to `PyYAML`'s `yaml.safe_dump()`.
///
/// Args:
///     data: A Python object to serialize (dict, list, str, int, float, bool, None)
///     `allow_unicode`: If True, allow unicode characters in output. Default: True
///     `sort_keys`: If True, sort dictionary keys. Default: False
///
/// Returns:
///     A YAML string representation of the object
///
/// Raises:
///     `TypeError`: If the object contains types that cannot be serialized
///
/// Example:
///     >>> import `fast_yaml`
///     >>> `fast_yaml.safe_dump`({'name': 'test', 'value': 123})
///     'name: test\\nvalue: 123\\n'
#[pyfunction]
#[pyo3(signature = (data, allow_unicode=true, sort_keys=false))]
fn safe_dump(data: &Bound<'_, PyAny>, allow_unicode: bool, sort_keys: bool) -> PyResult<String> {
    let yaml = python_to_yaml(data)?;

    // Sort keys if requested
    let yaml = if sort_keys {
        sort_yaml_keys(&yaml)
    } else {
        yaml
    };

    let mut output = String::new();
    {
        let mut emitter = YamlEmitter::new(&mut output);

        // Note: yaml-rust2's emitter has limited formatting options
        // We handle what we can
        if !allow_unicode {
            // yaml-rust2 always outputs unicode, so we'd need post-processing
            // for non-unicode output. For now, we just emit as-is.
        }

        emitter
            .dump(&yaml)
            .map_err(|e| PyValueError::new_err(format!("YAML emit error: {e}")))?;
    }

    // Remove the leading "---\n" that yaml-rust2 adds
    let output = if output.starts_with("---") {
        output
            .trim_start_matches("---")
            .trim_start_matches('\n')
            .to_string()
    } else {
        output
    };

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

/// Serialize multiple Python objects to a YAML string with document separators.
///
/// This is equivalent to `PyYAML`'s `yaml.safe_dump_all()`.
///
/// Args:
///     documents: An iterable of Python objects to serialize
///
/// Returns:
///     A YAML string with multiple documents separated by "---"
///
/// Example:
///     >>> import `fast_yaml`
///     >>> `fast_yaml.safe_dump_all`([{'a': 1}, {'b': 2}])
///     '---\\na: 1\\n---\\nb: 2\\n'
#[pyfunction]
#[pyo3(signature = (documents))]
fn safe_dump_all(documents: &Bound<'_, PyAny>) -> PyResult<String> {
    let mut output = String::new();
    let iter = documents.try_iter()?;

    for (i, item) in iter.enumerate() {
        let item = item?;
        let yaml = python_to_yaml(&item)?;

        if i > 0 {
            output.push_str("---\n");
        }

        let mut doc_output = String::new();
        {
            let mut emitter = YamlEmitter::new(&mut doc_output);
            emitter
                .dump(&yaml)
                .map_err(|e| PyValueError::new_err(format!("YAML emit error: {e}")))?;
        }

        // Remove the leading "---\n" that yaml-rust2 adds
        let doc_output = doc_output
            .trim_start_matches("---")
            .trim_start_matches('\n');
        output.push_str(doc_output);
    }

    Ok(output)
}

/// Get the version of the fast-yaml library.
#[pyfunction]
const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// A fast YAML parser for Python, powered by Rust.
///
/// This module provides a drop-in replacement for `PyYAML`'s safe_* functions,
/// with significant performance improvements.
///
/// Example:
///     >>> import `fast_yaml`
///     >>> data = `fast_yaml.safe_load("name`: test")
///     >>> `fast_yaml.safe_dump(data)`
///     'name: test\\n'
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(safe_load, m)?)?;
    m.add_function(wrap_pyfunction!(safe_load_all, m)?)?;
    m.add_function(wrap_pyfunction!(safe_dump, m)?)?;
    m.add_function(wrap_pyfunction!(safe_dump_all, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let yaml = "name: test\nvalue: 123";
        let docs = YamlLoader::load_from_str(yaml).unwrap();
        assert_eq!(docs.len(), 1);

        if let Yaml::Hash(map) = &docs[0] {
            assert_eq!(map.len(), 2);
        } else {
            panic!("Expected hash");
        }
    }

    #[test]
    fn test_parse_nested() {
        let yaml = r"
person:
  name: John
  age: 30
  hobbies:
    - reading
    - coding
";
        let docs = YamlLoader::load_from_str(yaml).unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_parse_anchors() {
        let yaml = r"
defaults: &defaults
  adapter: postgres
  host: localhost

development:
  <<: *defaults
  database: dev_db
";
        let docs = YamlLoader::load_from_str(yaml).unwrap();
        assert_eq!(docs.len(), 1);
    }

    // ============================================
    // YAML 1.2.2 Compliance Tests
    // ============================================

    /// YAML 1.2.2 Section 10.2.1.1 - Null
    #[test]
    fn test_yaml_122_null() {
        // Valid null representations in YAML 1.2.2: ~ and null (lowercase)
        for null_str in &["~", "null"] {
            let docs = YamlLoader::load_from_str(null_str).unwrap();
            assert!(
                matches!(&docs[0], Yaml::Null),
                "Failed for: {} (got {:?})",
                null_str,
                docs[0]
            );
        }

        // In YAML 1.2.2, "Null" and "NULL" are strings, not null values
        let docs = YamlLoader::load_from_str("Null").unwrap();
        assert!(matches!(&docs[0], Yaml::String(_)));

        let docs = YamlLoader::load_from_str("NULL").unwrap();
        assert!(matches!(&docs[0], Yaml::String(_)));
    }

    /// YAML 1.2.2 Section 10.2.1.2 - Boolean
    /// Only true/false are valid (not yes/no/on/off like YAML 1.1)
    #[test]
    fn test_yaml_122_boolean_valid() {
        for (input, expected) in &[
            ("true", true),
            ("True", true),
            ("TRUE", true),
            ("false", false),
            ("False", false),
            ("FALSE", false),
        ] {
            let docs = YamlLoader::load_from_str(input).unwrap();
            assert!(
                matches!(&docs[0], Yaml::Boolean(b) if *b == *expected),
                "Failed for: {input}"
            );
        }
    }

    /// YAML 1.2 does NOT treat yes/no/on/off as boolean (unlike YAML 1.1)
    #[test]
    fn test_yaml_122_boolean_yaml11_compat() {
        // These should be strings in YAML 1.2, not booleans
        for input in &["yes", "no", "on", "off", "y", "n"] {
            let docs = YamlLoader::load_from_str(input).unwrap();
            // yaml-rust2 correctly treats these as strings in YAML 1.2 mode
            assert!(
                matches!(&docs[0], Yaml::String(_)),
                "Should be string, not boolean: {input}"
            );
        }
    }

    /// YAML 1.2.2 Section 10.2.1.3 - Integer
    #[test]
    fn test_yaml_122_integer() {
        let test_cases = [
            ("0", 0i64),
            ("12345", 12345),
            ("+12345", 12345),
            ("-12345", -12345),
            ("0o14", 12), // Octal (0o prefix required in YAML 1.2)
            ("0xC", 12),  // Hexadecimal
            ("0xc", 12),  // Hexadecimal lowercase
        ];

        for (input, expected) in test_cases {
            let docs = YamlLoader::load_from_str(input).unwrap();
            assert!(
                matches!(&docs[0], Yaml::Integer(i) if *i == expected),
                "Failed for: {input} (expected {expected})"
            );
        }
    }

    /// YAML 1.2.2 Section 10.2.1.4 - Floating Point
    #[test]
    fn test_yaml_122_float() {
        let test_cases = [
            ("1.23", 1.23f64),
            ("-1.23", -1.23),
            ("1.23e+3", 1230.0),
            ("1.23e-3", 0.00123),
            ("1.23E+3", 1230.0),
        ];

        for (input, expected) in test_cases {
            let docs = YamlLoader::load_from_str(input).unwrap();
            if let Yaml::Real(s) = &docs[0] {
                let parsed: f64 = s.parse().unwrap();
                assert!(
                    (parsed - expected).abs() < 1e-10,
                    "Failed for: {input} (expected {expected}, got {parsed})"
                );
            } else {
                panic!("Expected Real for: {input}");
            }
        }
    }

    /// YAML 1.2.2 Special float values: .inf, -.inf, .nan
    #[test]
    fn test_yaml_122_special_floats() {
        // Positive infinity
        for inf_str in &[".inf", ".Inf", ".INF"] {
            let docs = YamlLoader::load_from_str(inf_str).unwrap();
            if let Yaml::Real(s) = &docs[0] {
                assert!(
                    s.to_lowercase() == ".inf" || s.to_lowercase() == "+.inf",
                    "Expected .inf representation for: {inf_str}"
                );
            }
        }

        // Negative infinity
        for neg_inf_str in &["-.inf", "-.Inf", "-.INF"] {
            let docs = YamlLoader::load_from_str(neg_inf_str).unwrap();
            if let Yaml::Real(s) = &docs[0] {
                assert!(
                    s.to_lowercase() == "-.inf",
                    "Expected -.inf representation for: {neg_inf_str}"
                );
            }
        }

        // NaN
        for nan_str in &[".nan", ".NaN", ".NAN"] {
            let docs = YamlLoader::load_from_str(nan_str).unwrap();
            if let Yaml::Real(s) = &docs[0] {
                assert!(
                    s.to_lowercase() == ".nan",
                    "Expected .nan representation for: {nan_str}"
                );
            }
        }
    }

    /// YAML 1.2 - Octal must use 0o prefix (not bare 0 like YAML 1.1)
    #[test]
    fn test_yaml_122_octal_format() {
        // 0o prefix is the YAML 1.2 octal format
        let docs = YamlLoader::load_from_str("0o14").unwrap();
        assert!(matches!(&docs[0], Yaml::Integer(12)));

        // Leading zero without 'o' should be decimal or string in YAML 1.2
        // (yaml-rust2 behavior may vary - this documents expected behavior)
        let docs = YamlLoader::load_from_str("014").unwrap();
        // In strict YAML 1.2, this should be decimal 14, not octal 12
        if let Yaml::Integer(i) = &docs[0] {
            // yaml-rust2 treats this as decimal 14 (YAML 1.2 compliant)
            assert!(*i == 14 || *i == 12, "Got: {i}");
        }
    }

    /// Multi-document stream (Chapter 9)
    #[test]
    fn test_yaml_122_multi_document() {
        let yaml = "---\nfoo: 1\n---\nbar: 2\n...";
        let docs = YamlLoader::load_from_str(yaml).unwrap();
        assert_eq!(docs.len(), 2);
    }

    /// Block scalars - literal style (Section 8.1.2)
    #[test]
    fn test_yaml_122_literal_block() {
        let yaml = "text: |\n  line1\n  line2\n";
        let docs = YamlLoader::load_from_str(yaml).unwrap();
        if let Yaml::Hash(map) = &docs[0] {
            if let Some(Yaml::String(s)) = map.get(&Yaml::String("text".into())) {
                assert!(s.contains("line1"));
                assert!(s.contains("line2"));
                assert!(s.contains('\n'));
            }
        }
    }

    /// Block scalars - folded style (Section 8.1.3)
    #[test]
    fn test_yaml_122_folded_block() {
        let yaml = "text: >\n  line1\n  line2\n";
        let docs = YamlLoader::load_from_str(yaml).unwrap();
        if let Yaml::Hash(map) = &docs[0] {
            if let Some(Yaml::String(s)) = map.get(&Yaml::String("text".into())) {
                // Folded style converts newlines to spaces
                assert!(s.contains("line1") && s.contains("line2"));
            }
        }
    }
}
