use crate::error::{ParseError, ParseResult, SourcePosition};
use crate::limits::{LimitGuard, ParseLimits, StreamBudget};
use crate::merge::{
    MergeError, MergeKeyId, MergeKeyTracker, MergeSource, MergeTarget, NodeRole, is_core_set_tag,
    is_merge_key_marker, is_set_marker, merge_into, merge_key_id, merge_key_tag, set_marker_tag,
};
use crate::scalar::{ResolvedScalar, resolve_scalar};
use crate::value::{Map, Value};
use saphyr::{ScalarOwned, YamlLoader};
use saphyr_parser::{
    Event, Marker, Parser as SaphyrParser, ScalarStyle, Span, SpannedEventReceiver, Tag,
};
use std::borrow::Cow;
use std::collections::HashMap;

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
    /// Returns the first document, but every document is validated, so an invalid merge key in
    /// a later document is an error just as in [`Parser::parse_all_with_limits`].
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Scanner` if the YAML syntax is invalid, `ParseError::Merge` if any
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
        let (docs, positions) =
            load_documents_with_budget(input, &StreamBudget::new(*limits), Bom::Strip)?;
        let mut first = None;
        for (index, doc) in docs.into_iter().enumerate() {
            let doc = canonicalize_located(doc, index, &positions)?;
            first.get_or_insert(doc);
        }
        Ok(first)
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
    /// Returns `ParseError::Scanner` if the YAML syntax is invalid, or
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
        canonicalize_documents(load_documents_with_budget(input, budget, Bom::Strip)?)
    }

    /// Parse all YAML documents of one chunk of a larger stream, without BOM handling.
    ///
    /// Same pipeline as [`Parser::parse_all_with_budget`] except that a leading U+FEFF is
    /// kept as content, matching [`Parser::parse_all`] for a BOM after the stream start: the
    /// caller strips the stream-leading BOM once (see [`strip_bom`]) before splitting.
    ///
    /// # Errors
    ///
    /// Returns `ParseError::Scanner` if the YAML syntax is invalid, or
    /// `ParseError::LimitExceeded` if the input exceeds the budget's limits.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::Parser;
    /// use fast_yaml_core::limits::{ParseLimits, StreamBudget};
    ///
    /// let budget = StreamBudget::new(ParseLimits::default());
    /// let kept = Parser::parse_chunk_with_budget("\u{FEFF}a: 1", &budget)?;
    /// let stripped = Parser::parse_all_with_budget("\u{FEFF}a: 1", &budget)?;
    /// assert_ne!(kept, stripped);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn parse_chunk_with_budget(input: &str, budget: &StreamBudget) -> ParseResult<Vec<Value>> {
        canonicalize_documents(load_documents_with_budget(input, budget, Bom::Keep)?)
    }
}

/// Whether a leading U+FEFF is an encoding signature to drop or content to keep.
#[derive(Clone, Copy)]
enum Bom {
    Strip,
    Keep,
}

fn canonicalize_documents(
    (docs, positions): (Vec<Value>, MergeKeyPositions),
) -> ParseResult<Vec<Value>> {
    docs.into_iter()
        .enumerate()
        .map(|(index, doc)| canonicalize_located(doc, index, &positions))
        .collect()
}

/// Drives the parser event by event so [`LimitGuard`] can reject input before the loader
/// recurses or clones aliases, then returns the un-canonicalized documents together with the
/// source position of every `<<` key.
fn load_documents_with_budget(
    input: &str,
    budget: &StreamBudget,
    bom: Bom,
) -> ParseResult<(Vec<Value>, MergeKeyPositions)> {
    let text = match bom {
        Bom::Strip => strip_bom(input),
        Bom::Keep => input,
    };
    // StrInput is required: BufferedInput loops forever on a directive name at EOF (#403)
    let mut parser = SaphyrParser::new_from_str(reject_nul(text)?);
    let mut loader = YamlLoader::<Value>::default();
    loader.early_parse(false);
    let mut guard = LimitGuard::with_budget(budget.clone());
    let mut keys = MergeKeyTagger::default();
    while let Some(event) = parser.next_event() {
        let (event, span) = event?;
        guard.observe(&event, span)?;
        loader.on_event(keys.tag(mark_set(event), span), span);
    }
    Ok((
        inject_implicit_null_if_empty(loader.into_documents(), input),
        keys.positions,
    ))
}

/// Rejects input containing a NUL (U+0000) character, returning it unchanged otherwise.
///
/// The scanner treats NUL as end of stream and would silently drop everything after it,
/// while YAML 1.2 excludes it from the printable character set (§5.1).
///
/// # Errors
///
/// Returns `ParseError::Scanner` positioned at the first NUL.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::reject_nul;
///
/// assert_eq!(reject_nul("a: 1")?, "a: 1");
/// assert!(reject_nul("a: 1\0\nb: 2").is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn reject_nul(input: &str) -> ParseResult<&str> {
    let Some(offset) = memchr::memchr(0, input.as_bytes()) else {
        return Ok(input);
    };
    let (mut chars, mut line, mut col) = (0, 1, 0);
    let mut prev = None;
    for c in input[..offset].chars() {
        chars += 1;
        match c {
            '\n' => (line, col) = (line + usize::from(prev != Some('\r')), 0),
            '\r' => (line, col) = (line + 1, 0),
            _ => col += 1,
        }
        prev = Some(c);
    }
    let marker = Marker::new(chars, line, col);
    Err(ParseError::Scanner(saphyr::ScanError::new(
        marker,
        "NUL (U+0000) is not allowed in YAML".to_owned(),
    )))
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
///   when present (#203). Every scalar other than a big integer becomes a typed `Value::Value`.
/// - Handle `True`, `TRUE`, `False`, `FALSE`, `Null` mixed-case variants.
/// - Keep integers that overflow `i64` (decimal, hex or octal) as plain `Value::Representation`
///   nodes (non-core tags kept, core tags dropped), so they stay distinguishable from strings.
///   Decimal values hold canonical decimal text; hex and octal values keep their source text.
///   Mapping keys of equal value collapse to one entry whatever their spelling: the first
///   spelling and its position stay, the last value wins.
/// - Because the text of a big integer depends on its spelling, `Value` equality for big integers
///   does too (`0xFF…` and its decimal form are unequal, and `parse(emit(parse(x)))` can differ
///   from `parse(x)`); compare them through [`resolve_scalar`] and
///   [`BigInt::canonical`](crate::BigInt::canonical).
/// - Resolve YAML 1.1 merge keys (`<<: *anchor`) into parent mappings (#204). Only the plain,
///   untagged scalar `<<` is a merge key; inside a `!!set` it is an ordinary element.
///
/// # Errors
///
/// Returns [`MergeError`] when a merge key's value is not a mapping or a sequence of mappings,
/// or is a `!!set`.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{Map, MergeError, ScalarOwned, Value, canonicalize};
/// use saphyr_parser::ScalarStyle;
///
/// let plain = |s: &str| Value::Representation(s.into(), ScalarStyle::Plain, None);
/// let mapping = |pairs: [(Value, Value); 1]| Value::Mapping(Map::from_iter(pairs));
///
/// let merged = canonicalize(mapping([(plain("<<"), mapping([(plain("x"), plain("1"))]))]))?;
/// let x = Value::Value(ScalarOwned::String("x".into()));
/// let Value::Mapping(map) = merged else { panic!("expected a mapping") };
/// assert_eq!(map[&x], Value::Value(ScalarOwned::Integer(1)));
///
/// let err = canonicalize(mapping([(plain("<<"), plain("1"))])).unwrap_err();
/// assert_eq!(err, MergeError::NotMapping);
/// # Ok::<(), MergeError>(())
/// ```
///
/// Recursion depth equals the nesting depth of `value`, which [`ParseLimits`] bounds for
/// parsed input; the collection arms are kept free of scalar temporaries to keep frames small.
pub fn canonicalize(value: Value) -> Result<Value, MergeError> {
    canonicalize_value(value, &mut Blame::default())
}

