use crate::error::ParseResult;
use crate::limits::{LimitGuard, ParseLimits};
use crate::value::Value;
use saphyr::{ScalarOwned, YamlLoader};
use saphyr_parser::{
    BufferedInput, Parser as SaphyrParser, ScalarStyle, SpannedEventReceiver, Tag,
};

/// Parser for YAML documents.
///
/// Wraps saphyr's YAML loading to provide a consistent API. Every entry point enforces
/// [`ParseLimits`] (nesting depth and alias expansion) before building the tree.
#[derive(Debug)]
pub struct Parser;

impl Parser {
    /// Parse a single YAML document from a string.
    ///
    /// Returns the first document if multiple are present, or None if the input is empty.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Scanner` if the YAML syntax is invalid, or
    /// `ParseError::LimitExceeded` if the input exceeds the default [`ParseLimits`].
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
        Self::parse_str_with_limits(input, &ParseLimits::default())
    }

    /// Parse a single YAML document, enforcing explicit [`ParseLimits`].
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Scanner` if the YAML syntax is invalid, or
    /// `ParseError::LimitExceeded` if the input exceeds `limits`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{ParseError, Parser};
    /// use fast_yaml_core::limits::{MaxDepth, ParseLimits};
    ///
    /// let limits = ParseLimits { max_depth: MaxDepth::new(1), ..ParseLimits::default() };
    /// let err = Parser::parse_str_with_limits("[[1]]", &limits).unwrap_err();
    /// assert!(matches!(err, ParseError::LimitExceeded { .. }));
    /// ```
    pub fn parse_str_with_limits(input: &str, limits: &ParseLimits) -> ParseResult<Option<Value>> {
        let docs = load_documents(input, limits)?;
        Ok(docs.into_iter().next().map(canonicalize))
    }

    /// Parse all YAML documents from a string.
    ///
    /// Returns a vector of all documents found in the input.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Scanner` if the YAML syntax is invalid, or
    /// `ParseError::LimitExceeded` if the input exceeds the default [`ParseLimits`].
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
        Self::parse_all_with_limits(input, &ParseLimits::default())
    }

    /// Parse all YAML documents, enforcing explicit [`ParseLimits`].
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Scanner` if the YAML syntax is invalid, or
    /// `ParseError::LimitExceeded` if the input exceeds `limits`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::Parser;
    /// use fast_yaml_core::limits::{MaxAliasBytes, ParseLimits};
    ///
    /// let limits = ParseLimits { max_alias_bytes: MaxAliasBytes::new(1), ..ParseLimits::default() };
    /// assert!(Parser::parse_all_with_limits("- &a x\n- *a\n- *a", &limits).is_err());
    /// ```
    pub fn parse_all_with_limits(input: &str, limits: &ParseLimits) -> ParseResult<Vec<Value>> {
        Ok(load_documents(input, limits)?
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
    /// Returns `ParseError::Scanner` if the YAML syntax is invalid, or
    /// `ParseError::LimitExceeded` if the input exceeds the default [`ParseLimits`].
    ///
    /// [`parse_all`]: Parser::parse_all
    pub fn parse_all_preserving_styles(input: &str) -> ParseResult<Vec<Value>> {
        load_documents(input, &ParseLimits::default())
    }
}

/// Drives the parser event by event so [`LimitGuard`] can reject input before the loader
/// recurses or clones aliases, then returns the un-canonicalized documents.
fn load_documents(input: &str, limits: &ParseLimits) -> ParseResult<Vec<Value>> {
    let mut parser = SaphyrParser::new(BufferedInput::new(strip_bom(input).chars()));
    let mut loader = YamlLoader::<Value>::default();
    loader.early_parse(false);
    let mut guard = LimitGuard::new(*limits);
    while let Some(event) = parser.next_event() {
        let (event, span) = event?;
        guard.observe(&event, span)?;
        loader.on_event(event, span);
    }
    Ok(inject_implicit_null_if_empty(
        loader.into_documents(),
        input,
    ))
}

/// Strips one leading UTF-8 byte order mark (U+FEFF) from `input`.
///
/// The BOM is an encoding signature, not YAML content (YAML 1.2 §5.2); a BOM in the
/// middle of the text is data and is left untouched.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::strip_bom;
///
/// assert_eq!(strip_bom("\u{FEFF}a: 1"), "a: 1");
/// assert_eq!(strip_bom("a: \u{FEFF}1"), "a: \u{FEFF}1");
/// ```
#[must_use]
pub fn strip_bom(input: &str) -> &str {
    input.strip_prefix('\u{FEFF}').unwrap_or(input)
}

/// Returns `true` when `tag` is the YAML non-specific tag `!`.
///
/// The non-specific tag forces the failsafe schema: scalars resolve to plain strings
/// regardless of their content (YAML 1.2 §6.8.1 / §10.3.2).
fn is_non_specific_tag(tag: &Tag) -> bool {
    tag.handle.is_empty() && tag.suffix == "!"
}

/// Injects one implicit null document when saphyr produces no documents for non-empty input.
///
/// Per YAML 1.2 §9.2, a stream with no explicit documents but non-empty content
/// (comments, bare markers, whitespace) represents one document with an implicit null node.
/// Empty string input stays `[]` to match `safe_load("")` → `None` behaviour.
fn inject_implicit_null_if_empty(docs: Vec<Value>, input: &str) -> Vec<Value> {
    if docs.is_empty() && !input.is_empty() {
        vec![Value::Value(ScalarOwned::Null)]
    } else {
        docs
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
///
/// Recursion depth equals the nesting depth of `value`, which [`ParseLimits`] bounds for
/// parsed input; the collection arms are kept free of scalar temporaries to keep frames small.
pub fn canonicalize(value: Value) -> Value {
    match value {
        Value::Sequence(seq) => canonicalize_sequence(seq),
        Value::Mapping(map) => canonicalize_mapping(map),
        Value::Tagged(tag, inner) => canonicalize_tagged(&tag, inner),
        other => canonicalize_scalar(other),
    }
}

fn canonicalize_sequence(mut seq: Vec<Value>) -> Value {
    for item in &mut seq {
        *item = canonicalize(std::mem::replace(item, Value::Value(ScalarOwned::Null)));
    }
    Value::Sequence(seq)
}

fn canonicalize_mapping(map: crate::value::Map) -> Value {
    let mut canonicalized = crate::value::Map::with_capacity(map.len());
    for (k, v) in map {
        canonicalized.insert(canonicalize(k), canonicalize(v));
    }
    resolve_merge_keys(canonicalized)
}

// A match, not `map_or_else`: closure frames would cost stack on every tagged level.
#[allow(clippy::option_if_let_else)]
fn canonicalize_tagged(tag: &Tag, inner: Box<Value>) -> Value {
    match coerce_tagged_scalar(tag, &inner) {
        Some(coerced) => coerced,
        None => canonicalize(*inner),
    }
}

/// Canonicalize a non-collection, non-tagged node.
fn canonicalize_scalar(value: Value) -> Value {
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

/// Returns `true` if `s` is an integer literal (decimal, hex, or octal) that may exceed `i64` range.
///
/// Matches optional `+`/`-` sign followed by `0x`/`0X` + hex digits, `0o`/`0O` + octal digits,
/// or plain ASCII decimal digits.
fn is_integer_literal(s: &str) -> bool {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        return !oct.is_empty() && oct.bytes().all(|b| matches!(b, b'0'..=b'7'));
    }
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
    // 1. Core-schema explicit tag (!!str, !!int, !!float, !!bool, !!null).
    if let Some(tag) = tag.filter(|t| t.is_yaml_core_schema()) {
        let coerced: Option<ScalarOwned> = match tag.suffix.as_str() {
            "int" => parse_core_schema_int(s)
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
    // 2. Non-specific tag `!`: failsafe schema forces string (YAML 1.2 §6.8.1 / §10.3.2).
    if tag.is_some_and(is_non_specific_tag) {
        return Value::Value(ScalarOwned::String(s.into()));
    }
    // 3. No tag or unknown tag: non-plain scalars are always strings.
    if style != ScalarStyle::Plain {
        return Value::Value(ScalarOwned::String(s.into()));
    }
    // 4. Empty plain scalar with no tag: implicit null (YAML 1.2 §10.3.2, bare `---`).
    if s.is_empty() {
        return Value::Value(ScalarOwned::Null);
    }
    // 5. Plain scalar: apply saphyr's implicit resolution rules.
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

/// Coerce a core-schema-tagged string scalar by tag suffix; `None` when not applicable.
fn coerce_tagged_scalar(tag: &Tag, inner: &Value) -> Option<Value> {
    if !tag.is_yaml_core_schema() {
        return None;
    }
    let Value::Value(ScalarOwned::String(s)) = inner else {
        return None;
    };
    let coerced = match tag.suffix.as_str() {
        "int" => parse_core_schema_int(s)
            .or_else(|| float_str_to_int(s))
            .map(ScalarOwned::Integer),
        "float" => parse_core_schema_float(s).map(|f| ScalarOwned::FloatingPoint(f.into())),
        "bool" => s.parse::<bool>().ok().map(ScalarOwned::Boolean),
        "null" => matches!(s.as_str(), "~" | "null" | "").then_some(ScalarOwned::Null),
        "str" => Some(ScalarOwned::String(s.clone())),
        _ => None,
    };
    coerced.map(Value::Value)
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

    #[test]
    fn test_hex_overflow_preserved_as_string() {
        let v = get_mapping_val("x: 0x8000000000000000", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(_))),
            "hex overflow should be String, got {v:?}"
        );
    }

    #[test]
    fn test_hex_max_i64_preserved_as_integer() {
        // 0x7FFFFFFFFFFFFFFF == i64::MAX == 9223372036854775807
        let v = get_mapping_val("x: 0x7FFFFFFFFFFFFFFF", "x");
        assert!(
            matches!(
                v,
                Value::Value(ScalarOwned::Integer(9_223_372_036_854_775_807))
            ),
            "0x7FFFFFFFFFFFFFFF should be Integer(i64::MAX), got {v:?}"
        );
    }

    #[test]
    fn test_octal_overflow_preserved_as_string() {
        let v = get_mapping_val("x: 0o1000000000000000000000", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(_))),
            "octal overflow should be String, got {v:?}"
        );
    }

    #[test]
    fn test_octal_max_fitting_preserved_as_integer() {
        // 0o777777777777777777777 == i64::MAX == 9223372036854775807
        let v = get_mapping_val("x: 0o777777777777777777777", "x");
        assert!(
            matches!(
                v,
                Value::Value(ScalarOwned::Integer(9_223_372_036_854_775_807))
            ),
            "0o777777777777777777777 should be Integer(i64::MAX), got {v:?}"
        );
    }

    #[test]
    fn test_tagged_int_hex_fits_i64() {
        let v = get_mapping_val("x: !!int 0xFF", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::Integer(255))),
            "!!int 0xFF should be Integer(255), got {v:?}"
        );
    }

    #[test]
    fn test_tagged_int_hex_overflow_preserved_as_string() {
        let v = get_mapping_val("x: !!int 0x8000000000000000", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(_))),
            "!!int hex overflow should be String, got {v:?}"
        );
    }

    #[test]
    fn test_negative_hex_overflow_preserved_as_string() {
        // -0x8000000000000000 == i64::MIN, which fits; -0x8000000000000001 overflows.
        // Both produce String because parse_core_schema_int does not handle sign + 0x prefix —
        // consistent with the decimal path where signed hex is not a YAML 1.2 core schema form.
        let v = get_mapping_val("x: -0x8000000000000001", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(_))),
            "negative hex overflow should be String, got {v:?}"
        );
    }

    #[test]
    fn test_tagged_int_octal_overflow_preserved_as_string() {
        let v = get_mapping_val("x: !!int 0o1000000000000000000000", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(_))),
            "!!int octal overflow should be String, got {v:?}"
        );
    }

    #[test]
    fn test_uppercase_prefix_hex_overflow_preserved_as_string() {
        let v = get_mapping_val("x: 0XDEADBEEFDEADBEEF", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(_))),
            "0X uppercase prefix overflow should be String, got {v:?}"
        );
    }

    #[test]
    fn test_uppercase_prefix_octal_overflow_preserved_as_string() {
        let v = get_mapping_val("x: 0O1000000000000000000000", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(_))),
            "0O uppercase prefix overflow should be String, got {v:?}"
        );
    }

    // --- #235: empty/comment-only/bare-marker streams yield one null doc ---

    #[test]
    fn test_empty_string_yields_empty_vec() {
        let docs = Parser::parse_all("").unwrap();
        assert!(docs.is_empty(), "empty string must stay []");
    }

    #[test]
    fn test_whitespace_only_yields_null_doc() {
        let docs = Parser::parse_all("   ").unwrap();
        assert_eq!(docs.len(), 1);
        assert!(matches!(docs[0], Value::Value(ScalarOwned::Null)));
    }

    #[test]
    fn test_comment_only_yields_null_doc() {
        let docs = Parser::parse_all("# comment").unwrap();
        assert_eq!(docs.len(), 1);
        assert!(matches!(docs[0], Value::Value(ScalarOwned::Null)));
    }

    #[test]
    fn test_bare_doc_end_yields_null_doc() {
        let docs = Parser::parse_all("...").unwrap();
        assert_eq!(docs.len(), 1);
        assert!(matches!(docs[0], Value::Value(ScalarOwned::Null)));
    }

    #[test]
    fn test_comment_then_doc_end_yields_null_doc() {
        let docs = Parser::parse_all("# c\n...").unwrap();
        assert_eq!(docs.len(), 1);
        assert!(matches!(docs[0], Value::Value(ScalarOwned::Null)));
    }

    #[test]
    fn test_bare_doc_start_yields_null_doc() {
        let docs = Parser::parse_all("---").unwrap();
        assert_eq!(docs.len(), 1);
        assert!(matches!(docs[0], Value::Value(ScalarOwned::Null)));
    }

    #[test]
    fn test_parse_str_comment_only_returns_null() {
        let result = Parser::parse_str("# comment").unwrap();
        assert!(matches!(result, Some(Value::Value(ScalarOwned::Null))));
    }

    #[test]
    fn test_parse_str_empty_unchanged() {
        let result = Parser::parse_str("").unwrap();
        assert!(result.is_none(), "empty string must still return None");
    }

    #[test]
    fn test_bom_only_yields_one_doc() {
        // BOM-only: saphyr processes the BOM and returns one document (empty Null scalar).
        // inject_implicit_null_if_empty is not needed here — saphyr handles it.
        // Document: BOM-only → 1 doc (saphyr behaviour, not injected).
        let docs = Parser::parse_all("\u{FEFF}").unwrap();
        assert_eq!(docs.len(), 1, "BOM-only should yield exactly one document");
    }

    // --- #238: non-specific tag `!` forces string (failsafe schema) ---

    #[test]
    fn test_non_specific_tag_plain_integer_is_string() {
        let v = get_mapping_val("x: ! 99", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(ref s)) if s == "99"),
            "! 99 should be String(\"99\"), got {v:?}"
        );
    }

    #[test]
    fn test_non_specific_tag_quoted_is_string() {
        let v = get_mapping_val("x: ! \"99\"", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(ref s)) if s == "99"),
            "! \"99\" should be String(\"99\"), got {v:?}"
        );
    }

    #[test]
    fn test_non_specific_tag_true_is_string() {
        let v = get_mapping_val("x: ! true", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(ref s)) if s == "true"),
            "! true should be String(\"true\"), got {v:?}"
        );
    }

    #[test]
    fn test_non_specific_tag_null_keyword_is_string() {
        let v = get_mapping_val("x: ! null", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(ref s)) if s == "null"),
            "! null should be String(\"null\"), got {v:?}"
        );
    }

    #[test]
    fn test_non_specific_tag_empty_is_string_not_null() {
        // `! ''` must be String(""), NOT Null. Order of branches is load-bearing.
        let v = get_mapping_val("x: ! ''", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(ref s)) if s.is_empty()),
            "! '' should be String(\"\") not Null, got {v:?}"
        );
    }

    #[test]
    fn test_non_specific_tag_on_sequence_is_noop() {
        // Non-specific tag on collection: failsafe seq = plain seq.
        let yaml = "x: ! [1, 2]";
        let result = Parser::parse_str(yaml).unwrap().unwrap();
        let Value::Mapping(map) = result else {
            panic!("expected mapping")
        };
        let k = Value::Value(ScalarOwned::String("x".into()));
        let val = &map[&k];
        assert!(
            matches!(val, Value::Sequence(_)),
            "! on sequence must stay Sequence, got {val:?}"
        );
    }

    // --- #235 round-trip: format(parse("# c")) freezes new expected output ---

    #[test]
    fn test_round_trip_comment_only() {
        use crate::emitter::Emitter;
        let docs = Parser::parse_all_preserving_styles("# comment").unwrap();
        assert_eq!(docs.len(), 1, "should have one null doc");
        // The null doc formats to "null\n" or "~\n" — freeze whatever the emitter produces.
        let formatted = Emitter::emit_all(&docs).unwrap();
        assert!(
            !formatted.is_empty(),
            "formatted output must be non-empty, got: {formatted:?}"
        );
        // Null should not format as empty string.
        assert_ne!(formatted.trim(), "", "null doc must not format to empty");
    }

    #[test]
    fn test_bom_before_comment_and_mapping_parses() {
        let value = Parser::parse_str("\u{FEFF}# c\na: 1").unwrap().unwrap();
        let Value::Mapping(map) = value else {
            panic!("expected mapping, got {value:?}");
        };
        assert!(
            map.keys()
                .any(|k| matches!(k, Value::Value(ScalarOwned::String(s)) if s == "a"))
        );
    }

    #[test]
    fn test_bom_not_part_of_first_key() {
        let docs = Parser::parse_all("\u{FEFF}a: 1").unwrap();
        let Value::Mapping(map) = &docs[0] else {
            panic!("expected mapping");
        };
        assert!(
            map.keys()
                .any(|k| matches!(k, Value::Value(ScalarOwned::String(s)) if s == "a"))
        );
    }

    #[test]
    fn test_bom_preserving_styles_parses() {
        let docs = Parser::parse_all_preserving_styles("\u{FEFF}# c\na: 1").unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_mid_text_bom_stays_data() {
        let v = get_mapping_val("b: \u{FEFF}x", "b");
        assert!(matches!(v, Value::Value(ScalarOwned::String(ref s)) if s == "\u{FEFF}x"));
    }

    #[test]
    fn test_strip_bom_strips_single_leading_bom_only() {
        assert_eq!(strip_bom("\u{FEFF}\u{FEFF}a"), "\u{FEFF}a");
        assert_eq!(strip_bom("a"), "a");
        assert_eq!(strip_bom(""), "");
    }

    #[test]
    fn test_bom_crlf_parses() {
        let v = get_mapping_val("\u{FEFF}# c\r\na: 1\r\n", "a");
        assert!(matches!(v, Value::Value(ScalarOwned::Integer(1))));
    }

    #[test]
    fn test_bom_multi_document() {
        let docs = Parser::parse_all("\u{FEFF}---\na: 1\n---\nb: 2\n").unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_bom_only_parse_str_is_null() {
        let v = Parser::parse_str("\u{FEFF}").unwrap();
        assert!(matches!(v, Some(Value::Value(ScalarOwned::Null))));
    }

    mod limits {
        use super::*;
        use crate::error::ParseError;
        use crate::limits::{LimitKind, MaxAliasBytes, MaxDepth, NODE_BYTES};
        use std::fmt::Write as _;

        const BOMB: &str = "a0: &a0 [x,x,x,x,x,x,x,x,x]\n\
            a1: &a1 [*a0,*a0,*a0,*a0,*a0,*a0,*a0,*a0,*a0]\n\
            a2: &a2 [*a1,*a1,*a1,*a1,*a1,*a1,*a1,*a1,*a1]\n\
            a3: &a3 [*a2,*a2,*a2,*a2,*a2,*a2,*a2,*a2,*a2]\n\
            a4: &a4 [*a3,*a3,*a3,*a3,*a3,*a3,*a3,*a3,*a3]\n\
            a5: &a5 [*a4,*a4,*a4,*a4,*a4,*a4,*a4,*a4,*a4]\n\
            a6: &a6 [*a5,*a5,*a5,*a5,*a5,*a5,*a5,*a5,*a5]\n\
            a7: &a7 [*a6,*a6,*a6,*a6,*a6,*a6,*a6,*a6,*a6]\n\
            a8: &a8 [*a7,*a7,*a7,*a7,*a7,*a7,*a7,*a7,*a7]\n";

        fn depth_limits(depth: usize) -> ParseLimits {
            ParseLimits {
                max_depth: MaxDepth::new(depth),
                ..ParseLimits::default()
            }
        }

        fn alias_limits(bytes: usize) -> ParseLimits {
            ParseLimits {
                max_alias_bytes: MaxAliasBytes::new(bytes),
                ..ParseLimits::default()
            }
        }

        #[test]
        fn deep_block_sequence_is_rejected_on_line_one() {
            let input = format!("{}x", "- ".repeat(20_000));
            match Parser::parse_str(&input) {
                Err(ParseError::LimitExceeded {
                    kind: LimitKind::Depth(_),
                    line,
                    ..
                }) => assert_eq!(line, 1),
                other => panic!("expected depth limit, got {other:?}"),
            }
        }

        #[test]
        fn deep_indented_mapping_is_rejected() {
            let mut input = String::new();
            for i in 0..1000 {
                writeln!(input, "{}k:", " ".repeat(i)).unwrap();
            }
            assert!(matches!(
                Parser::parse_str(&input),
                Err(ParseError::LimitExceeded {
                    kind: LimitKind::Depth(_),
                    ..
                })
            ));
        }

        #[test]
        fn depth_boundary_is_inclusive() {
            let limits = depth_limits(3);
            assert!(Parser::parse_str_with_limits("[[[1]]]", &limits).is_ok());
            assert!(matches!(
                Parser::parse_str_with_limits("[[[[1]]]]", &limits),
                Err(ParseError::LimitExceeded {
                    kind: LimitKind::Depth(_),
                    ..
                })
            ));
        }

        fn parse_and_drop_on_512k_stack(input: String) {
            std::thread::Builder::new()
                .stack_size(512 * 1024)
                .spawn(move || {
                    for value in Parser::parse_all(&input).unwrap() {
                        drop(value);
                    }
                    drop(Parser::parse_all_preserving_styles(&input).unwrap());
                })
                .unwrap()
                .join()
                .unwrap();
        }

        #[test]
        fn default_depth_parses_and_drops_on_small_stack() {
            parse_and_drop_on_512k_stack(format!("{}x", "- ".repeat(MaxDepth::DEFAULT.get())));
        }

        #[test]
        fn tagged_depth_parses_and_drops_on_small_stack() {
            let depth = 250;
            parse_and_drop_on_512k_stack(format!("{}1{}", "!t [".repeat(depth), "]".repeat(depth)));
        }

        fn str_bomb(scalar_len: usize, levels: usize) -> String {
            let mut yaml = format!("a0: &a0 \"{}\"\n", "x".repeat(scalar_len));
            for i in 1..=levels {
                writeln!(
                    yaml,
                    "a{i}: &a{i} [{}]",
                    vec![format!("*a{}", i - 1); 9].join(",")
                )
                .unwrap();
            }
            yaml
        }

        #[test]
        fn long_scalar_bombs_are_rejected() {
            for input in [str_bomb(10_240, 5), str_bomb(1_024, 6)] {
                assert!(matches!(
                    Parser::parse_all(&input),
                    Err(ParseError::LimitExceeded {
                        kind: LimitKind::AliasBytes(_),
                        ..
                    })
                ));
            }
        }

        #[test]
        fn long_tag_bombs_are_rejected() {
            let mut input = format!("a0: &a0 !<tag:{}> \"\"\n", "x".repeat(10_000));
            for i in 1..=5 {
                writeln!(
                    input,
                    "a{i}: &a{i} [{}]",
                    vec![format!("*a{}", i - 1); 9].join(",")
                )
                .unwrap();
            }
            assert!(matches!(
                Parser::parse_all(&input),
                Err(ParseError::LimitExceeded {
                    kind: LimitKind::AliasBytes(_),
                    ..
                })
            ));
        }

        #[test]
        fn shared_scalar_aliases_within_budget_load() {
            let input = "a: &x [1, 2, 3]\nb: *x\nc: &s \"text\"\nd: *s\n";
            assert_eq!(Parser::parse_all(input).unwrap().len(), 1);
        }

        #[test]
        fn tagged_alias_amplification_is_rejected() {
            let mut input = format!("a0: &a0 !t [{}]\n", "\"x\",".repeat(100));
            for i in 1..=8 {
                writeln!(
                    input,
                    "a{i}: &a{i} !t [{}]",
                    vec![format!("*a{}", i - 1); 9].join(",")
                )
                .unwrap();
            }
            assert!(matches!(
                Parser::parse_all(&input),
                Err(ParseError::LimitExceeded { .. })
            ));
        }

        #[test]
        fn anchor_id_reuse_across_documents_is_allowed() {
            let docs = Parser::parse_all("- &a [x]\n---\n- &a [y]\n- *a\n").unwrap();
            assert_eq!(docs.len(), 2);
        }

        #[test]
        fn alias_budget_is_cumulative_per_stream() {
            let doc = |name: &str| format!("- &{name} [x]\n- *{name}\n- *{name}\n- *{name}\n");
            let limits = alias_limits(400);
            assert!(Parser::parse_all_with_limits(&doc("a"), &limits).is_ok());
            let stream = format!("{}---\n{}", doc("a"), doc("b"));
            assert!(matches!(
                Parser::parse_all_with_limits(&stream, &limits),
                Err(ParseError::LimitExceeded {
                    kind: LimitKind::AliasBytes(_),
                    ..
                })
            ));
        }

        #[test]
        fn aliased_empty_collections_cost_one_node() {
            let limits = alias_limits(3 * NODE_BYTES);
            assert!(Parser::parse_all_with_limits("- &a []\n- *a\n- *a\n- *a\n", &limits).is_ok());
            assert!(
                Parser::parse_all_with_limits("- &a {}\n- *a\n- *a\n- *a\n- *a\n", &limits)
                    .is_err()
            );
        }

        #[test]
        fn alias_depth_boundary_is_inclusive() {
            let limits = depth_limits(3);
            assert!(Parser::parse_all_with_limits("- &a [[1]]\n- *a\n", &limits).is_ok());
            assert!(matches!(
                Parser::parse_all_with_limits("- &a [[1]]\n- [*a]\n", &limits),
                Err(ParseError::LimitExceeded {
                    kind: LimitKind::Depth(_),
                    ..
                })
            ));
        }

        #[test]
        fn alias_as_mapping_key_is_accounted() {
            let input = "- &k [x]\n- *k : v\n";
            assert!(Parser::parse_all_with_limits(input, &depth_limits(2)).is_err());
            assert!(Parser::parse_all_with_limits(input, &alias_limits(100)).is_err());
            assert!(Parser::parse_all(input).is_ok());
        }

        #[test]
        fn bom_prefixed_deep_input_is_rejected() {
            let input = format!("\u{FEFF}{}x", "- ".repeat(300));
            assert!(matches!(
                Parser::parse_all(&input),
                Err(ParseError::LimitExceeded {
                    kind: LimitKind::Depth(_),
                    ..
                })
            ));
        }

        #[test]
        fn bomb_is_rejected_by_every_entry_point() {
            let is_alias_limit = |r: ParseResult<()>| {
                matches!(
                    r,
                    Err(ParseError::LimitExceeded {
                        kind: LimitKind::AliasBytes(_),
                        ..
                    })
                )
            };
            assert!(is_alias_limit(Parser::parse_str(BOMB).map(drop)));
            assert!(is_alias_limit(Parser::parse_all(BOMB).map(drop)));
            assert!(is_alias_limit(
                Parser::parse_all_preserving_styles(BOMB).map(drop)
            ));
        }

        #[test]
        fn anchor_chain_depth_amplification_is_rejected() {
            let nest = |inner: &str| format!("{}{inner}{}", "[".repeat(200), "]".repeat(200));
            let mut input = format!("a0: &a0 {}\n", nest("1"));
            for i in 1..4 {
                writeln!(input, "a{i}: &a{i} {}", nest(&format!("*a{}", i - 1))).unwrap();
            }
            assert!(matches!(
                Parser::parse_str(&input),
                Err(ParseError::LimitExceeded {
                    kind: LimitKind::Depth(_),
                    ..
                })
            ));
        }

        #[test]
        fn alias_budget_boundary_is_inclusive() {
            let limits = alias_limits(400);
            let three = "- &a [x]\n- *a\n- *a\n- *a\n";
            let four = "- &a [x]\n- *a\n- *a\n- *a\n- *a\n";
            assert!(Parser::parse_all_with_limits(three, &limits).is_ok());
            assert!(matches!(
                Parser::parse_all_with_limits(four, &limits),
                Err(ParseError::LimitExceeded {
                    kind: LimitKind::AliasBytes(_),
                    ..
                })
            ));
        }

        #[test]
        fn cross_document_alias_is_unknown_anchor() {
            let err = Parser::parse_all("--- &a [x]\n--- *a\n").unwrap_err();
            assert!(matches!(err, ParseError::Scanner(_)), "{err:?}");
            assert!(err.to_string().contains("unknown anchor"));
        }

        #[test]
        fn self_referential_alias_does_not_panic() {
            assert!(Parser::parse_all("&a [*a]").is_ok());
        }

        #[test]
        fn display_reports_position() {
            let err = Parser::parse_str_with_limits("[\n [[1]]]", &depth_limits(2)).unwrap_err();
            let msg = err.to_string();
            assert!(msg.contains("limit exceeded"), "{msg}");
            assert!(msg.contains("line 2"), "{msg}");
            assert!(msg.contains("column 3"), "{msg}");
        }
    }
}
