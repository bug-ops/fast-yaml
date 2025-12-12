use crate::error::ParseResult;
use crate::value::Value;
use yaml_rust2::YamlLoader;

/// Parser for YAML documents.
///
/// Wraps yaml-rust2's `YamlLoader` to provide a consistent API.
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
        let docs = YamlLoader::load_from_str(input)?;
        Ok(docs.into_iter().next())
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
        Ok(YamlLoader::load_from_str(input)?)
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
