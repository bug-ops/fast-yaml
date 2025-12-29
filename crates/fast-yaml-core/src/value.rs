pub use saphyr::MappingOwned as Map;
pub use saphyr::ScalarOwned;
/// Wrapper around saphyr's `YamlOwned` type for consistent API.
///
/// This re-exports the saphyr types to provide a stable API
/// that can be extended in the future without breaking changes.
/// We use `YamlOwned` instead of `Yaml` to avoid lifetime parameters.
pub use saphyr::YamlOwned as Value;

/// Re-export `OrderedFloat` for users working with YAML float values.
///
/// This is used internally by saphyr for float comparison in mappings.
pub use ordered_float::OrderedFloat;

/// Type alias for YAML arrays.
pub type Array = Vec<Value>;

#[cfg(test)]
mod tests {
    use super::*;
    use saphyr::ScalarOwned;

    #[test]
    fn test_value_null() {
        let val = Value::Value(ScalarOwned::Null);
        assert!(matches!(val, Value::Value(ScalarOwned::Null)));
    }

    #[test]
    fn test_value_boolean() {
        let val = Value::Value(ScalarOwned::Boolean(true));
        assert!(matches!(val, Value::Value(ScalarOwned::Boolean(true))));
    }

    #[test]
    fn test_value_integer() {
        let val = Value::Value(ScalarOwned::Integer(42));
        assert!(matches!(val, Value::Value(ScalarOwned::Integer(42))));
    }

    #[test]
    fn test_value_string() {
        let val = Value::Value(ScalarOwned::String("test".to_string()));
        assert!(matches!(val, Value::Value(ScalarOwned::String(_))));
    }
}
