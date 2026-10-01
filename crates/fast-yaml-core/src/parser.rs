use crate::error::{ParseError, ParseResult, SourcePosition, SyntaxError};
use crate::input::NormalizedInput;
use crate::limits::{LimitGuard, ParseLimits, StreamBudget};
use crate::merge::{MergeError, NodeRole, merge_into};
use crate::merge_check::MergeKeyValidator;
use crate::scalar::{ResolvedScalar, core_tag_suffix, resolve_scalar};
use crate::value::{Mapping, Value};
use saphyr_parser::{Event, Parser as SaphyrParser, Span};
use std::collections::HashMap;

/// Parser for YAML documents.
///
/// Loads YAML into the resolved [`Value`] model. Every entry point enforces [`ParseLimits`]
/// (nesting depth and alias expansion) before building the tree.
#[derive(Debug)]
pub struct Parser;

impl Parser {
    /// Parse a single YAML document from a string.
    ///
    /// Returns the first document if multiple are present, or None if the input is empty.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Syntax` if the YAML syntax is invalid, or
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
    /// Returns the first document, but every document is validated, so an invalid merge key in
    /// a later document is an error just as in [`Parser::parse_all_with_limits`].
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Syntax` if the YAML syntax is invalid, `ParseError::Merge` if any
    /// document has an invalid `<<` value, or `ParseError::LimitExceeded` if the input exceeds
    /// `limits`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{ParseError, Parser};
    /// use fast_yaml_core::limits::{MaxDepth, ParseLimits};
    ///
    /// let limits = ParseLimits { max_depth: MaxDepth::new(1).unwrap(), ..ParseLimits::default() };
    /// let err = Parser::parse_str_with_limits("[[1]]", &limits).unwrap_err();
    /// assert!(matches!(err, ParseError::LimitExceeded { .. }));
    /// ```
    pub fn parse_str_with_limits(input: &str, limits: &ParseLimits) -> ParseResult<Option<Value>> {
        let docs = Self::parse_all_with_budget(input, &StreamBudget::new(*limits))?;
        Ok(docs.into_iter().next())
    }

    /// Parse all YAML documents from a string.
    ///
    /// Returns a vector of all documents found in the input.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Syntax` if the YAML syntax is invalid, or
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
    /// Returns `ParseError::Syntax` if the YAML syntax is invalid, or
    /// `ParseError::LimitExceeded` if the input exceeds `limits`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::Parser;
    /// use fast_yaml_core::limits::{MaxAliasBytes, ParseLimits};
    ///
    /// let limits = ParseLimits { max_alias_bytes: MaxAliasBytes::new(1).unwrap(), ..ParseLimits::default() };
    /// assert!(Parser::parse_all_with_limits("- &a x\n- *a\n- *a", &limits).is_err());
    /// ```
    pub fn parse_all_with_limits(input: &str, limits: &ParseLimits) -> ParseResult<Vec<Value>> {
        Self::parse_all_with_budget(input, &StreamBudget::new(*limits))
    }

