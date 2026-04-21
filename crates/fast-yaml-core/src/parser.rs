use crate::error::ParseResult;
use crate::value::Value;
use saphyr::{ScalarOwned, YamlLoader};
use saphyr_parser::{BufferedInput, Parser as SaphyrParser, ScalarStyle, Tag};

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
        let mut saphyr_parser = SaphyrParser::new(BufferedInput::new(input.chars()));
        let mut loader = YamlLoader::<Value>::default();
        loader.early_parse(false);
        saphyr_parser.load(&mut loader, true)?;
        Ok(loader.into_documents().into_iter().next().map(canonicalize))
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
        let mut saphyr_parser = SaphyrParser::new(BufferedInput::new(input.chars()));
        let mut loader = YamlLoader::<Value>::default();
        loader.early_parse(false);
        saphyr_parser.load(&mut loader, true)?;
        Ok(loader
            .into_documents()
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
/// This function post-processes the tree to:
/// - Resolve `Value::Representation` nodes (produced by `early_parse = false`) to typed scalars,
///   applying explicit YAML core schema tags (`!!int`, `!!float`, `!!bool`, `!!null`, `!!str`)
///   when present (#203).
/// - Handle `True`, `TRUE`, `False`, `FALSE`, `Null` mixed-case variants.
/// - Resolve YAML 1.1 merge keys (`<<: *anchor`) into parent mappings (#204).
pub fn canonicalize(value: Value) -> Value {
    match value {
        Value::Representation(ref s, style, ref tag) => {
            coerce_representation(s, style, tag.as_ref())
        }
        Value::Value(ScalarOwned::String(ref s)) => match s.as_str() {
            "True" | "TRUE" => Value::Value(ScalarOwned::Boolean(true)),
            "False" | "FALSE" => Value::Value(ScalarOwned::Boolean(false)),
            "Null" | "NULL" => Value::Value(ScalarOwned::Null),
            _ => value,
        },
        Value::Tagged(ref tag, ref inner) => coerce_tagged(tag, inner),
        Value::Sequence(seq) => Value::Sequence(seq.into_iter().map(canonicalize).collect()),
        Value::Mapping(map) => {
            let canonicalized: crate::value::Map = map
                .into_iter()
                .map(|(k, v)| (canonicalize(k), canonicalize(v)))
                .collect();
            resolve_merge_keys(canonicalized)
        }
        other => other,
    }
}

/// Parse a YAML core schema integer: decimal, hex (`0x`), or octal (`0o`).
///
/// Returns `None` for values that overflow `i64` or don't match integer syntax.
fn parse_core_schema_int(s: &str) -> Option<i64> {
    let (neg, digits) = s.strip_prefix('-').map_or_else(
        || (false, s.strip_prefix('+').unwrap_or(s)),
        |rest| (true, rest),
    );
    let raw: i64 = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16).ok()?
    } else if let Some(oct) = digits
        .strip_prefix("0o")
        .or_else(|| digits.strip_prefix("0O"))
    {
        i64::from_str_radix(oct, 8).ok()?
    } else {
        digits.parse::<i64>().ok()?
    };
    if neg { raw.checked_neg() } else { Some(raw) }
}

/// Returns `true` if `s` is a decimal integer literal that may exceed `i64` range.
///
/// Matches an optional `+`/`-` sign followed by one or more ASCII digits, with no `.` or `e`.
fn is_integer_literal(s: &str) -> bool {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// Attempt to coerce a float string to `i64` via truncation toward zero (`PyYAML` convention).
///
/// Returns `None` for non-finite values (.nan, .inf) and values outside the `i64` range.
/// Values very close to `i64::MAX` may saturate due to `f64` precision limits — this is a
/// known, benign edge case at the representable boundary.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn float_str_to_int(s: &str) -> Option<i64> {
    parse_core_schema_float(s)
        .filter(|f| f.is_finite() && *f >= i64::MIN as f64 && *f <= i64::MAX as f64)
        .map(|f| f as i64)
}