/// [`canonicalize`] that reports a rejected merge value at the source position of its `<<` key
/// and the index of the document holding it.
fn canonicalize_located(
    value: Value,
    document: usize,
    positions: &MergeKeyPositions,
) -> ParseResult<Value> {
    let mut blame = Blame::default();
    canonicalize_value(value, &mut blame).map_err(|error| {
        let SourcePosition { line, column } = blame
            .0
            .and_then(|id| positions.get(id))
            .unwrap_or_else(|| unreachable!("parser-loaded merge keys are always tagged"));
        ParseError::Merge {
            error,
            line,
            column,
            document,
        }
    })
}

fn canonicalize_value(mut value: Value, blame: &mut Blame) -> Result<Value, MergeError> {
    canonicalize_in_place(&mut value, SetElements::No, blame)?;
    Ok(value)
}

/// Out-of-band record of the `<<` key that failed, so recursion can return a one-byte error.
#[derive(Default)]
struct Blame(Option<MergeKeyId>);

/// Positions of all tagged `<<` keys of a stream, indexed by [`MergeKeyId`].
#[derive(Default)]
struct MergeKeyPositions(Vec<SourcePosition>);

impl MergeKeyPositions {
    fn record(&mut self, span: Span) -> MergeKeyId {
        let id = MergeKeyId::after(self.0.len());
        self.0.push(span.into());
        id
    }

