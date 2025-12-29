use anyhow::{Context, Result};
use fast_yaml_core::{Emitter, Parser, Value};
use serde_json;

use crate::cli::ConvertFormat;
use crate::io::{InputSource, OutputWriter};

/// Convert command implementation
pub struct ConvertCommand {
    target_format: ConvertFormat,
    pretty: bool,
}

impl ConvertCommand {
    pub const fn new(target_format: ConvertFormat, pretty: bool) -> Self {
        Self {
            target_format,
            pretty,
        }
    }

    /// Execute convert command
    pub fn execute(&self, input: &InputSource, output: &OutputWriter) -> Result<()> {
        match self.target_format {
            ConvertFormat::Json => self.yaml_to_json(input, output),
            ConvertFormat::Yaml => self.json_to_yaml(input, output),
        }
    }

    /// Convert YAML to JSON
    fn yaml_to_json(&self, input: &InputSource, output: &OutputWriter) -> Result<()> {
        // Parse YAML
        let value = Parser::parse_str(input.as_str())
            .context("Failed to parse YAML")?
            .ok_or_else(|| anyhow::anyhow!("Empty YAML document"))?;

        // Convert to serde_json::Value
        let json_value = value_to_json(&value)?;

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
    #[allow(clippy::unused_self)]
    fn json_to_yaml(&self, input: &InputSource, output: &OutputWriter) -> Result<()> {
        // Parse JSON
        let json_value: serde_json::Value =
            serde_json::from_str(input.as_str()).context("Failed to parse JSON")?;

        // Convert to YAML Value
        let yaml_value = json_to_value(&json_value)?;

        // Emit YAML
        let yaml_string = Emitter::emit_str(&yaml_value).context("Failed to emit YAML")?;

        // Write output
        output.write(&yaml_string)?;

        Ok(())
    }
}

/// Convert `fast_yaml_core::Value` to `serde_json::Value`
fn value_to_json(value: &Value) -> Result<serde_json::Value> {
    use Value as YValue;
    use serde_json::Value as JValue;

    Ok(match value {
        YValue::Null => JValue::Null,
        YValue::Boolean(b) => JValue::Bool(*b),
        YValue::Integer(i) => JValue::Number((*i).into()),
        YValue::Real(s) => {
            let f: f64 = s.parse().context("Failed to parse float")?;
            serde_json::Number::from_f64(f)
                .map(JValue::Number)
                .ok_or_else(|| anyhow::anyhow!("Invalid float value: {s}"))?
        }
        YValue::String(s) => JValue::String(s.clone()),
        YValue::Array(arr) => {
            let json_arr: Result<Vec<_>> = arr.iter().map(value_to_json).collect();
            JValue::Array(json_arr?)
        }
        YValue::Hash(map) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                let key = k
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Map key must be a string"))?;
                json_map.insert(key.to_string(), value_to_json(v)?);
            }
            JValue::Object(json_map)
        }
        YValue::Alias(_) => {
            anyhow::bail!("YAML aliases are not supported in JSON conversion");
        }
        YValue::BadValue => {
            anyhow::bail!("Invalid YAML value encountered");
        }
    })
}

/// Convert `serde_json::Value` to `fast_yaml_core::Value`
fn json_to_value(json: &serde_json::Value) -> Result<Value> {
    use Value as YValue;
    use fast_yaml_core::Map;
    use serde_json::Value as JValue;

    Ok(match json {
        JValue::Null => YValue::Null,
        JValue::Bool(b) => YValue::Boolean(*b),
        JValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                YValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                YValue::Real(f.to_string())
            } else {
                anyhow::bail!("Unsupported number type: {n}");
            }
        }
        JValue::String(s) => YValue::String(s.clone()),
        JValue::Array(arr) => {
            let yaml_arr: Result<Vec<_>> = arr.iter().map(json_to_value).collect();
            YValue::Array(yaml_arr?)
        }
        JValue::Object(map) => {
            let mut yaml_map = Map::new();
            for (k, v) in map {
                yaml_map.insert(YValue::String(k.clone()), json_to_value(v)?);
            }
            YValue::Hash(yaml_map)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::input::InputOrigin;

    #[test]
    fn test_yaml_to_json() {
        let input = InputSource {
            content: "name: test\nvalue: 123".to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().join("output.json");
        let output = OutputWriter::from_args(Some(temp_path.clone()), false, None).unwrap();

        let cmd = ConvertCommand::new(ConvertFormat::Json, true);
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
    fn test_json_to_yaml() {
        let input = InputSource {
            content: r#"{"name": "test", "value": 123}"#.to_string(),
            origin: InputOrigin::Stdin,
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path().join("output.yaml");
        let output = OutputWriter::from_args(Some(temp_path.clone()), false, None).unwrap();

        let cmd = ConvertCommand::new(ConvertFormat::Yaml, true);
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
            Value::Hash(map) => {
                assert_eq!(map.len(), 1);
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_invalid_yaml_to_json() {
        let input = InputSource {
            content: "invalid: [".to_string(),
            origin: InputOrigin::Stdin,
        };

        let output = OutputWriter::stdout();

        let cmd = ConvertCommand::new(ConvertFormat::Json, true);
        assert!(cmd.execute(&input, &output).is_err());
    }

    #[test]
    fn test_invalid_json_to_yaml() {
        let input = InputSource {
            content: "{invalid json}".to_string(),
            origin: InputOrigin::Stdin,
        };

        let output = OutputWriter::stdout();

        let cmd = ConvertCommand::new(ConvertFormat::Yaml, true);
        assert!(cmd.execute(&input, &output).is_err());
    }
}