/// Parse a YAML core schema float, handling special values (.inf, .nan, etc.).
fn parse_core_schema_float(s: &str) -> Option<f64> {
    match s {
        ".inf" | ".Inf" | ".INF" => Some(f64::INFINITY),
        "-.inf" | "-.Inf" | "-.INF" => Some(f64::NEG_INFINITY),
        ".nan" | ".NaN" | ".NAN" => Some(f64::NAN),
        // YAML 1.2 Core Schema float: optional sign, digits, optional fraction, optional exponent.
        // Reject bare words like "infinity" or "nan" that Rust's f64::parse() accepts.
        other => {
            let s = other.strip_prefix(['+', '-']).unwrap_or(other);
            let has_digit_start = s.starts_with(|c: char| c.is_ascii_digit());
            let looks_like_float = has_digit_start
                && s.chars().all(|c| {
                    c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-'
                });
            looks_like_float
                .then(|| other.parse::<f64>().ok())
                .flatten()
        }
    }
}

/// Coerce a `Value::Representation` scalar, applying the tag if present.
///
/// When `early_parse = false`, saphyr preserves the raw string, style, and tag in a
/// `Representation` node. This function resolves that node to a typed `Value::Value`.
fn coerce_representation(s: &str, style: ScalarStyle, tag: Option<&Tag>) -> Value {
    if let Some(tag) = tag.filter(|t| t.is_yaml_core_schema()) {
        let coerced: Option<ScalarOwned> = match tag.suffix.as_str() {
            "int" => s
                .parse::<i64>()
                .ok()
                .or_else(|| float_str_to_int(s))
                .map(ScalarOwned::Integer),
            "float" => parse_core_schema_float(s).map(|f| ScalarOwned::FloatingPoint(f.into())),
            "bool" => s.parse::<bool>().ok().map(ScalarOwned::Boolean),
            "null" => matches!(s, "~" | "null" | "").then_some(ScalarOwned::Null),
            "str" => Some(ScalarOwned::String(s.into())),
            _ => None,
        };
        if let Some(scalar) = coerced {
            return Value::Value(scalar);
        }
    }
    // No tag or unknown tag: non-plain scalars are always strings.
    if style != ScalarStyle::Plain {
        return Value::Value(ScalarOwned::String(s.into()));
    }
    // Plain scalar: apply saphyr's implicit resolution rules.
    let scalar = match s {
        "~" | "null" | "NULL" | "Null" => ScalarOwned::Null,
        "true" | "True" | "TRUE" => ScalarOwned::Boolean(true),
        "false" | "False" | "FALSE" => ScalarOwned::Boolean(false),
        other => parse_core_schema_int(other).map_or_else(
            || {
                if is_integer_literal(other) {
                    ScalarOwned::String(other.into())
                } else {
                    parse_core_schema_float(other).map_or_else(
                        || ScalarOwned::String(other.into()),
                        |f| ScalarOwned::FloatingPoint(f.into()),
                    )
                }
            },
            ScalarOwned::Integer,
        ),
    };
    Value::Value(scalar)
}

/// Coerce a tagged value to the appropriate scalar type based on the YAML core schema tag suffix.
fn coerce_tagged(tag: &Tag, inner: &Value) -> Value {
    if tag.is_yaml_core_schema()
        && let Value::Value(ScalarOwned::String(ref s)) = *inner
    {
        let coerced: Option<ScalarOwned> = match tag.suffix.as_str() {
            "int" => s
                .parse::<i64>()
                .ok()
                .or_else(|| float_str_to_int(s))
                .map(ScalarOwned::Integer),
            "float" => parse_core_schema_float(s).map(|f| ScalarOwned::FloatingPoint(f.into())),
            "bool" => s.parse::<bool>().ok().map(ScalarOwned::Boolean),
            "null" => matches!(s.as_str(), "~" | "null" | "").then_some(ScalarOwned::Null),
            "str" => Some(ScalarOwned::String(s.clone())),
            _ => None,
        };
        if let Some(scalar) = coerced {
            return Value::Value(scalar);
        }
    }
    canonicalize(inner.clone())
}