    fn get(&self, id: MergeKeyId) -> Option<SourcePosition> {
        self.0.get(id.index()).copied()
    }
}

/// Whether the mapping being canonicalized holds `!!set` elements, where `<<` is not a merge key.
#[derive(Clone, Copy)]
enum SetElements {
    Yes,
    No,
}

// In place, with a one-byte result: the recursion frame holds no `Value` temporaries.
fn canonicalize_in_place(
    slot: &mut Value,
    set: SetElements,
    blame: &mut Blame,
) -> Result<(), MergeError> {
    match slot {
        Value::Sequence(seq) => {
            for item in seq {
                canonicalize_in_place(item, SetElements::No, blame)?;
            }
            Ok(())
        }
        Value::Mapping(_) => canonicalize_mapping(slot, set, blame),
        Value::Tagged(..) => canonicalize_tagged(slot, blame),
        _ => {
            canonicalize_scalar(slot);
            Ok(())
        }
    }
}

fn canonicalize_mapping(
    slot: &mut Value,
    set: SetElements,
    blame: &mut Blame,
) -> Result<(), MergeError> {
    let Value::Mapping(map) = std::mem::replace(slot, Value::Value(ScalarOwned::Null)) else {
        return Ok(());
    };
    let mut explicit = KeyedMap::with_capacity(map.len());
    let mut merge = None;
    let mut merge_id = None;
    for (k, v) in map {
        if matches!(set, SetElements::No) && is_merge_key(&k) {
            let source = canonicalize_merge_source(v, blame)?;
            // A repeated `<<` keeps only the last value, but every value must be valid
            if let Some(earlier) = merge.replace(source) {
                merge_into(&mut Map::new(), Some(earlier), [])
                    .map_err(|e| blamed(e, merge_id, blame))?;
            }
            merge_id = merge_key_id_of(&k);
        } else {
            let (mut k, mut v) = (k, v);
            canonicalize_in_place(&mut k, SetElements::No, blame)?;
            canonicalize_in_place(&mut v, SetElements::No, blame)?;
            explicit.set(k, v)?;
        }
    }
    *slot = Value::Mapping(match merge {
        None => explicit.map,
        Some(merge) => {
            let mut result = KeyedMap::with_capacity(explicit.map.len());
            merge_into(&mut result, Some(merge), explicit.map)
                .map_err(|e| blamed(e, merge_id, blame))?;
            result.map
        }
    });
    Ok(())
}

