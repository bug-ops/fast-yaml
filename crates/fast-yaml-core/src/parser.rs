use crate::error::ParseResult;
use crate::value::Value;
use saphyr::{LoadableYamlNode, ScalarOwned, YamlLoader};
use saphyr_parser::{BufferedInput, Parser as SaphyrParser};

/// Parser for YAML documents.
///
/// Wraps saphyr's YAML loading to provide a consistent API.
#[derive(Debug)]
pub struct Parser;

impl Parser {
    /// Parse a single YAML document from a string.
    ///
    /// Returns the first document if multiple are present, or None if the input is empty.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Scanner` if the YAML syntax is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::Parser;
    ///
    /// let result = Parser::parse_str("name: test\nvalue: 123")?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn parse_str(input: &str) -> ParseResult<Option<Value>> {
        let docs = Value::load_from_str(input)?;
        Ok(docs.into_iter().next().map(canonicalize))
    }

    /// Parse all YAML documents from a string.
    ///
    /// Returns a vector of all documents found in the input.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Scanner` if the YAML syntax is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::Parser;
    ///
    /// let docs = Parser::parse_all("---\nfoo: 1\n---\nbar: 2")?;
    /// assert_eq!(docs.len(), 2);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn parse_all(input: &str) -> ParseResult<Vec<Value>> {
        Ok(Value::load_from_str(input)?
            .into_iter()
            .map(canonicalize)
            .collect())
    }

    /// Parse all YAML documents preserving scalar styles (literal `|`, folded `>`).
    ///
    /// Unlike [`parse_all`], this function uses `early_parse = false` in the loader,
    /// which keeps scalars as `Value::Representation` nodes with their original style
    /// information instead of resolving them eagerly.
    ///
    /// This is used by the format pipeline to preserve block scalar styles in output.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Scanner` if the YAML syntax is invalid.
    ///
    /// [`parse_all`]: Parser::parse_all
    pub fn parse_all_preserving_styles(input: &str) -> ParseResult<Vec<Value>> {
        let mut saphyr_parser = SaphyrParser::new(BufferedInput::new(input.chars()));
        let mut loader = YamlLoader::<Value>::default();
        loader.early_parse(false);
        saphyr_parser.load(&mut loader, true)?;
        Ok(loader.into_documents())
    }
}

/// Canonicalize mixed-case YAML 1.2.2 bool/null variants that saphyr leaves as strings.
///
/// saphyr handles lowercase `true`, `false`, `null`, `~` natively.
/// This function post-processes the tree to handle `True`, `TRUE`, `False`, `FALSE`, `Null`.
pub fn canonicalize(value: Value) -> Value {
    match value {
        Value::Value(ScalarOwned::String(ref s)) => match s.as_str() {
            "True" | "TRUE" => Value::Value(ScalarOwned::Boolean(true)),
            "False" | "FALSE" => Value::Value(ScalarOwned::Boolean(false)),
            "Null" | "NULL" => Value::Value(ScalarOwned::Null),
            _ => value,
        },
        Value::Sequence(seq) => Value::Sequence(seq.into_iter().map(canonicalize).collect()),
        Value::Mapping(map) => Value::Mapping(
            map.into_iter()
                .map(|(k, v)| (canonicalize(k), canonicalize(v)))
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_str_simple() {
        let result = Parser::parse_str("name: test\nvalue: 123").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_str_empty() {
        let result = Parser::parse_str("").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_all_multiple_docs() {
        let docs = Parser::parse_all("---\nfoo: 1\n---\nbar: 2").unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_yaml12_bool_true_variants() {
        use saphyr::ScalarOwned;
        for variant in &["True", "TRUE"] {
            let result = Parser::parse_str(&format!("val: {variant}"))
                .unwrap()
                .unwrap();
            if let Value::Mapping(map) = result {
                let v = map.values().next().unwrap();
                assert!(
                    matches!(v, Value::Value(ScalarOwned::Boolean(true))),
                    "{variant} should be Bool(true)"
                );
            } else {
                panic!("expected mapping");
            }
        }
    }

    #[test]
    fn test_yaml12_bool_false_variants() {
        use saphyr::ScalarOwned;
        for variant in &["False", "FALSE"] {
            let result = Parser::parse_str(&format!("val: {variant}"))
                .unwrap()
                .unwrap();
            if let Value::Mapping(map) = result {
                let v = map.values().next().unwrap();
                assert!(
                    matches!(v, Value::Value(ScalarOwned::Boolean(false))),
                    "{variant} should be Bool(false)"
                );
            } else {
                panic!("expected mapping");
            }
        }
    }

    #[test]
    fn test_yaml12_null_variant() {
        use saphyr::ScalarOwned;
        let result = Parser::parse_str("val: Null").unwrap().unwrap();
        if let Value::Mapping(map) = result {
            let v = map.values().next().unwrap();
            assert!(
                matches!(v, Value::Value(ScalarOwned::Null)),
                "Null should be Null"
            );
        } else {
            panic!("expected mapping");
        }
    }

    #[test]
    fn test_parse_str_invalid() {
        let result = Parser::parse_str("invalid: [\n  missing: bracket");
        assert!(result.is_err());
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
        let result = Parser::parse_str(yaml).unwrap();
        assert!(result.is_some());
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
        let result = Parser::parse_str(yaml).unwrap();
        assert!(result.is_some());
    }
}