/// Resolve YAML 1.1 merge keys (`<<`) in a canonicalized mapping.
///
/// Explicit keys always win over merged keys.
fn resolve_merge_keys(map: crate::value::Map) -> Value {
    let merge_key = Value::Value(ScalarOwned::String("<<".into()));
    if !map.contains_key(&merge_key) {
        return Value::Mapping(map);
    }

    let mut result: crate::value::Map = crate::value::Map::new();
    let mut merges: Vec<Value> = Vec::new();

    for (k, v) in map {
        if k == merge_key {
            merges.push(v);
        } else {
            result.insert(k, v);
        }
    }

    for merge_val in merges {
        match merge_val {
            Value::Mapping(merge_map) => {
                for (mk, mv) in merge_map {
                    result.entry(mk).or_insert(mv);
                }
            }
            Value::Sequence(seq) => {
                for item in seq {
                    if let Value::Mapping(merge_map) = item {
                        for (mk, mv) in merge_map {
                            result.entry(mk).or_insert(mv);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Value::Mapping(result)
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

    fn get_mapping_val(yaml: &str, key: &str) -> Value {
        let result = Parser::parse_str(yaml).unwrap().unwrap();
        let Value::Mapping(map) = result else {
            panic!("expected mapping");
        };
        let k = Value::Value(ScalarOwned::String(key.into()));
        map[&k].clone()
    }

    #[test]
    fn test_explicit_tag_int_quoted() {
        let v = get_mapping_val("val: !!int '42'", "val");
        assert!(
            matches!(v, Value::Value(ScalarOwned::Integer(42))),
            "got {v:?}"
        );
    }

    #[test]
    fn test_explicit_tag_float() {
        let v = get_mapping_val("val: !!float '3.14'", "val");
        if let Value::Value(ScalarOwned::FloatingPoint(f)) = v {
            #[allow(clippy::approx_constant)]
            let expected = 3.14_f64;
            assert!((f64::from(f) - expected).abs() < 1e-9);
        } else {
            panic!("expected FloatingPoint, got {v:?}");
        }
    }

    #[test]
    fn test_explicit_tag_bool() {
        let v = get_mapping_val("val: !!bool 'true'", "val");
        assert!(
            matches!(v, Value::Value(ScalarOwned::Boolean(true))),
            "got {v:?}"
        );
    }

    #[test]
    fn test_explicit_tag_null() {
        let v = get_mapping_val("val: !!null ''", "val");
        assert!(matches!(v, Value::Value(ScalarOwned::Null)), "got {v:?}");
    }

    #[test]
    fn test_explicit_tag_str_int() {
        let v = get_mapping_val("val: !!str 42", "val");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(ref s)) if s == "42"),
            "got {v:?}"
        );
    }

    #[test]
    fn test_explicit_tag_int_float_truncation() {
        let v = get_mapping_val("val: !!int 3.14", "val");
        assert!(
            matches!(v, Value::Value(ScalarOwned::Integer(3))),
            "got {v:?}"
        );
    }

    #[test]
    fn test_explicit_tag_int_negative_float() {
        let v = get_mapping_val("val: !!int -2.7", "val");
        assert!(
            matches!(v, Value::Value(ScalarOwned::Integer(-2))),
            "got {v:?}"
        );
    }

    #[test]
    fn test_explicit_tag_int_scientific() {
        let v = get_mapping_val("val: !!int 1.0e2", "val");
        assert!(
            matches!(v, Value::Value(ScalarOwned::Integer(100))),
            "got {v:?}"
        );
    }

    #[test]
    fn test_explicit_tag_int_exact_float() {
        let v = get_mapping_val("val: !!int 3.0", "val");
        assert!(
            matches!(v, Value::Value(ScalarOwned::Integer(3))),
            "got {v:?}"
        );
    }

    #[test]
    fn test_explicit_tag_int_nan_rejected() {
        let v = get_mapping_val("val: !!int .nan", "val");
        assert!(
            !matches!(v, Value::Value(ScalarOwned::Integer(_))),
            "!!int .nan should not produce an integer, got {v:?}"
        );
    }

    #[test]
    fn test_explicit_tag_int_inf_rejected() {
        let v = get_mapping_val("val: !!int .inf", "val");
        assert!(
            !matches!(v, Value::Value(ScalarOwned::Integer(_))),
            "!!int .inf should not produce an integer, got {v:?}"
        );
    }

    #[test]
    fn test_explicit_tag_int_overflow_rejected() {
        let v = get_mapping_val("val: !!int 1.0e20", "val");
        assert!(
            !matches!(v, Value::Value(ScalarOwned::Integer(_))),
            "!!int 1.0e20 should not produce a saturated integer, got {v:?}"
        );
    }

    #[test]
    fn test_merge_key_basic() {
        let yaml = r"
defaults: &defaults
  adapter: postgres
  host: localhost
development:
  <<: *defaults
  database: dev_db
";
        let result = Parser::parse_str(yaml).unwrap().unwrap();
        let Value::Mapping(root) = result else {
            panic!("expected mapping")
        };
        let dev_key = Value::Value(ScalarOwned::String("development".into()));
        let Value::Mapping(dev) = root[&dev_key].clone() else {
            panic!("expected mapping")
        };

        let adapter_key = Value::Value(ScalarOwned::String("adapter".into()));
        let host_key = Value::Value(ScalarOwned::String("host".into()));
        let db_key = Value::Value(ScalarOwned::String("database".into()));

        assert!(dev.contains_key(&adapter_key), "adapter should be merged");
        assert!(dev.contains_key(&host_key), "host should be merged");
        assert!(dev.contains_key(&db_key), "database should be present");
        assert!(
            !dev.contains_key(&Value::Value(ScalarOwned::String("<<".into()))),
            "<< should be removed"
        );
    }

    #[test]
    fn test_merge_key_explicit_wins() {
        let yaml = r"
base: &base
  host: localhost
  port: 5432
override:
  <<: *base
  host: remotehost
";
        let result = Parser::parse_str(yaml).unwrap().unwrap();
        let Value::Mapping(root) = result else {
            panic!("expected mapping")
        };
        let ov_key = Value::Value(ScalarOwned::String("override".into()));
        let Value::Mapping(ov) = root[&ov_key].clone() else {
            panic!("expected mapping")
        };
        let host_key = Value::Value(ScalarOwned::String("host".into()));
        assert!(
            matches!(&ov[&host_key], Value::Value(ScalarOwned::String(s)) if s == "remotehost"),
            "explicit host should win over merged"
        );
    }

    #[test]
    fn test_merge_key_sequence() {
        let yaml = r"
a: &a
  x: 1
b: &b
  y: 2
merged:
  <<: [*a, *b]
  z: 3
";
        let result = Parser::parse_str(yaml).unwrap().unwrap();
        let Value::Mapping(root) = result else {
            panic!("expected mapping")
        };
        let m_key = Value::Value(ScalarOwned::String("merged".into()));
        let Value::Mapping(m) = root[&m_key].clone() else {
            panic!("expected mapping")
        };

        let x = Value::Value(ScalarOwned::String("x".into()));
        let y = Value::Value(ScalarOwned::String("y".into()));
        let z = Value::Value(ScalarOwned::String("z".into()));
        assert!(m.contains_key(&x), "x should be merged from *a");
        assert!(m.contains_key(&y), "y should be merged from *b");
        assert!(m.contains_key(&z), "z should be present");
    }

    #[test]
    fn test_i64_max_boundary() {
        let v = get_mapping_val("x: 9223372036854775807", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::Integer(i64::MAX))),
            "i64::MAX should stay Integer, got {v:?}"
        );

        let v = get_mapping_val("x: 9223372036854775808", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(_))),
            "i64::MAX+1 should become String, got {v:?}"
        );
    }

    #[test]
    fn test_leading_plus_large_integer() {
        let v = get_mapping_val("x: +42", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::Integer(42))),
            "+42 should be Integer(42), got {v:?}"
        );

        let v = get_mapping_val("x: +99999999999999999999", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(_))),
            "+overflow should be String, got {v:?}"
        );
    }

    #[test]
    fn test_large_integer_preserved_as_string() {
        let big =
            "99999999999999999999999999999999999999999999999999999999999999999999999999999999";
        let v = get_mapping_val(&format!("x: {big}"), "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(ref s)) if s == big),
            "got {v:?}"
        );
    }

    #[test]
    fn test_normal_integer_unaffected() {
        let v = get_mapping_val("x: 42", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::Integer(42))),
            "got {v:?}"
        );
    }

    #[test]
    fn test_float_unaffected() {
        let v = get_mapping_val("x: 1.5e10", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::FloatingPoint(_))),
            "got {v:?}"
        );
    }

    #[test]
    fn test_negative_large_integer() {
        let big = "-99999999999999999999999999999999";
        let v = get_mapping_val(&format!("x: {big}"), "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(ref s)) if s == big),
            "got {v:?}"
        );
    }
}