/// Records the `<<` key a merge failure belongs to.
#[cold]
#[inline(never)]
const fn blamed(error: MergeError, key: Option<MergeKeyId>, blame: &mut Blame) -> MergeError {
    blame.0 = key;
    error
}

fn merge_key_id_of(key: &Value) -> Option<MergeKeyId> {
    match key {
        Value::Representation(_, _, Some(tag)) => merge_key_id(tag),
        _ => None,
    }
}

/// Identity of a big-integer mapping key: canonical decimal value and tag, independent of spelling.
#[derive(PartialEq, Eq, Hash)]
struct BigKey {
    canonical: String,
    tag: Option<Tag>,
}

impl BigKey {
    fn of(key: &Value) -> Option<Self> {
        let Value::Representation(s, style, tag) = key else {
            return None;
        };
        match resolve_scalar(s, *style, tag.as_ref()) {
            ResolvedScalar::BigInt(big) => Some(Self {
                canonical: big.canonical().into_owned(),
                tag: tag.clone(),
            }),
            _ => None,
        }
    }
}

/// A mapping under construction in which big-integer keys of equal value collapse whatever their
/// spelling: the first spelling and its position stay, the last value wins.
#[derive(Default)]
struct KeyedMap {
    map: Map,
    first_spelling: HashMap<BigKey, Value>,
}

impl KeyedMap {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            map: Map::with_capacity(capacity),
            first_spelling: HashMap::new(),
        }
    }

    fn first_spelling_of(&mut self, key: Value) -> Value {
        match BigKey::of(&key) {
            Some(id) => self.first_spelling.entry(id).or_insert(key).clone(),
            None => key,
        }
    }
}

impl MergeTarget for KeyedMap {
    type Node = Value;
    type Error = MergeError;
    type Entries = Map;
    type Items = Vec<Value>;

    fn classify(&self, node: Value) -> Result<MergeSource<Map, Vec<Value>>, MergeError> {
        self.map.classify(node)
    }

    fn reject(error: MergeError) -> MergeError {
        error
    }

    fn set_if_absent(&mut self, key: Value, value: Value) -> Result<(), MergeError> {
        let key = self.first_spelling_of(key);
        self.map.set_if_absent(key, value)
    }

    fn set(&mut self, key: Value, value: Value) -> Result<(), MergeError> {
        let key = self.first_spelling_of(key);
        self.map.set(key, value)
    }
}

/// Whether `key` is the plain, untagged scalar `<<`; quoted and tagged forms are ordinary keys.
fn is_merge_key(key: &Value) -> bool {
    match key {
        Value::Representation(s, ScalarStyle::Plain, tag) => {
            s == "<<" && tag.as_ref().is_none_or(is_merge_key_marker)
        }
        _ => false,
    }
}

/// Re-tags `!!set` mappings with a non-core marker: saphyr's loader drops core collection
/// tags, and [`canonicalize`] must tell sets from mappings to resolve `<<` correctly.
fn mark_set(event: Event<'_>) -> Event<'_> {
    match event {
        Event::MappingStart(anchor, Some(tag)) if is_core_set_tag(&tag) => {
            Event::MappingStart(anchor, Some(Cow::Owned(set_marker_tag())))
        }
        other => other,
    }
}

/// Tags every plain `<<` key with a unique marker tag and records its source position.
///
/// The loader collapses equal keys, which would hide an invalid earlier `<<` value from
/// [`canonicalize`]. The tag only makes the keys unequal; the scalar text stays `<<`, so an
/// alias to such a key elsewhere still resolves to the plain string. The marker handle contains
/// NUL, which input validation rejects.
#[derive(Default)]
struct MergeKeyTagger {
    tracker: MergeKeyTracker,
    positions: MergeKeyPositions,
}

impl MergeKeyTagger {
    fn marked<'a>(&mut self, text: Cow<'a, str>, anchor: usize, span: Span) -> Event<'a> {
        let tag = Cow::Owned(merge_key_tag(self.positions.record(span)));
        Event::Scalar(text, ScalarStyle::Plain, anchor, Some(tag))
    }

    fn tag<'a>(&mut self, event: Event<'a>, span: Span) -> Event<'a> {
        if self.tracker.observe(&event) != Some(NodeRole::MergeKey) {
            return event;
        }
        match event {
            Event::Scalar(text, _, anchor, _) => self.marked(text, anchor, span),
            _ => self.marked(Cow::Borrowed("<<"), 0, span),
        }
    }
}