    /// Parse all YAML documents, drawing alias bytes from a caller-owned [`StreamBudget`].
    ///
    /// Use one budget across several inputs (for example chunks of one stream) so that the
    /// alias limit applies to their total rather than to each separately.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Syntax` if the YAML syntax is invalid, or
    /// `ParseError::LimitExceeded` if the input exceeds the budget's limits.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::Parser;
    /// use fast_yaml_core::limits::{ParseLimits, StreamBudget};
    ///
    /// let budget = StreamBudget::new(ParseLimits::default());
    /// let docs = Parser::parse_all_with_budget("a: 1\n---\nb: 2", &budget)?;
    /// assert_eq!(docs.len(), 2);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn parse_all_with_budget(input: &str, budget: &StreamBudget) -> ParseResult<Vec<Value>> {
        Self::parse_normalized(&NormalizedInput::new(input)?, budget)
    }

    /// Parse all YAML documents of an already normalized input.
    ///
    /// The other entry points normalize their `&str` first; use this one when the caller needs
    /// the [`NormalizedInput`] itself, for example to map offsets back to the original text or
    /// to parse the slices of one stream (see [`NormalizedInput::slice`]).
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Syntax` if the YAML syntax is invalid, or
    /// `ParseError::LimitExceeded` if the input exceeds the budget's limits.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{NormalizedInput, Parser};
    /// use fast_yaml_core::limits::{ParseLimits, StreamBudget};
    ///
    /// let budget = StreamBudget::new(ParseLimits::default());
    /// let input = NormalizedInput::new("a: 1\n---\nb: 2\n")?;
    /// let second = input.slice(5..input.as_str().len()).unwrap();
    /// assert_eq!(Parser::parse_normalized(&second, &budget)?.len(), 1);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn parse_normalized(
        input: &NormalizedInput<'_>,
        budget: &StreamBudget,
    ) -> ParseResult<Vec<Value>> {
        load_documents_with_budget(input, budget)
    }
}

/// Drives the parser event by event so [`LimitGuard`] can reject input before the builder
/// clones aliases, then returns the loaded documents.
fn load_documents_with_budget(
    input: &NormalizedInput<'_>,
    budget: &StreamBudget,
) -> ParseResult<Vec<Value>> {
    // StrInput is required: BufferedInput loops forever on a directive name at EOF (#403)
    let mut parser = SaphyrParser::new_from_str(input.as_str());
    let mut guard = LimitGuard::with_budget(budget.clone());
    let mut merge_keys = MergeKeyValidator::default();
    let mut builder = Builder::default();
    while let Some(event) = parser.next_event() {
        let (event, span) = event.map_err(|error| ParseError::scanner(&error, guard.document()))?;
        guard.observe(&event, span)?;
        let role = merge_keys.observe(&event, span)?;
        builder.event(event, span, role)?;
    }
    Ok(inject_implicit_null_if_empty(
        builder.documents,
        input.original_len(),
    ))
}

/// Strips one leading UTF-8 byte order mark (U+FEFF) from `input`.
///
/// For text that is not YAML, such as JSON. YAML entry points use
/// [`NormalizedInput`], which also strips the BOMs of later document
/// prefixes.
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

/// Injects one implicit null document when the parser produces no documents for non-empty input.
///
/// Per YAML 1.2 §9.2, a stream with no explicit documents but non-empty content
/// (comments, bare markers, whitespace) represents one document with an implicit null node.
/// Empty string input stays `[]` to match `safe_load("")` → `None` behaviour.
fn inject_implicit_null_if_empty(docs: Vec<Value>, original_len: usize) -> Vec<Value> {
    if docs.is_empty() && original_len > 0 {
        vec![Value::Null]
    } else {
        docs
    }
}

/// What the builder expects next inside an open mapping.
enum Slot {
    Key,
    Value(Value),
    MergeValue,
}

struct MappingFrame {
    set: bool,
    entries: Mapping,
    merge: Option<Value>,
    slot: Slot,
}

enum Body {
    Sequence(Vec<Value>),
    Mapping(MappingFrame),
}

struct Frame {
    anchor: usize,
    role: NodeRole,
    body: Body,
}

/// Iterative tree builder: a heap stack instead of recursion, so nesting depth cannot overflow
/// the call stack. Merge values are validated by `MergeKeyValidator` before they reach it.
#[derive(Default)]
struct Builder {
    stack: Vec<Frame>,
    anchors: HashMap<usize, Value>,
    root: Option<Value>,
    documents: Vec<Value>,
}

impl Builder {
    fn event(&mut self, event: Event<'_>, span: Span, role: Option<NodeRole>) -> ParseResult<()> {
        let role = role.unwrap_or(NodeRole::Root);
        match event {
            Event::DocumentStart(_) => {
                self.anchors.clear();
                self.root = None;
            }
            Event::DocumentEnd => {
                self.documents.push(self.root.take().unwrap_or(Value::Null));
            }
            Event::Scalar(text, style, anchor, tag) => {
                let value = match resolve_scalar(&text, style, tag.as_deref()) {
                    ResolvedScalar::Str(_) => Value::String(text.into_owned()),
                    other => Value::from(other),
                };
                self.deliver(value, anchor, role);
            }
            Event::Alias(id) => {
                let value = self.anchors.get(&id).cloned().ok_or_else(|| {
                    ParseError::Syntax(SyntaxError::recursive_alias(
                        span.into(),
                        self.documents.len(),
                    ))
                })?;
                self.deliver(value, 0, role);
            }
            Event::SequenceStart(anchor, _) => self.stack.push(Frame {
                anchor,
                role,
                body: Body::Sequence(Vec::new()),
            }),
            Event::MappingStart(anchor, tag) => self.stack.push(Frame {
                anchor,
                role,
                body: Body::Mapping(MappingFrame {
                    set: tag.as_deref().and_then(core_tag_suffix) == Some("set"),
                    entries: Mapping::new(),
                    merge: None,
                    slot: Slot::Key,
                }),
            }),
            Event::SequenceEnd | Event::MappingEnd => {
                if let Some(frame) = self.stack.pop() {
                    let value = self.finish(frame.body, span)?;
                    self.deliver(value, frame.anchor, frame.role);
                }
            }
            Event::Nothing | Event::StreamStart | Event::StreamEnd => {}
        }
        Ok(())
    }

    fn finish(&self, body: Body, span: Span) -> ParseResult<Value> {
        Ok(match body {
            Body::Sequence(items) => Value::Sequence(items),
            Body::Mapping(MappingFrame {
                set: true, entries, ..
            }) => Value::Set(entries.into_iter().map(|(member, _)| member).collect()),
            Body::Mapping(MappingFrame {
                entries,
                merge: None,
                ..
            }) => Value::Mapping(entries),
            Body::Mapping(MappingFrame {
                entries,
                merge: Some(merge),
                ..
            }) => {
                let mut merged = Mapping::with_capacity(entries.len());
                merge_into(&mut merged, Some(merge), entries)
                    .map_err(|error| self.merge_error(error, span))?;
                Value::Mapping(merged)
            }
        })
    }

    // Unreachable for parser-loaded input, which `MergeKeyValidator` has already checked.
    fn merge_error(&self, error: MergeError, span: Span) -> ParseError {
        let SourcePosition { line, column } = span.into();
        ParseError::Merge {
            error,
            line,
            column,
            document: self.documents.len(),
        }
    }

    fn deliver(&mut self, value: Value, anchor: usize, role: NodeRole) {
        if anchor > 0 {
            self.anchors.insert(anchor, value.clone());
        }
        match self.stack.last_mut().map(|frame| &mut frame.body) {
            None => self.root = Some(value),
            Some(Body::Sequence(items)) => items.push(value),
            Some(Body::Mapping(frame)) => match std::mem::replace(&mut frame.slot, Slot::Key) {
                Slot::Key if role == NodeRole::MergeKey => {
                    frame.slot = Slot::MergeValue;
                }
                Slot::Key => frame.slot = Slot::Value(value),
                Slot::Value(key) => {
                    frame.entries.insert(key, value);
                }
                Slot::MergeValue => frame.merge = Some(value),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::BigInt;

    #[test]
    fn unterminated_directive_errors_instead_of_hanging() {
        for input in [
            "%",
            "%YAML",
            "%TAG",
            "%FOO",
            "\n%",
            "a\n...\n%",
            "---\n%",
            "a: 1\n%",
            "\u{feff}%",
            "a\n...\n%FOO bar",
        ] {
            assert!(Parser::parse_all(input).is_err(), "{input:?}");
            assert!(Parser::parse_str(input).is_err(), "{input:?}");
        }
    }

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
                    matches!(v, Value::Bool(true)),
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
                    matches!(v, Value::Bool(false)),
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
            assert!(matches!(v, Value::Null), "Null should be Null");
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

    fn big_int(text: &str) -> Value {
        Value::BigInt(BigInt::parse(text).unwrap())
    }

    fn radix_text_of(v: &Value) -> Option<&str> {
        let Value::BigInt(b) = v else {
            panic!("big int expected, got {v:?}");
        };
        b.radix_text()
    }

    fn get_mapping_val(yaml: &str, key: &str) -> Value {
        let result = Parser::parse_str(yaml).unwrap().unwrap();
        let Value::Mapping(map) = result else {
            panic!("expected mapping");
        };
        let k = Value::String(key.into());
        map[&k].clone()
    }

    #[test]
    fn test_explicit_tag_int_quoted() {
        let v = get_mapping_val("val: !!int '42'", "val");
        assert!(matches!(v, Value::Int(42)), "got {v:?}");
    }

    #[test]
    fn test_explicit_tag_float() {
        let v = get_mapping_val("val: !!float '3.14'", "val");
        if let Value::Float(f) = v {
            #[allow(clippy::approx_constant)]
            let expected = 3.14_f64;
            assert!((f.get() - expected).abs() < 1e-9);
        } else {
            panic!("expected FloatingPoint, got {v:?}");
        }
    }

    #[test]
    fn test_explicit_tag_bool() {
        let v = get_mapping_val("val: !!bool 'true'", "val");
        assert!(matches!(v, Value::Bool(true)), "got {v:?}");
    }

    #[test]
    fn test_explicit_tag_null() {
        let v = get_mapping_val("val: !!null ''", "val");
        assert!(matches!(v, Value::Null), "got {v:?}");
    }

    #[test]
    fn test_explicit_tag_str_int() {
        let v = get_mapping_val("val: !!str 42", "val");
        assert!(matches!(v, Value::String(ref s) if s == "42"), "got {v:?}");
    }

    #[test]
    fn test_explicit_tag_int_float_truncation() {
        let v = get_mapping_val("val: !!int 3.14", "val");
        assert!(matches!(v, Value::Int(3)), "got {v:?}");
    }

    #[test]
    fn test_explicit_tag_int_negative_float() {
        let v = get_mapping_val("val: !!int -2.7", "val");
        assert!(matches!(v, Value::Int(-2)), "got {v:?}");
    }

    #[test]
    fn test_explicit_tag_int_scientific() {
        let v = get_mapping_val("val: !!int 1.0e2", "val");
        assert!(matches!(v, Value::Int(100)), "got {v:?}");
    }

    #[test]
    fn test_explicit_tag_int_exact_float() {
        let v = get_mapping_val("val: !!int 3.0", "val");
        assert!(matches!(v, Value::Int(3)), "got {v:?}");
    }

    #[test]
    fn test_explicit_tag_int_nan_rejected() {
        let v = get_mapping_val("val: !!int .nan", "val");
        assert!(
            !matches!(v, Value::Int(_)),
            "!!int .nan should not produce an integer, got {v:?}"
        );
    }

    #[test]
    fn test_explicit_tag_int_inf_rejected() {
        let v = get_mapping_val("val: !!int .inf", "val");
        assert!(
            !matches!(v, Value::Int(_)),
            "!!int .inf should not produce an integer, got {v:?}"
        );
    }

    #[test]
    fn test_explicit_tag_int_overflow_rejected() {
        let v = get_mapping_val("val: !!int 1.0e20", "val");
        assert!(
            !matches!(v, Value::Int(_)),
            "!!int 1.0e20 should not produce a saturated integer, got {v:?}"
        );
    }

    #[test]
    fn parse_str_validates_merge_keys_in_every_document() {
        for yaml in [
            "x: 1\n---\nm:\n  <<: [1]",
            "x: 1\n---\ny: 2\n---\nm:\n  <<: 1",
        ] {
            assert!(
                matches!(Parser::parse_str(yaml), Err(ParseError::Merge { .. })),
                "{yaml}"
            );
            assert!(Parser::parse_all(yaml).is_err(), "{yaml}");
        }
    }

    #[test]
    fn parse_str_accepts_valid_merge_keys_in_later_documents() {
        let yaml = "x: 1\n---\nb: &b {y: 2}\nm:\n  <<: *b\n---\nz: 3";
        assert_eq!(
            Parser::parse_str(yaml).unwrap(),
            Parser::parse_all(yaml).unwrap().into_iter().next()
        );
        let Some(Value::Mapping(first)) = Parser::parse_str(yaml).unwrap() else {
            panic!("expected mapping")
        };
        assert_eq!(first.len(), 1);
    }

    #[test]
    fn parse_str_with_limits_enforces_limits_in_last_document() {
        let limits = ParseLimits {
            max_depth: crate::limits::MaxDepth::new(2).unwrap(),
            ..ParseLimits::default()
        };
        let err =
            Parser::parse_str_with_limits("a: 1\n---\nb: 2\n---\n[[[1]]]\n", &limits).unwrap_err();
        assert!(matches!(err, ParseError::LimitExceeded { .. }));
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
        let dev_key = Value::String("development".into());
        let Value::Mapping(dev) = root[&dev_key].clone() else {
            panic!("expected mapping")
        };

        let adapter_key = Value::String("adapter".into());
        let host_key = Value::String("host".into());
        let db_key = Value::String("database".into());

        assert!(dev.contains_key(&adapter_key), "adapter should be merged");
        assert!(dev.contains_key(&host_key), "host should be merged");
        assert!(dev.contains_key(&db_key), "database should be present");
        assert!(
            !dev.contains_key(&Value::String("<<".into())),
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
        let ov_key = Value::String("override".into());
        let Value::Mapping(ov) = root[&ov_key].clone() else {
            panic!("expected mapping")
        };
        let host_key = Value::String("host".into());
        assert!(
            matches!(&ov[&host_key], Value::String(s) if s == "remotehost"),
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
        assert_eq!(merged_entries(yaml, "merged"), ["x: 1", "y: 2", "z: 3"]);
    }

    fn sub_mapping(doc: &Value, key: &str) -> Mapping {
        let Value::Mapping(root) = doc else {
            panic!("expected mapping")
        };
        match root[&Value::String(key.into())].clone() {
            Value::Mapping(m) => m,
            Value::Set(set) => set
                .into_iter()
                .map(|member| (member, Value::Null))
                .collect(),
            other => panic!("expected mapping or set, got {other:?}"),
        }
    }

    fn entry_texts(m: &Mapping) -> Vec<String> {
        m.iter()
            .map(|(k, v)| format!("{}: {}", scalar_text(k), scalar_text(v)))
            .collect()
    }

    fn merged_entries(yaml: &str, key: &str) -> Vec<String> {
        entry_texts(&sub_mapping(
            &Parser::parse_str(yaml).unwrap().unwrap(),
            key,
        ))
    }

    fn scalar_text(v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Int(i) => i.to_string(),
            Value::Null => "null".to_owned(),
            other => format!("{other:?}"),
        }
    }

    #[test]
    fn test_merge_key_order_merged_first() {
        let yaml = "b: &b {x: 1, y: 2}
m:
  k: 0
  <<: *b
  y: 9
";
        assert_eq!(merged_entries(yaml, "m"), ["x: 1", "y: 9", "k: 0"]);
        let yaml = "b: &b {x: 1, y: 2}
m:
  <<: *b
  k: 0
  y: 9
";
        assert_eq!(merged_entries(yaml, "m"), ["x: 1", "y: 9", "k: 0"]);
    }

    #[test]
    fn test_merge_key_sequence_earlier_wins_forward_order() {
        let yaml = "a: &a {x: 1, p: A}
b: &b {y: 2, p: B}
m:
  <<: [*a, *b]
  k: 0
";
        assert_eq!(merged_entries(yaml, "m"), ["x: 1", "p: A", "y: 2", "k: 0"]);
    }

    #[test]
    fn test_merge_key_nested_anchor_and_inline() {
        let yaml = "a: &a {x: 1}
b: &b
  <<: *a
  y: 2
m:
  <<: *b
  z: 3
";
        assert_eq!(merged_entries(yaml, "m"), ["x: 1", "y: 2", "z: 3"]);
        assert_eq!(
            merged_entries(
                "m:
  <<: {x: 1}
  y: 2
",
                "m"
            ),
            ["x: 1", "y: 2"]
        );
    }

    #[test]
    fn test_merge_key_repeated_last_wins() {
        let yaml = "a: &a {x: 1}
b: &b {y: 2}
m:
  <<: *a
  <<: *b
";
        assert_eq!(merged_entries(yaml, "m"), ["y: 2"]);
    }

    #[test]
    fn test_merge_key_non_mapping_is_rejected() {
        for merge in [
            "1",
            "null",
            "[1]",
            "[[{x: 1}]]",
            "text",
            "[{x: 1}, 5]",
            "true",
        ] {
            let yaml = format!("m:\n  <<: {merge}\n  k: 0\n");
            assert!(
                matches!(
                    Parser::parse_str(&yaml),
                    Err(ParseError::Merge {
                        error: MergeError::NotMapping,
                        ..
                    })
                ),
                "{merge}"
            );
        }
    }

    #[test]
    fn test_merge_key_flow_mapping_not_first() {
        let yaml = "b: &b {x: 1, y: 2}\nm: {k: 0, <<: *b, y: 9}\n";
        assert_eq!(merged_entries(yaml, "m"), ["x: 1", "y: 9", "k: 0"]);
    }

    #[test]
    fn test_quoted_and_tagged_merge_key_is_ordinary() {
        for key in ["'<<'", "\"<<\"", "!!str <<", "! <<"] {
            let yaml = format!("b: &b {{x: 1}}\nm:\n  {key}: *b\n  k: 0\n");
            let m = sub_mapping(&Parser::parse_str(&yaml).unwrap().unwrap(), "m");
            assert_eq!(entry_texts(&m).len(), 2, "{key}");
            assert_eq!(scalar_text(m.keys().next().unwrap()), "<<", "{key}");
            assert!(
                matches!(m.values().next(), Some(Value::Mapping(_))),
                "{key}"
            );
        }
    }

    #[test]
    fn test_quoted_merge_key_with_non_mapping_value_is_kept() {
        assert_eq!(
            merged_entries("m:\n  '<<': 1\n  k: 0\n", "m"),
            ["<<: 1", "k: 0"]
        );
    }

    #[test]
    fn test_plain_and_quoted_merge_keys_coexist() {
        let yaml = "b: &b {x: 1}\nm:\n  <<: *b\n  '<<': 2\n";
        assert_eq!(merged_entries(yaml, "m"), ["x: 1", "<<: 2"]);
    }

    #[test]
    fn test_json_merge_key_is_not_merged() {
        let yaml = crate::Emitter::emit_str(
            &Parser::parse_str(r#"{"m": {"<<": {"admin": true}, "k": 0}}"#)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let m = sub_mapping(&Parser::parse_str(&yaml).unwrap().unwrap(), "m");
        assert_eq!(scalar_text(m.keys().next().unwrap()), "<<");
        assert!(matches!(m.values().next(), Some(Value::Mapping(_))));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn test_merge_key_empty_source() {
        assert_eq!(merged_entries("m:\n  <<: {}\n  k: 0\n", "m"), ["k: 0"]);
    }

    #[test]
    fn test_merge_key_mixed_sequence() {
        let yaml = "a: &a {x: 1}\nm:\n  <<: [*a, 5, null, [{w: 0}], {z: 3}]\n  k: 0\n";
        assert!(matches!(
            Parser::parse_str(yaml),
            Err(ParseError::Merge {
                error: MergeError::NotMapping,
                ..
            })
        ));
    }

    fn merge_error(yaml: &str) -> Option<MergeError> {
        match Parser::parse_str(yaml) {
            Err(ParseError::Merge { error, .. }) => Some(error),
            _ => None,
        }
    }

    fn merge_at(result: ParseResult<impl std::fmt::Debug>) -> (MergeError, usize, usize) {
        match result {
            Err(ParseError::Merge {
                error,
                line,
                column,
                ..
            }) => (error, line, column),
            other => panic!("merge error expected, got {other:?}"),
        }
    }

    #[test]
    fn test_merge_error_position() {
        let cases = [
            ("m:\n  <<: 1\n", MergeError::NotMapping, 2, 3),
            ("<<: 1\n", MergeError::NotMapping, 1, 1),
            ("a: 1\nb: 2\n<<: null\n", MergeError::NotMapping, 3, 1),
            (
                "s: &s !!set {x}\nm:\n  k: 0\n  <<: *s\n",
                MergeError::SetSource,
                4,
                3,
            ),
            ("m:\n  <<: [{a: 1}, 5]\n", MergeError::NotMapping, 2, 3),
            ("m: {k: 0, <<: 1}\n", MergeError::NotMapping, 1, 11),
            ("m:\n  <<: 1\n  <<: {a: 1}\n", MergeError::NotMapping, 2, 3),
            ("m:\n  <<: {a: 1}\n  <<: 1\n", MergeError::NotMapping, 3, 3),
            ("a:\n  b:\n    <<: 1\n", MergeError::NotMapping, 3, 5),
            ("- x: 1\n- <<: 1\n", MergeError::NotMapping, 2, 3),
            ("m:\n  \u{e9}: 1\n  <<: 1\n", MergeError::NotMapping, 3, 3),
            ("a: 1\r\nm:\r\n  <<: 1\r\n", MergeError::NotMapping, 3, 3),
        ];
        for (yaml, error, line, column) in cases {
            assert_eq!(
                merge_at(Parser::parse_str(yaml)),
                (error, line, column),
                "{yaml:?}"
            );
        }
    }

    #[test]
    fn test_merge_error_position_alias_key() {
        let yaml = "a: {&m <<: {x: 1}}\nb:\n  *m : 5\n";
        assert_eq!(
            merge_at(Parser::parse_str(yaml)),
            (MergeError::NotMapping, 3, 3)
        );
    }

    #[test]
    fn test_merge_error_position_non_ascii_prefix() {
        let yaml = "m: {\"\u{e9}\u{e9}\": 1, <<: 1}\n";
        assert_eq!(
            merge_at(Parser::parse_str(yaml)),
            (MergeError::NotMapping, 1, 14)
        );
    }

    #[test]
    fn test_merge_error_position_across_documents() {
        let yaml = "a: 1\n---\nb: 2\n---\nm:\n  <<: 1\n";
        assert_eq!(
            merge_at(Parser::parse_all(yaml)),
            (MergeError::NotMapping, 6, 3)
        );
    }

    #[test]
    fn test_merge_error_position_display() {
        let err = Parser::parse_str("m:\n  <<: 1\n").unwrap_err();
        assert_eq!(
            err.to_string(),
            "merge key `<<` requires a mapping or a sequence of mappings at line 2, column 3"
        );
    }

    fn format_merge_at(yaml: &str) -> Option<(MergeError, usize, usize)> {
        match crate::streaming::format_streaming(yaml, &crate::EmitterConfig::default()) {
            Err(crate::EmitError::Parse(err)) => Some(merge_at(Err::<(), _>(err))),
            _ => None,
        }
    }

    /// Asserts that every parse entry point and the formatter reject `yaml` identically.
    fn assert_rejected_everywhere(yaml: &str, expected: (MergeError, usize, usize)) {
        let budget = StreamBudget::new(ParseLimits::default());
        assert_eq!(merge_at(Parser::parse_all(yaml)), expected, "{yaml:?}");
        assert_eq!(merge_at(Parser::parse_str(yaml)), expected, "{yaml:?}");
        assert_eq!(
            merge_at(Parser::parse_all_with_budget(yaml, &budget)),
            expected,
            "{yaml:?}"
        );
        assert_eq!(
            merge_at(Parser::parse_normalized(
                &NormalizedInput::new(yaml).unwrap(),
                &budget
            )),
            expected,
            "{yaml:?}"
        );
        assert_eq!(format_merge_at(yaml), Some(expected), "{yaml:?}");
    }

    fn assert_accepted_everywhere(yaml: &str) {
        let budget = StreamBudget::new(ParseLimits::default());
        assert!(Parser::parse_all(yaml).is_ok(), "{yaml:?}");
        assert!(Parser::parse_str(yaml).is_ok(), "{yaml:?}");
        assert!(
            Parser::parse_all_with_budget(yaml, &budget).is_ok(),
            "{yaml:?}"
        );
        assert!(
            Parser::parse_normalized(&NormalizedInput::new(yaml).unwrap(), &budget).is_ok(),
            "{yaml:?}"
        );
        assert!(
            crate::streaming::format_streaming(yaml, &crate::EmitterConfig::default()).is_ok(),
            "{yaml:?}"
        );
    }

    #[test]
    fn test_merge_error_hidden_by_duplicate_key() {
        let cases = [
            ("x: {<<: 1}\nx: 2\n", MergeError::NotMapping, 1, 5),
            ("<<: 1\n<<: {a: 1}\n", MergeError::NotMapping, 1, 1),
            ("a:\n  x: {<<: 1}\n  x: 2\n", MergeError::NotMapping, 2, 7),
            ("x: &a {<<: 1}\nx: 2\ny: *a\n", MergeError::NotMapping, 1, 8),
            (
                "a: 1\n---\nx: {<<: !!set {a}}\nx: 2\n",
                MergeError::SetSource,
                3,
                5,
            ),
            ("x: {a: 1, a: {<<: 1}}\n", MergeError::NotMapping, 1, 15),
            ("x: {a: {<<: 1}, a: 1}\n", MergeError::NotMapping, 1, 9),
            ("x: {<<: [{a: 1}, 2]}\nx: 3\n", MergeError::NotMapping, 1, 5),
            ("0x10: {<<: 1}\n16: 2\n", MergeError::NotMapping, 1, 8),
            (
                "a: &a {x: 1}\nb: &a 5\nc: {<<: *a}\n",
                MergeError::NotMapping,
                3,
                5,
            ),
            (
                "s: &s !!set {<<: {a: 1}}\nx: {<<: *s}\n",
                MergeError::SetSource,
                2,
                5,
            ),
        ];
        for (yaml, error, line, column) in cases {
            assert_rejected_everywhere(yaml, (error, line, column));
        }
    }

    #[test]
    fn test_merge_error_is_first_in_document_order() {
        let cases = [
            (
                "m: {<<: 1, x: {<<: !!set {a}}}\n",
                MergeError::NotMapping,
                1,
                5,
            ),
            (
                "m: {x: {<<: !!set {a}}, <<: 1}\n",
                MergeError::SetSource,
                1,
                9,
            ),
            (
                "m: {<<: {a: 1}, y: {<<: 2}, <<: 3}\n",
                MergeError::NotMapping,
                1,
                21,
            ),
            (
                "m:\n  <<: [{a: 1}, {<<: 1}, 2]\n",
                MergeError::NotMapping,
                2,
                17,
            ),
        ];
        for (yaml, error, line, column) in cases {
            assert_rejected_everywhere(yaml, (error, line, column));
        }
    }

    #[test]
    fn test_valid_merges_unaffected_by_duplicates() {
        let yaml = "x: {<<: {a: 1}}\nx: 2\nm: {<<: {a: 1}, <<: {b: 2}}\n";
        let Some(Value::Mapping(map)) = Parser::parse_str(yaml).unwrap() else {
            panic!("mapping expected");
        };
        let key = |k: &str| Value::String(k.into());
        assert_eq!(map[&key("x")], Value::Int(2));
        let Value::Mapping(m) = &map[&key("m")] else {
            panic!("mapping expected");
        };
        assert_eq!(m.len(), 1);
        assert_eq!(m[&key("b")], Value::Int(2));
    }

    #[test]
    fn test_anchor_redefinition_uses_the_latest_definition() {
        assert_accepted_everywhere("a: &a 5\nb: &a {x: 1}\nc: {<<: *a}\n");
    }

    #[test]
    fn test_duplicate_non_merge_keys_are_accepted() {
        for yaml in [
            "m: {\"<<\": 1, \"<<\": 2}\n",
            "m: {!!str <<: 1, !!str <<: 2}\n",
            "m: !!set {<<, <<}\n",
        ] {
            assert_accepted_everywhere(yaml);
        }
    }

    #[test]
    fn test_hidden_merge_error_in_later_document_and_first_error_wins() {
        let yaml = "a: 1\n---\nx: {<<: 1}\nx: 2\n";
        assert_rejected_everywhere(yaml, (MergeError::NotMapping, 3, 5));
        assert_eq!(Parser::parse_all(yaml).unwrap_err().document_index(), 1);

        let yaml = "a: {<<: 1}\n---\nb: {<<: !!set {x}}\n---\nc: {<<: 2}\n";
        assert_rejected_everywhere(yaml, (MergeError::NotMapping, 1, 5));
        assert_eq!(Parser::parse_all(yaml).unwrap_err().document_index(), 0);
    }

    #[test]
    fn test_merge_error_document_index() {
        let yaml = "a: 1\n---\nb: 2\n---\nm:\n  <<: 1\n";
        let budget = StreamBudget::new(ParseLimits::default());
        for err in [
            Parser::parse_str(yaml).unwrap_err(),
            Parser::parse_all(yaml).unwrap_err(),
            Parser::parse_normalized(&NormalizedInput::new(yaml).unwrap(), &budget).unwrap_err(),
        ] {
            assert_eq!(err.document_index(), 2);
            assert!(err.to_string().ends_with("(document 3)"), "{err}");
        }
    }

    #[test]
    fn test_scanner_and_limit_errors_report_their_document() {
        let scanner = Parser::parse_all(
            "a: 1\n---
b: 2\n---\nc: [\n",
        )
        .unwrap_err();
        assert!(
            matches!(&scanner, ParseError::Syntax(e) if e.document() == 2),
            "{scanner:?}"
        );
        assert_eq!(scanner.document_index(), 2);

        let limits = ParseLimits {
            max_depth: crate::MaxDepth::new(2).unwrap(),
            ..ParseLimits::default()
        };
        let err =
            Parser::parse_all_with_limits("a: 1\n---\nb: 2\n---\nc: {d: {e: {f: 1}}}\n", &limits)
                .unwrap_err();
        assert!(
            matches!(err, ParseError::LimitExceeded { document: 2, .. }),
            "{err:?}"
        );
    }

    #[test]
    fn test_error_display_names_only_later_documents() {
        let first = Parser::parse_all("a: [\n").unwrap_err().to_string();
        assert!(!first.contains("document"), "{first}");
        let later = Parser::parse_all("a: 1\n---\nb: [\n")
            .unwrap_err()
            .to_string();
        assert!(later.ends_with("(document 2)"), "{later}");
    }

    #[test]
    fn test_nul_after_document_end_belongs_to_the_following_document() {
        let err = Parser::parse_all("a: 1\n...\n\0\n").unwrap_err();
        assert_eq!(err.document_index(), 1);
    }

    #[test]
    fn test_self_referencing_alias_is_a_syntax_error_in_every_position() {
        for input in ["&a {*a : 1}\n", "&a [*a]\n", "&a {k: *a}\n"] {
            assert!(matches!(
                Parser::parse_str(input),
                Err(ParseError::Syntax(_))
            ));
        }
    }

    #[test]
    fn test_anchors_do_not_cross_documents_in_the_loader() {
        let docs = Parser::parse_all("a: &x 1\n---\nb: &x 2\nc: *x\n").unwrap();
        let Value::Mapping(second) = &docs[1] else {
            panic!("expected a mapping");
        };
        let c = Value::String("c".into());
        assert_eq!(second.get(&c), Some(&Value::Int(2)));
    }

    #[test]
    fn test_first_document_error_reports_index_zero() {
        let err = Parser::parse_all("a: [\n---\nb: 1\n").unwrap_err();
        assert_eq!(err.document_index(), 0);
    }

    #[test]
    fn test_stale_alias_error_reports_its_document() {
        let err = Parser::parse_all("a: &x 1\n---\nb: *x\n").unwrap_err();
        assert!(
            matches!(&err, ParseError::Syntax(e) if e.document() == 1),
            "{err:?}"
        );
    }

    #[test]
    fn test_relocated_shifts_merge_document() {
        let err = Parser::parse_all("m: {<<: 1}\n")
            .unwrap_err()
            .relocated(4, 3);
        assert!(matches!(
            err,
            ParseError::Merge {
                line: 5,
                column: 5,
                document: 3,
                ..
            }
        ));
    }

    #[test]
    fn test_set_source_error_follows_item_order() {
        let set = "s: &s !!set {x}\nm:\n  <<: ";
        for (merge, expected) in [
            ("*s", MergeError::SetSource),
            ("[*s]", MergeError::SetSource),
            ("[{z: 1}, *s]", MergeError::SetSource),
            ("[5, *s]", MergeError::NotMapping),
            ("[[*s]]", MergeError::NotMapping),
        ] {
            assert_eq!(
                merge_error(&format!("{set}{merge}\n")),
                Some(expected),
                "{merge}"
            );
        }
    }

    #[test]
    fn test_forged_set_marker_tag_is_an_ordinary_tag() {
        for tag in ["!x!set", "!x!foo"] {
            let yaml = format!(
                "%TAG !x! tag:fast-yaml.internal:\n---\nb: &b {{x: 1}}\nm: {tag} {{<<: *b, k: 0}}\n"
            );
            assert_eq!(merged_entries(&yaml, "m"), ["x: 1", "k: 0"], "{tag}");
        }
    }

    #[test]
    fn test_core_tagged_merge_values_are_mappings() {
        assert_eq!(
            merged_entries("m:\n  <<: !!map {x: 1}\n  k: 0\n", "m"),
            ["x: 1", "k: 0"]
        );
        assert_eq!(
            merged_entries("m:\n  <<: !!seq [{x: 1}, {y: 2}]\n  k: 0\n", "m"),
            ["x: 1", "y: 2", "k: 0"]
        );
    }

    #[test]
    fn test_nested_merge_error_propagates() {
        for yaml in [
            "m:\n  <<: {<<: 1}\n",
            "a: &a {<<: 1}\nm:\n  <<: *a\n",
            "m:\n  <<: [{x: 1}, {<<: [2]}]\n",
            "m: [{<<: {<<: null}}]\n",
        ] {
            assert_eq!(merge_error(yaml), Some(MergeError::NotMapping), "{yaml}");
        }
    }

    #[test]
    fn test_duplicate_plain_merge_key_last_wins() {
        let yaml = "a: &a {x: 1}\nb: &b {y: 2}\nm:\n  <<: *a\n  <<: *b\n";
        assert_eq!(merged_entries(yaml, "m"), ["y: 2"]);
    }

    #[test]
    fn test_duplicate_merge_key_rejects_invalid_earlier_value() {
        for yaml in [
            "m:\n  <<: 1\n  <<: {a: 1}\n",
            "s: &s !!set {x}\nm:\n  <<: *s\n  <<: {a: 1}\n",
            "m: {<<: [2], <<: {a: 1}}\n",
        ] {
            assert!(merge_error(yaml).is_some(), "{yaml}");
        }
        let set = Parser::parse_str("s: !!set {<<, <<, k}\n")
            .unwrap()
            .unwrap();
        assert_eq!(
            entry_texts(&sub_mapping(&set, "s")),
            ["<<: null", "k: null"]
        );
    }

    const ANCHORED_KEY: &str = "m: {&k <<: {x: 1}}\n";

    #[test]
    fn test_alias_to_anchored_merge_key_in_value_is_the_plain_string() {
        let doc = format!("{ANCHORED_KEY}b: *k\n");
        assert_eq!(scalar_text(&get_mapping_val(&doc, "b")), "<<");
    }

    #[test]
    fn test_alias_to_anchored_merge_key_in_sequence_is_the_plain_string() {
        let doc = format!("{ANCHORED_KEY}n: [*k]\n");
        let Value::Sequence(items) = get_mapping_val(&doc, "n") else {
            panic!("expected sequence")
        };
        assert_eq!(items.iter().map(scalar_text).collect::<Vec<_>>(), ["<<"]);
    }

    #[test]
    fn test_alias_to_anchored_merge_key_as_later_key_merges() {
        let doc = Parser::parse_str(&format!("{ANCHORED_KEY}n: {{*k : {{y: 2}}}}\n"))
            .unwrap()
            .unwrap();
        assert_eq!(entry_texts(&sub_mapping(&doc, "n")), ["y: 2"]);
    }

    #[test]
    fn test_alias_to_anchored_merge_key_in_set_is_the_plain_string() {
        let doc = Parser::parse_str(&format!("{ANCHORED_KEY}s: !!set {{*k, a}}\n"))
            .unwrap()
            .unwrap();
        assert_eq!(
            entry_texts(&sub_mapping(&doc, "s")),
            ["<<: null", "a: null"]
        );
    }

    #[test]
    fn test_duplicate_merge_key_through_alias_rejects_invalid_earlier_value() {
        let yaml = "m:\n  ? &k <<\n  : 1\n  *k : {y: 2}\n";
        assert_eq!(merge_error(yaml), Some(MergeError::NotMapping));
    }

    #[test]
    fn test_alias_to_plain_merge_key_scalar_merges() {
        let yaml = "k: &k <<\nb: &b {x: 1}\nm:\n  *k : *b\n  z: 0\n";
        assert_eq!(merged_entries(yaml, "m"), ["x: 1", "z: 0"]);
    }

    #[test]
    fn test_aliased_set_keeps_merge_element() {
        let doc = Parser::parse_str("a: &a !!set {k, <<}\nb: *a\n")
            .unwrap()
            .unwrap();
        assert_eq!(
            entry_texts(&sub_mapping(&doc, "b")),
            ["k: null", "<<: null"]
        );
    }

    #[test]
    fn test_set_keeps_merge_element() {
        let doc = Parser::parse_str("s: !!set {k, <<}\n").unwrap().unwrap();
        assert_eq!(
            entry_texts(&sub_mapping(&doc, "s")),
            ["k: null", "<<: null"]
        );
    }

    #[test]
    fn test_merge_key_multi_document() {
        let docs = Parser::parse_all(
            "a: &a {x: 1}\nm:\n  <<: *a\n---\nb: &b {y: 2}\nm:\n  k: 0\n  <<: *b\n",
        )
        .unwrap();
        let entries: Vec<Vec<String>> = docs
            .iter()
            .map(|doc| entry_texts(&sub_mapping(doc, "m")))
            .collect();
        assert_eq!(entries, [vec!["x: 1"], vec!["y: 2", "k: 0"]]);
    }

    #[test]
    fn test_merge_key_shallow() {
        let yaml = "b: &b {n: {p: 1, q: 2}}
m:
  <<: *b
  n: {p: 9}
";
        let doc = Parser::parse_str(yaml).unwrap().unwrap();
        let Value::Mapping(n) = sub_mapping(&doc, "m")[&Value::String("n".into())].clone() else {
            panic!("expected mapping")
        };
        assert_eq!(n.len(), 1);
    }

    #[test]
    fn test_i64_max_boundary() {
        let v = get_mapping_val("x: 9223372036854775807", "x");
        assert!(
            matches!(v, Value::Int(i64::MAX)),
            "i64::MAX should stay Integer, got {v:?}"
        );

        let v = get_mapping_val("x: 9223372036854775808", "x");
        assert_eq!(v, big_int("9223372036854775808"));
    }

    #[test]
    fn test_leading_plus_large_integer() {
        let v = get_mapping_val("x: +42", "x");
        assert!(
            matches!(v, Value::Int(42)),
            "+42 should be Integer(42), got {v:?}"
        );

        let v = get_mapping_val("x: +99999999999999999999", "x");
        assert_eq!(v, big_int("99999999999999999999"));
    }

    #[test]
    fn test_large_integer_becomes_big_int() {
        let big =
            "99999999999999999999999999999999999999999999999999999999999999999999999999999999";
        let v = get_mapping_val(&format!("x: {big}"), "x");
        assert_eq!(v, big_int(big));
    }

    #[test]
    fn test_quoted_large_integer_stays_string() {
        let v = get_mapping_val("x: \"9223372036854775808\"", "x");
        assert!(
            matches!(v, Value::String(ref s) if s == "9223372036854775808"),
            "got {v:?}"
        );
    }

    #[test]
    fn test_tagged_large_integer_is_big_int() {
        let v = get_mapping_val("x: !!int 9223372036854775808", "x");
        assert_eq!(v, big_int("9223372036854775808"));
    }

    #[test]
    fn test_custom_tag_on_big_integer_is_dropped_but_source_text_kept() {
        let Value::BigInt(v) = get_mapping_val("x: !foo 0xFFFFFFFFFFFFFFFFFF", "x") else {
            panic!("big int expected");
        };
        assert_eq!(v.radix_text(), Some("0xFFFFFFFFFFFFFFFFFF"));
        assert_eq!(v.canonical(), "4722366482869645213695");
    }

    #[test]
    fn test_tagged_str_large_integer_is_string() {
        let v = get_mapping_val("x: !!str 9223372036854775808", "x");
        assert!(
            matches!(v, Value::String(ref s) if s == "9223372036854775808"),
            "got {v:?}"
        );
    }

    #[test]
    fn test_large_integer_as_mapping_key_is_big_int() {
        let root = Parser::parse_str("9223372036854775808: x")
            .unwrap()
            .unwrap();
        let Value::Mapping(map) = root else {
            panic!("expected mapping")
        };
        let key = map.keys().next().unwrap();
        assert_eq!(key, &big_int("9223372036854775808"));
    }

    #[test]
    fn test_normal_integer_unaffected() {
        let v = get_mapping_val("x: 42", "x");
        assert!(matches!(v, Value::Int(42)), "got {v:?}");
    }

    #[test]
    fn test_float_unaffected() {
        let v = get_mapping_val("x: 1.5e10", "x");
        assert!(matches!(v, Value::Float(_)), "got {v:?}");
    }

    #[test]
    fn test_negative_large_integer() {
        let big = "-99999999999999999999999999999999";
        let v = get_mapping_val(&format!("x: {big}"), "x");
        assert_eq!(v, big_int(big));
    }

    #[test]
    fn test_hex_and_octal_overflow_keep_source_text_in_values() {
        for (raw, expected) in [
            ("0x8000000000000000", Some("0x8000000000000000")),
            ("-0x8000000000000001", Some("-0x8000000000000001")),
            ("0o7777777777777777777777", Some("0o7777777777777777777777")),
            ("!!int 0xFFFFFFFFFFFFFFFFFF", Some("0xFFFFFFFFFFFFFFFFFF")),
            (
                "!!int 0o1000000000000000000000",
                Some("0o1000000000000000000000"),
            ),
            ("0XDEADBEEFDEADBEEF", Some("0XDEADBEEFDEADBEEF")),
            ("+0099999999999999999999", None),
        ] {
            let Value::BigInt(v) = get_mapping_val(&format!("x: {raw}"), "x") else {
                panic!("{raw}: big int expected");
            };
            assert_eq!(v.radix_text(), expected, "{raw}");
        }
    }

    #[test]
    fn test_radix_big_integer_keys_collapse_keeping_first_spelling() {
        let Some(Value::Mapping(map)) = Parser::parse_str(
            "0xFFFFFFFFFFFFFFFFFF: a\nz: 0\n4722366482869645213695: b\n0o7777777777777777777777: c\n",
        )
        .unwrap() else {
            unreachable!()
        };
        let entries: Vec<_> = map.iter().collect();
        assert_eq!(entries.len(), 3);
        assert_eq!(radix_text_of(entries[0].0), Some("0xFFFFFFFFFFFFFFFFFF"));
        assert_eq!(entries[0].1, &Value::String("b".into()));
        assert_eq!(
            radix_text_of(entries[2].0),
            Some("0o7777777777777777777777")
        );
    }

    #[test]
    fn test_equal_keys_collapse_at_first_position_with_last_value() {
        let Some(Value::Mapping(map)) =
            Parser::parse_str("0x10: a\nb: 1\n16: c\ntrue: x\nk: 2\nTrue: y\n").unwrap()
        else {
            unreachable!()
        };
        let entries: Vec<_> = map.iter().collect();
        let text = |s: &str| Value::String(s.into());
        assert_eq!(
            entries,
            [
                (&Value::Int(16), &text("c")),
                (&text("b"), &Value::Int(1)),
                (&Value::Bool(true), &text("y")),
                (&text("k"), &Value::Int(2)),
            ]
        );
    }

    fn string_keys(value: &Value) -> Vec<String> {
        let Value::Mapping(map) = value else {
            panic!("expected a mapping")
        };
        map.keys()
            .map(|k| match k {
                Value::String(s) => s.clone(),
                other => panic!("unexpected key {other:?}"),
            })
            .collect()
    }

    #[test]
    fn test_literal_duplicate_keys_keep_first_position_with_last_value() {
        let doc = Parser::parse_str("a: 1\nb: 2\na: 3\nc: 4\na: 5\n")
            .unwrap()
            .unwrap();
        assert_eq!(string_keys(&doc), ["a", "b", "c"]);
        let Value::Mapping(map) = &doc else {
            unreachable!()
        };
        let a = Value::String("a".into());
        assert_eq!(map[&a], Value::Int(5));
    }

    #[test]
    fn test_duplicate_keys_with_anchor_tag_alias_or_complex_shape_keep_first_position() {
        for input in [
            "&x a: 1\nb: 2\na: 3\n",
            "a: 1\nb: 2\n&x a: 3\n",
            "&x a: 1\nb: 2\n&y a: 3\n",
            "k: &x a\nm: {a: 1, b: 2, *x : 3}\n",
            "!!str a: 1\nb: 2\n!!str a: 3\n",
            "!!str a: 1\nb: 2\na: 3\n",
            "a: 1\nb: 2\n!!str a: 3\n",
            "? [a]\n: 1\nb: 2\n? [a]\n: 3\n",
            "? {p: 1}\n: 1\nb: 2\n? {p: 1}\n: 3\n",
        ] {
            let mut doc = Parser::parse_str(input).unwrap().unwrap();
            if input.starts_with("k:") {
                let Value::Mapping(top) = &doc else {
                    unreachable!()
                };
                doc = top[&Value::String("m".into())].clone();
            }
            let Value::Mapping(map) = &doc else {
                panic!("expected a mapping for {input:?}");
            };
            let entries: Vec<_> = map.iter().collect();
            assert_eq!(entries.len(), 2, "{input:?}: {entries:?}");
            assert!(
                matches!(entries[1].0, Value::String(s) if s == "b"),
                "{input:?}: {entries:?}"
            );
            assert_eq!(entries[1].1, &Value::Int(2), "{input:?}");
        }
    }

    #[test]
    fn test_literal_duplicate_keys_in_nested_and_flow_mappings() {
        let doc = Parser::parse_str(
            "m:\n  x: 1\n  y: 2\n  x: 3\nf: {p: 1, q: 2, p: 3}\ns:\n  - {k: 1, j: 2, k: 3}\n",
        )
        .unwrap()
        .unwrap();
        let Value::Mapping(top) = &doc else {
            unreachable!()
        };
        let get = |key: &str| top[&Value::String(key.into())].clone();
        assert_eq!(string_keys(&get("m")), ["x", "y"]);
        assert_eq!(string_keys(&get("f")), ["p", "q"]);
        let Value::Sequence(items) = get("s") else {
            unreachable!()
        };
        assert_eq!(string_keys(&items[0]), ["k", "j"]);
    }

    #[test]
    fn test_literal_duplicate_keys_leave_no_marker_in_output() {
        let doc = Parser::parse_str("a: 1\nb: 2\na: 3\n").unwrap().unwrap();
        assert!(matches!(
            &doc,
            Value::Mapping(map) if map.keys().all(|k| matches!(k, Value::String(_)))
        ));
    }

    #[test]
    fn test_literal_duplicate_keys_in_aliased_mapping_and_merge() {
        let input = "base: &b\n  x: 1\n  y: 2\n  x: 3\nchild:\n  <<: *b\n  z: 4\n";
        let Some(Value::Mapping(top)) = Parser::parse_str(input).unwrap() else {
            unreachable!()
        };
        let child = &top[&Value::String("child".into())];
        assert_eq!(string_keys(child), ["x", "y", "z"]);
    }

    #[test]
    fn test_duplicate_keys_across_documents_are_independent() {
        let docs = Parser::parse_all("a: 1\nb: 2\na: 3\n---\nb: 1\na: 2\n").unwrap();
        assert_eq!(string_keys(&docs[0]), ["a", "b"]);
        assert_eq!(string_keys(&docs[1]), ["b", "a"]);
    }

    #[test]
    fn test_radix_big_integer_keys_collapse_through_merge_and_alias() {
        let input = "base: &b\n  0xFFFFFFFFFFFFFFFFFF: from-base\n  other: 1\nchild:\n  <<: *b\n  4722366482869645213695: explicit\n";
        let Some(Value::Mapping(map)) = Parser::parse_str(input).unwrap() else {
            unreachable!()
        };
        let child = map.get(&Value::String("child".into())).unwrap();
        let Value::Mapping(child) = child else {
            unreachable!()
        };
        assert_eq!(child.len(), 2);
        let (key, value) = child.iter().next().unwrap();
        assert_eq!(radix_text_of(key), Some("0xFFFFFFFFFFFFFFFFFF"));
        assert_eq!(value, &Value::String("explicit".into()));
    }

    #[test]
    fn test_hex_min_i64_is_integer() {
        let v = get_mapping_val("x: -0x8000000000000000", "x");
        assert!(matches!(v, Value::Int(i64::MIN)), "got {v:?}");
    }

    #[test]
    fn test_hex_beyond_bit_cap_stays_string() {
        let raw = format!("0x1{}", "0".repeat(3571));
        let v = get_mapping_val(&format!("x: {raw}"), "x");
        assert!(matches!(v, Value::String(ref s) if *s == raw), "got {v:?}");
    }

    #[test]
    fn test_equal_big_integer_spellings_collapse_to_one_key() {
        let root = Parser::parse_str(
            "+99999999999999999999: a\n99999999999999999999: b\n0x56BC75E2D630FFFFF: c",
        )
        .unwrap()
        .unwrap();
        let Value::Mapping(map) = root else {
            panic!("expected mapping")
        };
        assert_eq!(map.len(), 1, "got {map:?}");
        let (key, value) = map.iter().next().unwrap();
        assert_eq!(key, &big_int("99999999999999999999"));
        assert_eq!(radix_text_of(key), None);
        assert!(matches!(value, Value::String(s) if s == "c"));
    }

    #[test]
    fn test_hex_max_i64_preserved_as_integer() {
        // 0x7FFFFFFFFFFFFFFF == i64::MAX == 9223372036854775807
        let v = get_mapping_val("x: 0x7FFFFFFFFFFFFFFF", "x");
        assert!(
            matches!(v, Value::Int(9_223_372_036_854_775_807)),
            "0x7FFFFFFFFFFFFFFF should be Integer(i64::MAX), got {v:?}"
        );
    }

    #[test]
    fn test_octal_max_fitting_preserved_as_integer() {
        // 0o777777777777777777777 == i64::MAX == 9223372036854775807
        let v = get_mapping_val("x: 0o777777777777777777777", "x");
        assert!(
            matches!(v, Value::Int(9_223_372_036_854_775_807)),
            "0o777777777777777777777 should be Integer(i64::MAX), got {v:?}"
        );
    }

    #[test]
    fn test_tagged_int_hex_fits_i64() {
        let v = get_mapping_val("x: !!int 0xFF", "x");
        assert!(
            matches!(v, Value::Int(255)),
            "!!int 0xFF should be Integer(255), got {v:?}"
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
        assert!(matches!(docs[0], Value::Null));
    }

    #[test]
    fn test_comment_only_yields_null_doc() {
        let docs = Parser::parse_all("# comment").unwrap();
        assert_eq!(docs.len(), 1);
        assert!(matches!(docs[0], Value::Null));
    }

    #[test]
    fn test_bare_doc_end_yields_null_doc() {
        let docs = Parser::parse_all("...").unwrap();
        assert_eq!(docs.len(), 1);
        assert!(matches!(docs[0], Value::Null));
    }

    #[test]
    fn test_comment_then_doc_end_yields_null_doc() {
        let docs = Parser::parse_all("# c\n...").unwrap();
        assert_eq!(docs.len(), 1);
        assert!(matches!(docs[0], Value::Null));
    }

    #[test]
    fn test_bare_doc_start_yields_null_doc() {
        let docs = Parser::parse_all("---").unwrap();
        assert_eq!(docs.len(), 1);
        assert!(matches!(docs[0], Value::Null));
    }

    #[test]
    fn test_parse_str_comment_only_returns_null() {
        let result = Parser::parse_str("# comment").unwrap();
        assert!(matches!(result, Some(Value::Null)));
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
            matches!(v, Value::String(ref s) if s == "99"),
            "! 99 should be String(\"99\"), got {v:?}"
        );
    }

    #[test]
    fn test_non_specific_tag_quoted_is_string() {
        let v = get_mapping_val("x: ! \"99\"", "x");
        assert!(
            matches!(v, Value::String(ref s) if s == "99"),
            "! \"99\" should be String(\"99\"), got {v:?}"
        );
    }

    #[test]
    fn test_non_specific_tag_true_is_string() {
        let v = get_mapping_val("x: ! true", "x");
        assert!(
            matches!(v, Value::String(ref s) if s == "true"),
            "! true should be String(\"true\"), got {v:?}"
        );
    }

    #[test]
    fn test_non_specific_tag_null_keyword_is_string() {
        let v = get_mapping_val("x: ! null", "x");
        assert!(
            matches!(v, Value::String(ref s) if s == "null"),
            "! null should be String(\"null\"), got {v:?}"
        );
    }

    #[test]
    fn test_non_specific_tag_empty_is_string_not_null() {
        // `! ''` must be String(""), NOT Null. Order of branches is load-bearing.
        let v = get_mapping_val("x: ! ''", "x");
        assert!(
            matches!(v, Value::String(ref s) if s.is_empty()),
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
        let k = Value::String("x".into());
        let val = &map[&k];
        assert!(
            matches!(val, Value::Sequence(_)),
            "! on sequence must stay Sequence, got {val:?}"
        );
    }

    #[test]
    fn test_parse_all_strips_only_one_of_double_bom() {
        let budget = StreamBudget::new(ParseLimits::default());
        let docs = Parser::parse_all_with_budget("\u{FEFF}\u{FEFF}", &budget).unwrap();
        assert_eq!(docs, vec![Value::String("\u{FEFF}".into())]);
    }

    #[test]
    fn test_bom_before_comment_and_mapping_parses() {
        let value = Parser::parse_str("\u{FEFF}# c\na: 1").unwrap().unwrap();
        let Value::Mapping(map) = value else {
            panic!("expected mapping, got {value:?}");
        };
        assert!(
            map.keys()
                .any(|k| matches!(k, Value::String(s) if s == "a"))
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
                .any(|k| matches!(k, Value::String(s) if s == "a"))
        );
    }

    #[test]
    fn test_mid_text_bom_stays_data() {
        let v = get_mapping_val("b: \u{FEFF}x", "b");
        assert!(matches!(v, Value::String(ref s) if s == "\u{FEFF}x"));
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
        assert!(matches!(v, Value::Int(1)));
    }

    #[test]
    fn test_bom_multi_document() {
        let docs = Parser::parse_all("\u{FEFF}---\na: 1\n---\nb: 2\n").unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn test_bom_only_parse_str_is_null() {
        let v = Parser::parse_str("\u{FEFF}").unwrap();
        assert!(matches!(v, Some(Value::Null)));
    }

    mod limits {
        use super::*;
        use crate::error::ParseError;
        use crate::limits::{LimitKind, MaxAliasBytes, MaxDepth, MaxTagBytes, NODE_BYTES};
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
                max_depth: MaxDepth::new(depth).unwrap(),
                ..ParseLimits::default()
            }
        }

        fn alias_limits(bytes: usize) -> ParseLimits {
            ParseLimits {
                max_alias_bytes: MaxAliasBytes::new(bytes).unwrap(),
                ..ParseLimits::default()
            }
        }

        #[test]
        fn nested_anchor_copies_are_bounded_by_the_budget_and_the_source_size() {
            let nested = |levels: usize, ints: usize| {
                let leaf = (0..ints).fold(String::new(), |mut acc, i| {
                    write!(acc, "{}, ", 100_000 + i).unwrap();
                    acc
                });
                let open = (0..levels).fold(String::new(), |mut acc, i| {
                    write!(acc, "&a{i} [").unwrap();
                    acc
                });
                format!("{open}{leaf}0{}", "]".repeat(levels))
            };
            // a few anchored wrappers over a wide leaf are legitimate even under a tiny budget
            assert!(Parser::parse_str_with_limits(&nested(3, 2000), &alias_limits(1)).is_ok());
            assert!(Parser::parse_str_with_limits(&nested(5, 2000), &alias_limits(1)).is_ok());
            // many wrappers: the copies dwarf the source
            for levels in [50, 100, 250] {
                assert!(
                    matches!(
                        Parser::parse_str_with_limits(
                            &nested(levels, 2000),
                            &alias_limits(1 << 20)
                        ),
                        Err(ParseError::LimitExceeded {
                            kind: LimitKind::AnchorCopies(_),
                            ..
                        })
                    ),
                    "{levels}"
                );
            }
            // a budget above the copies accepts the same document
            assert!(
                Parser::parse_str_with_limits(&nested(100, 2000), &alias_limits(1 << 30)).is_ok()
            );
        }

        #[test]
        fn sibling_anchors_are_not_charged() {
            let input = (0..2000).fold(String::new(), |mut acc, i| {
                writeln!(acc, "- &a{i} [{}]", "y".repeat(100)).unwrap();
                acc
            });
            assert!(Parser::parse_str_with_limits(&input, &alias_limits(1 << 20)).is_ok());
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

        fn parse_and_drop_at_max_depth(input: String, stack_size: usize) {
            std::thread::Builder::new()
                .stack_size(stack_size)
                .spawn(move || {
                    let limits = ParseLimits {
                        max_depth: MaxDepth::MAX,
                        ..ParseLimits::default()
                    };
                    for value in Parser::parse_all_with_limits(&input, &limits).unwrap() {
                        drop(value);
                    }
                })
                .unwrap()
                .join()
                .unwrap();
        }

        #[test]
        fn max_depth_block_sequence_parses_and_drops_on_2mib_stack() {
            parse_and_drop_at_max_depth(
                format!("{}x", "- ".repeat(MaxDepth::MAX.get())),
                2 * 1024 * 1024,
            );
        }

        #[test]
        fn max_depth_tagged_sequence_parses_and_drops_on_1mib_stack() {
            let depth = MaxDepth::MAX.get();
            let mut input = String::new();
            for i in 0..depth {
                writeln!(input, "{}- !t", "  ".repeat(i)).unwrap();
            }
            input.push_str(&"  ".repeat(depth));
            input.push('x');
            parse_and_drop_at_max_depth(input, 1024 * 1024);
        }

        #[test]
        fn max_depth_nested_mappings_parse_and_drop_on_2mib_stack() {
            let depth = MaxDepth::MAX.get();
            let mut input = String::new();
            for i in 0..depth {
                writeln!(input, "{}k:", " ".repeat(i)).unwrap();
            }
            input.push_str(&" ".repeat(depth));
            input.push('x');
            parse_and_drop_at_max_depth(input, 2 * 1024 * 1024);
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

        fn tag_limits(bytes: usize) -> ParseLimits {
            ParseLimits {
                max_tag_bytes: MaxTagBytes::new(bytes),
                ..ParseLimits::default()
            }
        }

        fn tag_doc(directive: &str, tag: &str, uses: usize) -> String {
            let long = "a".repeat(200);
            let directive = directive.replace("PREFIX", &long);
            let mut doc = format!("{directive}\n---\n");
            for i in 0..uses {
                writeln!(doc, "k{i}: {tag} v").unwrap();
            }
            doc
        }

        #[test]
        fn tag_prefix_reuse_over_budget_is_rejected() {
            let doc = tag_doc("%TAG !e! tag:e.com,PREFIX", "!e!x", 10);
            let per_use = 200 + "tag:e.com,".len() - crate::limits::TAG_PREFIX_ALLOWANCE;
            assert!(Parser::parse_all_with_limits(&doc, &tag_limits(per_use * 10)).is_ok());
            assert!(matches!(
                Parser::parse_all_with_limits(&doc, &tag_limits(per_use * 10 - 1)),
                Err(ParseError::LimitExceeded {
                    kind: LimitKind::TagBytes(_),
                    ..
                })
            ));
        }

        #[test]
        fn tag_prefix_overrides_are_rejected() {
            for (directive, tag) in [
                ("%TAG !! tag:e.com,PREFIX", "!!x"),
                ("%TAG ! tag:e.com,PREFIX", "!x"),
            ] {
                let doc = tag_doc(directive, tag, 10);
                assert!(
                    matches!(
                        Parser::parse_all_with_limits(&doc, &tag_limits(1_000)),
                        Err(ParseError::LimitExceeded {
                            kind: LimitKind::TagBytes(_),
                            ..
                        })
                    ),
                    "{directive}"
                );
            }
        }

        #[test]
        fn tag_prefix_on_collections_is_rejected() {
            let long = "a".repeat(200);
            let mut doc = format!("%TAG !e! tag:e.com,{long}\n---\n");
            for _ in 0..10 {
                doc.push_str("- !e!m {a: 1}\n- !e!s [1]\n");
            }
            assert!(matches!(
                Parser::parse_all_with_limits(&doc, &tag_limits(1_000)),
                Err(ParseError::LimitExceeded {
                    kind: LimitKind::TagBytes(_),
                    ..
                })
            ));
        }

        #[test]
        fn many_core_tags_stay_free_under_default_budget() {
            let mut doc = String::new();
            for i in 0..10_000 {
                writeln!(doc, "k{i}: !!str v").unwrap();
            }
            assert!(Parser::parse_all(&doc).is_ok());
        }

        #[test]
        fn cross_document_alias_is_unknown_anchor() {
            let err = Parser::parse_all("--- &a [x]\n--- *a\n").unwrap_err();
            assert!(matches!(err, ParseError::Syntax(_)), "{err:?}");
            assert!(err.to_string().contains("unknown anchor"));
        }

        #[test]
        fn self_referential_alias_is_a_syntax_error_at_the_alias() {
            for input in [
                "&a [*a]",
                "&a {k: *a}",
                "- &a
  - *a
",
            ] {
                let err = Parser::parse_all(input).unwrap_err();
                assert!(matches!(err, ParseError::Syntax(_)), "{input:?}: {err:?}");
                assert!(err.to_string().contains("still being defined"), "{err}");
            }
            let position = Parser::parse_all("&a [*a]").unwrap_err().position();
            assert_eq!((position.line, position.column), (1, 5));
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

    fn keys_of(yaml: &str) -> Vec<String> {
        let Some(Value::Mapping(map)) = Parser::parse_str(yaml).unwrap() else {
            panic!("mapping expected");
        };
        map.iter()
            .map(|(k, v)| format!("{}={}", k.key_text().unwrap(), v.key_text().unwrap()))
            .collect()
    }

    #[test]
    fn duplicate_keys_keep_the_first_position_and_the_last_value() {
        assert_eq!(keys_of("b: 1\na: 2\nb: 3\n"), ["b=3", "a=2"]);
        assert_eq!(keys_of("\"b\": 1\na: 2\n'b': 3\nb: 4\n"), ["b=4", "a=2"]);
        assert_eq!(
            keys_of("x: 1\n+99999999999999999999: a\ny: 2\n99999999999999999999: b\n"),
            ["x=1", "99999999999999999999=b", "y=2"]
        );
        assert_eq!(keys_of("{b: 1, a: 2, b: 3}"), ["b=3", "a=2"]);
    }

    #[test]
    fn duplicate_keys_across_merge_keep_explicit_position() {
        let yaml = "base: &b {x: 1, y: 2}\nm:\n  y: 0\n  <<: *b\n  y: 9\n  y: 10\n";
        assert_eq!(merged_entries(yaml, "m"), ["x: 1", "y: 10"]);
    }

    #[test]
    fn anchored_values_are_shared_by_value() {
        let doc = Parser::parse_str("a: &x [1, {k: v}]\nb: *x\n")
            .unwrap()
            .unwrap();
        let Value::Mapping(map) = doc else {
            unreachable!()
        };
        assert_eq!(
            map.get(&Value::String("a".into())),
            map.get(&Value::String("b".into()))
        );
    }

    #[test]
    fn scalar_types_follow_the_core_schema_without_tags_in_the_value() {
        let doc = Parser::parse_str("a: !custom 5\nb: !!set {x}\nc: !local [1]\n")
            .unwrap()
            .unwrap();
        let Value::Mapping(map) = doc else {
            unreachable!()
        };
        assert_eq!(map[&Value::String("a".into())], Value::Int(5));
        let Value::Set(set) = &map[&Value::String("b".into())] else {
            panic!("a !!set loads as a set");
        };
        assert!(set.contains(&Value::String("x".into())));
        assert_eq!(
            map[&Value::String("c".into())],
            Value::Sequence(vec![Value::Int(1)])
        );
    }

    #[test]
    fn deep_nesting_within_limits_loads_without_recursion() {
        let depth = 400;
        let limits = ParseLimits {
            max_depth: crate::limits::MaxDepth::new(512).unwrap(),
            ..ParseLimits::default()
        };
        let yaml = format!("{}1", "- ".repeat(depth));
        assert!(Parser::parse_str_with_limits(&yaml, &limits).is_ok());
    }

    const CORE: &str = "tag:yaml.org,2002:";

    #[test]
    fn verbatim_core_tags_behave_like_the_shorthand() {
        for (shorthand, tag, input, expected) in [
            ("!!int", "int", "\"7\"", Value::Int(7)),
            ("!!int", "int", "3.0", Value::Int(3)),
            (
                "!!float",
                "float",
                "'1'",
                Value::Float(crate::Float::new(1.0)),
            ),
            ("!!bool", "bool", "'true'", Value::Bool(true)),
            ("!!null", "null", "''", Value::Null),
            ("!!str", "str", "42", Value::String("42".into())),
        ] {
            for text in [
                format!("x: {shorthand} {input}"),
                format!("x: !<{CORE}{tag}> {input}"),
                format!("%TAG !e! {CORE}\n---\nx: !e!{tag} {input}"),
            ] {
                assert_eq!(get_mapping_val(&text, "x"), expected, "{text}");
            }
        }
    }

    #[test]
    fn verbatim_tag_outside_the_core_namespace_is_not_a_core_tag() {
        let v = get_mapping_val("x: !<tag:example.com,2000:str> 7", "x");
        assert_eq!(v, Value::Int(7));
        let v = get_mapping_val("x: !<tag:yaml.org,2003:str> 7", "x");
        assert_eq!(v, Value::Int(7));
    }

    #[test]
    fn verbatim_set_tag_is_a_set_for_merging_and_for_its_keys() {
        let set = format!("s: &s !<{CORE}set> {{x}}\nm:\n  <<: *s\n");
        assert_eq!(merge_error(&set), Some(MergeError::SetSource));
        let doc = format!("s: !<{CORE}set> {{k, <<}}\n");
        let Some(Value::Mapping(map)) = Parser::parse_str(&doc).unwrap() else {
            unreachable!()
        };
        let Value::Set(set) = &map[&Value::String("s".into())] else {
            panic!("a !!set loads as a set");
        };
        assert_eq!(set.len(), 2);
        assert!(set.contains(&Value::String("<<".into())));
    }

    #[test]
    fn merge_tag_makes_any_scalar_a_merge_key() {
        for key in [
            "!!merge <<",
            "!!merge '<<'",
            "!!merge \"<<\"",
            "!!merge merge",
            "!!merge ''",
            &format!("!<{CORE}merge> <<"),
        ] {
            let yaml = format!("b: &b {{x: 1}}\nm:\n  {key}: *b\n  k: 0\n");
            assert_eq!(merged_entries(&yaml, "m"), ["x: 1", "k: 0"], "{key}");
        }
    }

    #[test]
    fn merge_tag_key_with_invalid_value_is_rejected_at_the_keys_content() {
        assert_eq!(
            merge_at(Parser::parse_str("m:\n  !!merge <<: 1\n")),
            (MergeError::NotMapping, 2, 11)
        );
        assert_eq!(
            merge_at(Parser::parse_str("m: {k: 0, !!merge x: [1]}\n")),
            (MergeError::NotMapping, 1, 19)
        );
    }

    #[test]
    fn merge_tag_anchor_is_a_merge_key_through_an_alias() {
        let yaml = "k: &k !!merge <<\nb: &b {x: 1}\nm:\n  *k : *b\n  z: 0\n";
        assert_eq!(merged_entries(yaml, "m"), ["x: 1", "z: 0"]);
    }

    #[test]
    fn other_tags_and_quotes_do_not_make_a_merge_key() {
        for key in [
            "'<<'",
            "\"<<\"",
            "!!str <<",
            "!foo <<",
            "!<tag:example.com,2000:merge> <<",
        ] {
            let yaml = format!("b: &b {{x: 1}}\nm:\n  {key}: *b\n  k: 0\n");
            let m = sub_mapping(&Parser::parse_str(&yaml).unwrap().unwrap(), "m");
            assert_eq!(m.len(), 2, "{key}");
        }
    }

    #[test]
    fn merge_tag_inside_a_set_is_an_ordinary_element() {
        let doc = Parser::parse_str("s: !!set {!!merge <<, k}\n")
            .unwrap()
            .unwrap();
        assert_eq!(entry_texts(&sub_mapping(&doc, "s")).len(), 2);
    }

    const B: char = '\u{FEFF}';

    fn texts(docs: &[Value]) -> Vec<String> {
        docs.iter()
            .map(|d| {
                d.key_text()
                    .map_or_else(|| format!("{d:?}"), std::borrow::Cow::into_owned)
            })
            .collect()
    }

    #[test]
    fn bom_in_a_later_document_prefix_is_not_content() {
        for (yaml, expected) in [
            (format!("a\n...\n{B}b"), vec!["a", "b"]),
            (format!("a\n...\n\n# c\n{B}b"), vec!["a", "b"]),
            (format!("a\n...\n{B}%YAML 1.2\n---\nb"), vec!["a", "b"]),
            (format!("a\n---\n{B}--- b"), vec!["a", "null", "b"]),
            (format!("{B}a\n...\n{B}---\nb"), vec!["a", "b"]),
        ] {
            let docs = Parser::parse_all(&yaml);
            assert_eq!(texts(&docs.unwrap()), expected, "{yaml:?}");
        }
    }

    #[test]
    fn bom_elsewhere_is_content() {
        let docs = Parser::parse_all(&format!("a: {B}1\n...\nb{B}")).unwrap();
        assert_eq!(docs[0], Parser::parse_all(&format!("a: {B}1")).unwrap()[0]);
        assert_eq!(texts(&docs[1..]), [format!("b{B}")]);
        let docs = Parser::parse_all(&format!("a\n{B}b")).unwrap();
        assert_eq!(texts(&docs), [format!("a {B}b")]);
    }

    #[test]
    fn marker_after_a_bom_ends_a_root_block_scalar() {
        let docs = Parser::parse_all(&format!("--- |\nfoo\n{B}---\nbar\n")).unwrap();
        // saphyr reads a column-0 `---` inside a root block scalar as content (#407), so only the
        // BOM is lost
        assert_eq!(texts(&docs), ["foo\n---\nbar\n"]);
    }

    #[test]
    fn marker_after_a_bom_inside_a_quoted_scalar_is_an_error() {
        assert!(Parser::parse_all(&format!("\"a\n{B}--- y\"")).is_err());
    }

    #[test]
    fn bom_only_input_is_one_null_document() {
        assert_eq!(
            Parser::parse_all(&B.to_string()).unwrap(),
            vec![Value::Null]
        );
        assert_eq!(Parser::parse_all("").unwrap(), Vec::<Value>::new());
    }

    #[test]
    fn non_printable_characters_are_syntax_errors_at_their_position() {
        for (yaml, c, line, column) in [
            ("a: \u{FFFE}\n", 'U', 1, 4),
            ("a: b\n\u{7F}: 1\n", 'U', 2, 1),
            ("a: \"x\u{86}y\"\n", 'U', 1, 6),
            ("a: 1\n# c\u{FFFF}\n", 'U', 2, 4),
            ("a: 1\0", 'N', 1, 5),
        ] {
            for result in [
                Parser::parse_all(yaml).map(|_| ()),
                Parser::parse_str(yaml).map(|_| ()),
            ] {
                let Err(ParseError::Syntax(err)) = result else {
                    panic!("syntax error expected for {yaml:?}");
                };
                assert_eq!((err.line(), err.column()), (line, column), "{yaml:?}");
                assert!(err.to_string().starts_with(c), "{err}");
                assert!(err.to_string().contains("not allowed in YAML"), "{err}");
            }
        }
    }
}
