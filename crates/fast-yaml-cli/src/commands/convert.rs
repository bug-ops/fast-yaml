use anyhow::{Context, Result};
use fast_yaml_core::limits::ParseLimits;
use fast_yaml_core::{Emitter, Parser, ResolvedScalar, Value, resolve_scalar};
use serde_json;

use crate::cli::ConvertFormat;
use crate::io::{InputSource, OutputWriter};

/// Convert command implementation
pub struct ConvertCommand {
    target_format: ConvertFormat,
    pretty: bool,
    limits: ParseLimits,
}

impl ConvertCommand {
    pub const fn new(target_format: ConvertFormat, pretty: bool, limits: ParseLimits) -> Self {
        Self {
            target_format,
            pretty,
            limits,
        }
    }

    /// Execute convert command
    pub fn execute(&self, input: &InputSource, output: &OutputWriter) -> Result<()> {
        match self.target_format {
            ConvertFormat::Json => self.yaml_to_json(input, output),
            ConvertFormat::Yaml => Self::json_to_yaml(input, output),
        }
    }

    /// Convert YAML to JSON
    fn yaml_to_json(&self, input: &InputSource, output: &OutputWriter) -> Result<()> {
        // Parse all YAML documents to support multi-document streams
        let docs = Parser::parse_all_with_limits(input.as_str(), &self.limits)
            .context("Failed to parse YAML")?;

        if docs.is_empty() {
            return Err(anyhow::anyhow!("Empty YAML document"));
        }

        let json_value = if docs.len() == 1 {
            // Single document: preserve existing behaviour (plain object/value)
            value_to_json(&docs[0])?
        } else {
            // Multi-document stream: output a JSON array
            let arr: Result<Vec<_>> = docs.iter().map(value_to_json).collect();
            serde_json::Value::Array(arr?)
        };

        // Serialize to JSON
        let mut json_string = if self.pretty {
            serde_json::to_string_pretty(&json_value).context("Failed to serialize JSON")?
        } else {
            serde_json::to_string(&json_value).context("Failed to serialize JSON")?
        };

        // Add trailing newline for JSON
        json_string.push('\n');

        // Write output
        output.write(&json_string)?;

        Ok(())
    }

    /// Convert JSON to YAML
    fn json_to_yaml(input: &InputSource, output: &OutputWriter) -> Result<()> {
        // Parse JSON
        let json_value: serde_json::Value =
            serde_json::from_str(fast_yaml_core::strip_bom(input.as_str()))
                .context("Failed to parse JSON")?;

        // Convert to YAML Value
        let yaml_value = json_to_value(&json_value)?;

        // Emit YAML
        let yaml_string = Emitter::emit_str(&yaml_value).context("Failed to emit YAML")?;

        // Write output
        output.write(&yaml_string)?;

        Ok(())
    }
}

/// Coerce a YAML scalar key to its string representation for JSON output.
///
/// JSON only supports string keys. Scalar YAML keys are converted to their
/// canonical string form: null -> "null", bool -> "true"/"false",
/// integers and floats -> their decimal string representation.
///
/// # Errors
///
/// Returns an error for non-scalar key types (mappings, sequences, aliases)
/// that have no meaningful string representation.
fn yaml_key_to_string(key: &Value) -> Result<String> {
    fast_yaml_core::value::scalar_key_text(key).ok_or_else(|| {
        anyhow::anyhow!(
            "Unsupported YAML map key type: only scalar keys (string, number, boolean, null) \
             can be converted to JSON"
        )
    })
}