/// Canonicalizes a merge value, keeping the marker on a `!!set` source (and on sequence items)
/// so that [`MergeTarget::classify`](crate::merge::MergeTarget::classify) rejects it.
fn canonicalize_merge_source(raw: Value, blame: &mut Blame) -> Result<Value, MergeError> {
    match raw {
        Value::Sequence(items) => items
            .into_iter()
            .map(|item| canonicalize_set_source(item, blame))
            .collect::<Result<_, _>>()
            .map(Value::Sequence),
        other => canonicalize_set_source(other, blame),
    }
}

fn canonicalize_set_source(raw: Value, blame: &mut Blame) -> Result<Value, MergeError> {
    match raw {
        Value::Tagged(tag, mut inner) if is_set_marker(&tag) => {
            canonicalize_in_place(&mut inner, SetElements::Yes, blame)?;
            Ok(Value::Tagged(tag, inner))
        }
        other => canonicalize_value(other, blame),
    }
}

fn canonicalize_tagged(slot: &mut Value, blame: &mut Blame) -> Result<(), MergeError> {
    let Value::Tagged(tag, inner) = std::mem::replace(slot, Value::Value(ScalarOwned::Null)) else {
        return Ok(());
    };
    if let Some(coerced) = coerce_tagged_scalar(&tag, &inner) {
        *slot = coerced;
        return Ok(());
    }
    *slot = *inner;
    let set = if is_set_marker(&tag) {
        SetElements::Yes
    } else {
        SetElements::No
    };
    canonicalize_in_place(slot, set, blame)
}

/// Canonicalize a non-collection, non-tagged node.
fn canonicalize_scalar(slot: &mut Value) {
    let value = std::mem::replace(slot, Value::Value(ScalarOwned::Null));
    *slot = match value {
        Value::Representation(s, style, tag) => {
            let resolved = resolve_scalar(&s, style, tag.as_ref());
            match resolved {
                // `Str` borrows all of `s`, so the owned text is reused.
                ResolvedScalar::Str(_) => Value::Value(ScalarOwned::String(s)),
                // A non-core tag survives so the emitter can write it back.
                ResolvedScalar::BigInt(big) => Value::Representation(
                    big.retained_text().into_owned(),
                    ScalarStyle::Plain,
                    tag.filter(|t| !t.is_yaml_core_schema()),
                ),
                other => scalar_to_value(other),
            }
        }
        Value::Value(ScalarOwned::String(ref s)) => match s.as_str() {
            "True" | "TRUE" => Value::Value(ScalarOwned::Boolean(true)),
            "False" | "FALSE" => Value::Value(ScalarOwned::Boolean(false)),
            "Null" | "NULL" => Value::Value(ScalarOwned::Null),
            _ => value,
        },
        other => other,
    };
}

/// Converts a resolved scalar to its owned core value; strings are copied.
///
/// Integers beyond `i64` become a plain `Value::Representation` holding their
/// [retained text](crate::BigInt::retained_text).
pub(crate) fn scalar_to_value(resolved: ResolvedScalar<'_>) -> Value {
    Value::Value(match resolved {
        ResolvedScalar::Null => ScalarOwned::Null,
        ResolvedScalar::Bool(b) => ScalarOwned::Boolean(b),
        ResolvedScalar::Int(i) => ScalarOwned::Integer(i),
        ResolvedScalar::Float(f) => ScalarOwned::FloatingPoint(f.into()),
        ResolvedScalar::BigInt(big) => {
            return Value::Representation(
                big.retained_text().into_owned(),
                ScalarStyle::Plain,
                None,
            );
        }
        ResolvedScalar::Str(s) => ScalarOwned::String(s.into()),
    })
}

