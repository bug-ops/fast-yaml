use crate::error::{EmitError, EmitResult};
use crate::value::Value;
use yaml_rust2::YamlEmitter;

/// Emitter for YAML documents.
///
/// Wraps yaml-rust2's `YamlEmitter` to provide a consistent API.
#[derive(Debug)]
pub struct Emitter;

impl Emitter {
    /// Emit a single YAML document to a string.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Emit` if the value cannot be serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, Value};
    ///
    /// let value = Value::String("test".to_string());
    /// let yaml = Emitter::emit_str(&value)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn emit_str(value: &Value) -> EmitResult<String> {
        let mut output = String::new();
        {
            let mut emitter = YamlEmitter::new(&mut output);
            emitter
                .dump(value)
                .map_err(|e| EmitError::Emit(e.to_string()))?;
        }

        // Remove the leading "---\n" that yaml-rust2 adds
        if output.starts_with("---") {
            output = output
                .trim_start_matches("---")
                .trim_start_matches('\n')
                .to_string();
        }

        Ok(output)
    }

    /// Emit multiple YAML documents to a string with document separators.
    ///
    /// # Errors
    ///
    /// Returns `EmitError::Emit` if any value cannot be serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Emitter, Value};
    ///
    /// let docs = vec![
    ///     Value::String("first".to_string()),
    ///     Value::String("second".to_string()),
    /// ];
    /// let yaml = Emitter::emit_all(&docs)?;
    /// assert!(yaml.contains("---"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn emit_all(values: &[Value]) -> EmitResult<String> {
        let mut output = String::new();

        for (i, value) in values.iter().enumerate() {
            if i > 0 {
                output.push_str("---\n");
            }

            let mut doc_output = String::new();
            {
                let mut emitter = YamlEmitter::new(&mut doc_output);
                emitter
                    .dump(value)
                    .map_err(|e| EmitError::Emit(e.to_string()))?;
            }

            // Remove the leading "---\n" that yaml-rust2 adds
            let doc_output = doc_output
                .trim_start_matches("---")
                .trim_start_matches('\n');
            output.push_str(doc_output);
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emit_str_string() {
        let value = Value::String("test".to_string());
        let result = Emitter::emit_str(&value).unwrap();
        assert!(result.contains("test"));
    }

    #[test]
    fn test_emit_str_integer() {
        let value = Value::Integer(42);
        let result = Emitter::emit_str(&value).unwrap();
        assert!(result.contains("42"));
    }

    #[test]
    fn test_emit_all_multiple() {
        let values = vec![
            Value::String("first".to_string()),
            Value::String("second".to_string()),
        ];
        let result = Emitter::emit_all(&values).unwrap();
        assert!(result.contains("first"));
        assert!(result.contains("second"));
        assert!(result.contains("---"));
    }

    #[test]
    fn test_emit_all_single() {
        let values = vec![Value::String("only".to_string())];
        let result = Emitter::emit_all(&values).unwrap();
        assert!(result.contains("only"));
        assert!(!result.starts_with("---"));
    }
}
