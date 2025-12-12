/// Wrapper around yaml-rust2's Yaml type for consistent API.
///
/// This re-exports the yaml-rust2 types to provide a stable API
/// that can be extended in the future without breaking changes.
pub use yaml_rust2::Yaml as Value;
pub use yaml_rust2::yaml::Hash as Map;

/// Type alias for YAML arrays.
pub type Array = Vec<Value>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_null() {
        let val = Value::Null;
        assert!(matches!(val, Value::Null));
    }

    #[test]
    fn test_value_boolean() {
        let val = Value::Boolean(true);
        assert!(matches!(val, Value::Boolean(true)));
    }

    #[test]
    fn test_value_integer() {
        let val = Value::Integer(42);
        assert!(matches!(val, Value::Integer(42)));
    }

    #[test]
    fn test_value_string() {
        let val = Value::String("test".to_string());
        assert!(matches!(val, Value::String(_)));
    }
}