/// Coerce a core-schema-tagged string scalar; `None` when the tag is not a core-schema tag.
fn coerce_tagged_scalar(tag: &Tag, inner: &Value) -> Option<Value> {
    if !tag.is_yaml_core_schema() {
        return None;
    }
    let Value::Value(ScalarOwned::String(s)) = inner else {
        return None;
    };
    let resolved = resolve_scalar(s, ScalarStyle::Plain, Some(tag));
    Some(scalar_to_value(resolved))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(merged_entries(yaml, "merged"), ["x: 1", "y: 2", "z: 3"]);
    }

    fn sub_mapping(doc: &Value, key: &str) -> Map {
        let Value::Mapping(root) = doc else {
            panic!("expected mapping")
        };
        let Value::Mapping(m) = root[&Value::Value(ScalarOwned::String(key.into()))].clone() else {
            panic!("expected mapping")
        };
        m
    }

    fn entry_texts(m: &Map) -> Vec<String> {
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
            Value::Value(ScalarOwned::String(s)) => s.clone(),
            Value::Value(ScalarOwned::Integer(i)) => i.to_string(),
            Value::Value(ScalarOwned::Null) => "null".to_owned(),
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
            "merge key `<<` requires a mapping or a sequence of mappings at line 2, column 3 (document 1)"
        );
    }

    #[test]
    fn test_merge_error_document_index() {
        let yaml = "a: 1\n---\nb: 2\n---\nm:\n  <<: 1\n";
        let budget = StreamBudget::new(ParseLimits::default());
        for err in [
            Parser::parse_str(yaml).unwrap_err(),
            Parser::parse_all(yaml).unwrap_err(),
            Parser::parse_chunk_with_budget(yaml, &budget).unwrap_err(),
        ] {
            assert_eq!(err.document_index(), Some(2));
            assert!(err.to_string().ends_with("(document 3)"), "{err}");
        }
    }

    #[test]
    fn test_relocated_shifts_merge_document() {
        let err = Parser::parse_all("m: {<<: 1}\n")
            .unwrap_err()
            .relocated(4, 20, 3);
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
    fn test_canonicalize_without_positions_reports_bare_error() {
        let key = Value::Representation("<<".into(), ScalarStyle::Plain, None);
        let one = Value::Representation("1".into(), ScalarStyle::Plain, None);
        let map = Value::Mapping(Map::from_iter([(key, one)]));
        assert_eq!(canonicalize(map).unwrap_err(), MergeError::NotMapping);
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
    fn test_hand_built_nul_spelling_is_an_ordinary_key() {
        let key = Value::Representation("<<\0".into(), ScalarStyle::Plain, None);
        let value = Value::Mapping(Map::from_iter([(
            key,
            Value::Value(ScalarOwned::Integer(1)),
        )]));
        let Value::Mapping(map) = canonicalize(value).unwrap() else {
            panic!("expected mapping")
        };
        assert_eq!(entry_texts(&map), ["<<\0: 1"]);
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
        let Value::Mapping(n) =
            sub_mapping(&doc, "m")[&Value::Value(ScalarOwned::String("n".into()))].clone()
        else {
            panic!("expected mapping")
        };
        assert_eq!(n.len(), 1);
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
            matches!(v, Value::Representation(ref s, _, None) if s == "9223372036854775808"),
            "i64::MAX+1 should stay Representation, got {v:?}"
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
            matches!(v, Value::Representation(ref s, ScalarStyle::Plain, None) if s == "99999999999999999999"),
            "+overflow should be canonical Representation, got {v:?}"
        );
    }

    #[test]
    fn test_large_integer_preserved_as_representation() {
        let big =
            "99999999999999999999999999999999999999999999999999999999999999999999999999999999";
        let v = get_mapping_val(&format!("x: {big}"), "x");
        assert!(
            matches!(v, Value::Representation(ref s, _, None) if s == big),
            "got {v:?}"
        );
    }

    #[test]
    fn test_quoted_large_integer_stays_string() {
        let v = get_mapping_val("x: \"9223372036854775808\"", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(ref s)) if s == "9223372036854775808"),
            "got {v:?}"
        );
    }

    #[test]
    fn test_tagged_large_integer_is_untagged_representation() {
        let v = get_mapping_val("x: !!int 9223372036854775808", "x");
        assert!(
            matches!(v, Value::Representation(ref s, ScalarStyle::Plain, None) if s == "9223372036854775808"),
            "got {v:?}"
        );
    }

    #[test]
    fn test_custom_tag_on_big_integer_is_kept_with_source_text() {
        let v = get_mapping_val("x: !foo 0xFFFFFFFFFFFFFFFFFF", "x");
        assert!(
            matches!(v, Value::Representation(ref s, ScalarStyle::Plain, Some(_)) if s == "0xFFFFFFFFFFFFFFFFFF"),
            "got {v:?}"
        );
    }

    #[test]
    fn test_canonicalize_tagged_wrapper_large_integer() {
        let tag = Tag {
            handle: "tag:yaml.org,2002:".into(),
            suffix: "int".into(),
        };
        let inner = Value::Value(ScalarOwned::String("9223372036854775808".into()));
        let v = canonicalize(Value::Tagged(tag, Box::new(inner))).unwrap();
        assert!(
            matches!(v, Value::Representation(ref s, ScalarStyle::Plain, None) if s == "9223372036854775808"),
            "got {v:?}"
        );
    }

    #[test]
    fn test_tagged_str_large_integer_is_string() {
        let v = get_mapping_val("x: !!str 9223372036854775808", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(ref s)) if s == "9223372036854775808"),
            "got {v:?}"
        );
    }

    #[test]
    fn test_large_integer_as_mapping_key_stays_representation() {
        let root = Parser::parse_str("9223372036854775808: x")
            .unwrap()
            .unwrap();
        let Value::Mapping(map) = root else {
            panic!("expected mapping")
        };
        let key = map.keys().next().unwrap();
        assert!(
            matches!(key, Value::Representation(s, _, None) if s == "9223372036854775808"),
            "got {key:?}"
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
            matches!(v, Value::Representation(ref s, _, None) if s == big),
            "got {v:?}"
        );
    }

    #[test]
    fn test_hex_and_octal_overflow_keep_source_text_in_values() {
        for (raw, expected) in [
            ("0x8000000000000000", "0x8000000000000000"),
            ("-0x8000000000000001", "-0x8000000000000001"),
            ("0o7777777777777777777777", "0o7777777777777777777777"),
            ("!!int 0xFFFFFFFFFFFFFFFFFF", "0xFFFFFFFFFFFFFFFFFF"),
            ("!!int 0o1000000000000000000000", "0o1000000000000000000000"),
            ("0XDEADBEEFDEADBEEF", "0XDEADBEEFDEADBEEF"),
            ("+0099999999999999999999", "99999999999999999999"),
        ] {
            let v = get_mapping_val(&format!("x: {raw}"), "x");
            assert!(
                matches!(v, Value::Representation(ref s, ScalarStyle::Plain, None) if s == expected),
                "{raw}: got {v:?}"
            );
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
        assert!(matches!(
            entries[0].0,
            Value::Representation(s, _, None) if s == "0xFFFFFFFFFFFFFFFFFF"
        ));
        assert_eq!(entries[0].1, &Value::Value(ScalarOwned::String("b".into())));
        assert!(matches!(
            entries[2].0,
            Value::Representation(s, _, None) if s == "0o7777777777777777777777"
        ));
    }

    #[test]
    fn test_equal_keys_collapse_at_first_position_with_last_value() {
        let Some(Value::Mapping(map)) =
            Parser::parse_str("0x10: a\nb: 1\n16: c\ntrue: x\nk: 2\nTrue: y\n").unwrap()
        else {
            unreachable!()
        };
        let entries: Vec<_> = map.iter().collect();
        let text = |s: &str| Value::Value(ScalarOwned::String(s.into()));
        assert_eq!(
            entries,
            [
                (&Value::Value(ScalarOwned::Integer(16)), &text("c")),
                (&text("b"), &Value::Value(ScalarOwned::Integer(1))),
                (&Value::Value(ScalarOwned::Boolean(true)), &text("y")),
                (&text("k"), &Value::Value(ScalarOwned::Integer(2))),
            ]
        );
    }

    #[test]
    fn test_radix_big_integer_keys_collapse_through_merge_and_alias() {
        let input = "base: &b\n  0xFFFFFFFFFFFFFFFFFF: from-base\n  other: 1\nchild:\n  <<: *b\n  4722366482869645213695: explicit\n";
        let Some(Value::Mapping(map)) = Parser::parse_str(input).unwrap() else {
            unreachable!()
        };
        let child = map
            .get(&Value::Value(ScalarOwned::String("child".into())))
            .unwrap();
        let Value::Mapping(child) = child else {
            unreachable!()
        };
        assert_eq!(child.len(), 2);
        let (key, value) = child.iter().next().unwrap();
        assert!(matches!(
            key,
            Value::Representation(s, _, None) if s == "0xFFFFFFFFFFFFFFFFFF"
        ));
        assert_eq!(value, &Value::Value(ScalarOwned::String("explicit".into())));
    }

    #[test]
    fn test_hex_min_i64_is_integer() {
        let v = get_mapping_val("x: -0x8000000000000000", "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::Integer(i64::MIN))),
            "got {v:?}"
        );
    }

    #[test]
    fn test_hex_beyond_bit_cap_stays_string() {
        let raw = format!("0x1{}", "0".repeat(3571));
        let v = get_mapping_val(&format!("x: {raw}"), "x");
        assert!(
            matches!(v, Value::Value(ScalarOwned::String(ref s)) if *s == raw),
            "got {v:?}"
        );
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
        assert!(matches!(key, Value::Representation(s, _, None) if s == "99999999999999999999"));
        assert!(matches!(value, Value::Value(ScalarOwned::String(s)) if s == "c"));
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

    #[test]
    fn test_parse_chunk_keeps_bom_that_parse_all_strips() {
        let budget = StreamBudget::new(ParseLimits::default());
        let key_of = |docs: Vec<Value>| {
            let Some(Value::Mapping(map)) = docs.into_iter().next() else {
                panic!("expected mapping");
            };
            let Some(Value::Value(ScalarOwned::String(key))) = map.keys().next().cloned() else {
                panic!("expected string key");
            };
            key
        };
        let stripped = Parser::parse_all_with_budget("\u{FEFF}a: 1", &budget).unwrap();
        assert_eq!(key_of(stripped), "a");
        let kept = Parser::parse_chunk_with_budget("\u{FEFF}a: 1", &budget).unwrap();
        assert_eq!(key_of(kept), "\u{FEFF}a");
    }

    #[test]
    fn test_parse_chunk_bom_only_is_string_document() {
        let budget = StreamBudget::new(ParseLimits::default());
        let docs = Parser::parse_chunk_with_budget("\u{FEFF}", &budget).unwrap();
        assert_eq!(
            docs,
            vec![Value::Value(ScalarOwned::String("\u{FEFF}".into()))]
        );
        let docs = Parser::parse_all_with_budget("\u{FEFF}", &budget).unwrap();
        assert_eq!(docs, vec![Value::Value(ScalarOwned::Null)]);
    }

    #[test]
    fn test_parse_all_strips_only_one_of_double_bom() {
        let budget = StreamBudget::new(ParseLimits::default());
        let docs = Parser::parse_all_with_budget("\u{FEFF}\u{FEFF}", &budget).unwrap();
        assert_eq!(
            docs,
            vec![Value::Value(ScalarOwned::String("\u{FEFF}".into()))]
        );
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