/// Convert `fast_yaml_core::Value` to `serde_json::Value`
fn value_to_json(value: &Value) -> Result<serde_json::Value> {
    use Value as YValue;
    use fast_yaml_core::value::ScalarOwned;
    use serde_json::Value as JValue;

    Ok(match value {
        YValue::Value(scalar) => match scalar {
            ScalarOwned::Null => JValue::Null,
            ScalarOwned::Boolean(b) => JValue::Bool(*b),
            ScalarOwned::Integer(i) => JValue::Number((*i).into()),
            ScalarOwned::FloatingPoint(f) => serde_json::Number::from_f64(f.0)
                .map(JValue::Number)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "YAML value '{f}' cannot be represented in JSON \
                     (JSON does not support infinity/NaN). \
                     Consider replacing with a numeric sentinel value."
                    )
                })?,
            ScalarOwned::String(s) => JValue::String(s.clone()),
        },
        YValue::Sequence(arr) => {
            let json_arr: Result<Vec<_>> = arr.iter().map(value_to_json).collect();
            JValue::Array(json_arr?)
        }
        YValue::Mapping(map) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                let key = yaml_key_to_string(k)?;
                json_map.insert(key, value_to_json(v)?);
            }
            JValue::Object(json_map)
        }
        YValue::Alias(_) => {
            anyhow::bail!("YAML aliases are not supported in JSON conversion");
        }
        YValue::BadValue => {
            anyhow::bail!("Invalid YAML value encountered");
        }
        YValue::Representation(s, style, tag) => match resolve_scalar(s, *style, tag.as_ref()) {
            ResolvedScalar::BigInt(big) => JValue::Number(
                big.canonical()
                    .parse::<serde_json::Number>()
                    .with_context(|| format!("invalid big integer '{s}'"))?,
            ),
            _ => JValue::String(s.clone()),
        },
        YValue::Tagged(_, inner) => {
            // Ignore the tag and convert the inner value
            value_to_json(inner)?
        }
    })
}

