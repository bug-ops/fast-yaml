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

/// Returns the text of a scalar used as a mapping key in string-keyed formats (JSON, JS objects).
///
/// Null, booleans, integers and floats use their canonical text; a `Representation` uses its
/// stored text, which [`crate::canonicalize`] makes canonical decimal for integers beyond `i64`.
/// Returns `None` for collections, aliases and other non-scalar nodes.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{Parser, Value, value::scalar_key_text};
///
/// let Some(Value::Mapping(map)) = Parser::parse_str("+0x8000000000000000: x").unwrap() else {
///     unreachable!()
/// };
/// let key = map.keys().next().unwrap();
/// assert_eq!(scalar_key_text(key).as_deref(), Some("9223372036854775808"));
/// ```
#[must_use]
pub fn scalar_key_text(key: &Value) -> Option<String> {
    match key {
        Value::Value(scalar) => Some(match scalar {
            ScalarOwned::Null => "null".to_string(),
            ScalarOwned::Boolean(b) => b.to_string(),
            ScalarOwned::Integer(i) => i.to_string(),
            ScalarOwned::FloatingPoint(f) => f.to_string(),
            ScalarOwned::String(s) => s.clone(),
        }),
        Value::Representation(s, ..) => Some(s.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saphyr::ScalarOwned;
    use saphyr_parser::ScalarStyle;

    #[test]
    fn scalar_key_text_of_parsed_big_int_is_canonical() {
        for (raw, expected) in [
            ("+99999999999999999999", "99999999999999999999"),
            ("-99999999999999999999", "-99999999999999999999"),
            (
                "000000000000000000000123456789012345678901",
                "123456789012345678901",
            ),
            ("0xFFFFFFFFFFFFFFFFFF", "4722366482869645213695"),
        ] {
            let Some(Value::Mapping(map)) = crate::Parser::parse_str(&format!("{raw}: x")).unwrap()
            else {
                unreachable!()
            };
            let key = map.keys().next().unwrap();
            assert_eq!(scalar_key_text(key).as_deref(), Some(expected), "{raw}");
        }
    }

    #[test]
    fn scalar_key_text_keeps_quoted_big_int_text_raw() {
        let key = Value::Representation(
            "+99999999999999999999".to_string(),
            ScalarStyle::DoubleQuoted,
            None,
        );
        assert_eq!(
            scalar_key_text(&key).as_deref(),
            Some("+99999999999999999999")
        );
    }

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