/// Convert `serde_json::Value` to `fast_yaml_core::Value`
fn json_to_value(json: &serde_json::Value) -> Result<Value> {
    use Value as YValue;
    use fast_yaml_core::Map;
    use fast_yaml_core::value::ScalarOwned;
    use serde_json::Value as JValue;

    Ok(match json {
        JValue::Null => YValue::Value(ScalarOwned::Null),
        JValue::Bool(b) => YValue::Value(ScalarOwned::Boolean(*b)),
        JValue::Number(n) => {
            use saphyr_parser::ScalarStyle;
            // With the `arbitrary_precision` serde_json feature, `as_str()` returns the
            // original JSON token (e.g. "1.0", "1.23e10", "42"). Use it to distinguish
            // floats (contain '.' or 'e'/'E') from integers so that `1.0` is preserved
            // as a floating-point YAML scalar rather than being coerced to integer `1`.
            let raw = n.as_str();
            let is_float = raw.contains('.') || raw.contains('e') || raw.contains('E');
            if is_float {
                // Validate the value is representable, then store the original JSON token
                // as a plain scalar so the YAML output preserves the float notation.
                let _ = n.as_f64().ok_or_else(|| {
                    anyhow::anyhow!("Float value out of representable range: {n}")
                })?;
                YValue::Representation(raw.to_string(), ScalarStyle::Plain, None)
            } else if let Some(i) = n.as_i64() {
                YValue::Value(ScalarOwned::Integer(i))
            } else {
                YValue::Representation(raw.to_string(), ScalarStyle::Plain, None)
            }
        }
        JValue::String(s) => YValue::Value(ScalarOwned::String(s.clone())),
        JValue::Array(arr) => {
            let yaml_arr: Result<Vec<_>> = arr.iter().map(json_to_value).collect();
            YValue::Sequence(yaml_arr?)
        }
        JValue::Object(map) => {
            let mut yaml_map = Map::new();
            for (k, v) in map {
                yaml_map.insert(
                    YValue::Value(ScalarOwned::String(k.clone())),
                    json_to_value(v)?,
                );
            }
            YValue::Mapping(yaml_map)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::input::InputOrigin;
    use saphyr_parser::ScalarStyle;

    fn parse_json_number(raw: &str) -> Value {
        json_to_value(&serde_json::from_str(raw).unwrap()).unwrap()
    }

    #[test]
    fn json_integers_beyond_i64_keep_exact_digits() {
        for raw in [
            "9223372036854775808",
            "18446744073709551615",
            "-9223372036854775809",
            "123456789012345678901234567890",
        ] {
            assert_eq!(
                parse_json_number(raw),
                Value::Representation(raw.to_string(), ScalarStyle::Plain, None)
            );
        }
    }

    #[test]
    fn json_integers_within_i64_stay_integers() {
        assert_eq!(
            parse_json_number("9223372036854775807"),
            Value::Value(fast_yaml_core::ScalarOwned::Integer(i64::MAX))
        );
        assert_eq!(
            parse_json_number("-9223372036854775808"),
            Value::Value(fast_yaml_core::ScalarOwned::Integer(i64::MIN))
        );
    }

    #[test]
    fn big_int_representation_becomes_json_number() {
        for (text, expected) in [
            ("+99999999999999999999", "99999999999999999999"),
            ("-99999999999999999999", "-99999999999999999999"),
            (
                "000000000000000000000123456789012345678901",
                "123456789012345678901",
            ),
        ] {
            let value = Value::Representation(text.to_string(), ScalarStyle::Plain, None);
            let json = value_to_json(&value).unwrap();
            assert_eq!(serde_json::to_string(&json).unwrap(), expected);
        }
    }

    #[test]
    fn non_big_int_representation_stays_json_string() {
        let over_cap = format!("0x1{}", "0".repeat(3571));
        for text in ["1.5", "abc", over_cap.as_str()] {
            let value = Value::Representation(text.to_string(), ScalarStyle::Plain, None);
            assert_eq!(
                value_to_json(&value).unwrap(),
                serde_json::Value::String(text.to_string())
            );
        }
    }

    #[test]
    fn radix_big_int_representation_becomes_json_number() {
        let value = Value::Representation(
            "0xFFFFFFFFFFFFFFFFFFFF".to_string(),
            ScalarStyle::Plain,
            None,
        );
        assert_eq!(
            value_to_json(&value).unwrap().to_string(),
            "1208925819614629174706175"
        );
    }

    #[test]
    fn quoted_big_int_text_stays_json_string() {
        let value = Value::Representation(
            "9223372036854775808".to_string(),
            ScalarStyle::DoubleQuoted,
            None,
        );
        assert_eq!(
            value_to_json(&value).unwrap(),
            serde_json::Value::String("9223372036854775808".to_string())
        );
    }

    #[test]
    fn test_yaml_to_json() {
        let input = InputSource {
            content: "name: test\nvalue: 123".to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().join("output.json");
        let output = OutputWriter::from_args(Some(temp_path.clone()), false, None).unwrap();

        let cmd = ConvertCommand::new(ConvertFormat::Json, true, ParseLimits::default());
        let result = cmd.execute(&input, &output);
        if let Err(e) = &result {
            eprintln!("Execute error: {e}");
        }
        assert!(result.is_ok());

        let json_str = std::fs::read_to_string(&temp_path)
            .unwrap_or_else(|e| panic!("Failed to read {temp_path:?}: {e}"));
        assert!(!json_str.is_empty(), "Output file is empty!");
        let json: serde_json::Value = serde_json::from_str(&json_str)
            .unwrap_or_else(|e| panic!("Failed to parse JSON from '{json_str}': {e}"));
        assert_eq!(json["name"], "test");
        assert_eq!(json["value"], 123);
    }

    #[test]
    fn test_yaml_to_json_resolves_merge_keys() {
        let input = InputSource {
            content: "b: &b {x: 1, y: 2}\nm:\n  k: 0\n  <<: *b\n  y: 9\n".to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().join("output.json");
        let output = OutputWriter::from_args(Some(temp_path.clone()), false, None).unwrap();

        let cmd = ConvertCommand::new(ConvertFormat::Json, false, ParseLimits::default());
        cmd.execute(&input, &output).unwrap();

        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&temp_path).unwrap()).unwrap();
        assert_eq!(json["m"], serde_json::json!({"x": 1, "y": 9, "k": 0}));
    }

    #[test]
    fn test_json_to_yaml() {
        let input = InputSource {
            content: r#"{"name": "test", "value": 123}"#.to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().join("output.yaml");
        let output = OutputWriter::from_args(Some(temp_path.clone()), false, None).unwrap();

        let cmd = ConvertCommand::new(ConvertFormat::Yaml, true, ParseLimits::default());
        assert!(cmd.execute(&input, &output).is_ok());

        let yaml_str = std::fs::read_to_string(&temp_path).unwrap();
        assert!(yaml_str.contains("name:"));
        assert!(yaml_str.contains("value:"));
    }

    #[test]
    fn test_value_to_json_simple() {
        let yaml = "name: test";
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let json = value_to_json(&value).unwrap();

        assert_eq!(json["name"], "test");
    }

    #[test]
    fn test_json_to_value_simple() {
        let json_str = r#"{"name": "test"}"#;
        let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let yaml = json_to_value(&json).unwrap();

        match yaml {
            Value::Mapping(map) => {
                assert_eq!(map.len(), 1);
            }
            _ => panic!("Expected Mapping"),
        }
    }

    #[test]
    fn test_invalid_yaml_to_json() {
        let input = InputSource {
            content: "invalid: [".to_string(),
            origin: InputOrigin::Stdin,
        };

        let output = OutputWriter::stdout();

        let cmd = ConvertCommand::new(ConvertFormat::Json, true, ParseLimits::default());
        assert!(cmd.execute(&input, &output).is_err());
    }

    #[test]
    fn test_invalid_json_to_yaml() {
        let input = InputSource {
            content: "{invalid json}".to_string(),
            origin: InputOrigin::Stdin,
        };

        let output = OutputWriter::stdout();

        let cmd = ConvertCommand::new(ConvertFormat::Yaml, true, ParseLimits::default());
        assert!(cmd.execute(&input, &output).is_err());
    }

    #[test]
    fn test_multi_document_yaml_to_json() {
        let input = InputSource {
            content: "---\nfoo: 1\n---\nbar: 2\n---\nbaz: 3\n".to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().join("output.json");
        let output = OutputWriter::from_args(Some(temp_path.clone()), false, None).unwrap();

        let cmd = ConvertCommand::new(ConvertFormat::Json, false, ParseLimits::default());
        assert!(cmd.execute(&input, &output).is_ok());

        let json_str = std::fs::read_to_string(&temp_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(json_str.trim()).unwrap();
        assert!(
            json.is_array(),
            "Expected JSON array for multi-document stream"
        );
        let arr = json.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["foo"], 1);
        assert_eq!(arr[1]["bar"], 2);
        assert_eq!(arr[2]["baz"], 3);
    }

    #[test]
    fn test_yaml_inf_nan_to_json_gives_clear_error() {
        for yaml in &["val: .inf", "val: -.inf", "val: .nan"] {
            let input = InputSource {
                content: (*yaml).to_string(),
                origin: InputOrigin::Stdin,
            };
            let output = OutputWriter::stdout();
            let cmd = ConvertCommand::new(ConvertFormat::Json, false, ParseLimits::default());
            let err = cmd.execute(&input, &output).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("cannot be represented in JSON"),
                "expected descriptive error, got: {msg}"
            );
        }
    }

    #[test]
    fn test_json_float_preserves_type() {
        let input = InputSource {
            content: r#"{"whole_float": 1.0, "sci": 1.23e10, "integer": 42}"#.to_string(),
            origin: InputOrigin::Stdin,
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().join("output.yaml");
        let output = OutputWriter::from_args(Some(temp_path.clone()), false, None).unwrap();
        let cmd = ConvertCommand::new(ConvertFormat::Yaml, false, ParseLimits::default());
        assert!(cmd.execute(&input, &output).is_ok());

        let yaml_str = std::fs::read_to_string(&temp_path).unwrap();
        // 1.0 must not become bare integer "1"
        assert!(
            yaml_str.contains("whole_float: 1.0"),
            "expected 'whole_float: 1.0' in: {yaml_str}"
        );
        // integer stays integer
        assert!(
            yaml_str.contains("integer: 42"),
            "expected 'integer: 42' in: {yaml_str}"
        );
    }

    #[test]
    fn test_explicit_int_tag_float_to_json() {
        let input = InputSource {
            content: "val: !!int 3.14".to_string(),
            origin: InputOrigin::Stdin,
        };
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().join("output.json");
        let output = OutputWriter::from_args(Some(temp_path.clone()), false, None).unwrap();
        let cmd = ConvertCommand::new(ConvertFormat::Json, false, ParseLimits::default());
        assert!(cmd.execute(&input, &output).is_ok());
        let json_str = std::fs::read_to_string(&temp_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(json_str.trim()).unwrap();
        assert_eq!(json["val"], 3, "!!int 3.14 should truncate to integer 3");
    }

    #[test]
    fn test_value_to_json_null_key() {
        let yaml = "null: value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let json = value_to_json(&value).unwrap();
        assert_eq!(json["null"], "value");
    }

    #[test]
    fn test_value_to_json_bool_key() {
        let yaml = "true: yes_value";
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let json = value_to_json(&value).unwrap();
        assert_eq!(json["true"], "yes_value");
    }

    #[test]
    fn test_value_to_json_integer_key() {
        let yaml = "42: answer";
        let value = Parser::parse_str(yaml).unwrap().unwrap();
        let json = value_to_json(&value).unwrap();
        assert_eq!(json["42"], "answer");
    }
}
