//! Resolved YAML data model produced by the parser and consumed by the emitter.
//!
//! [`Value`] holds only resolved data: scalars are typed under the YAML 1.2 core schema, tags and
//! anchors are consumed while loading, and merge keys are already applied. No parser or emitter
//! type appears in this API.

use std::borrow::Cow;
use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher, RandomState};
use std::sync::LazyLock;

use indexmap::{IndexMap, IndexSet};

use crate::events::ScalarStyle;
use crate::scalar::{BigIntRef, IntRadix, ResolvedScalar, resolve_scalar};

/// A resolved YAML node.
///
/// The enum is exhaustive on purpose: adding a variant must break every consumer at compile time.
/// `Value` is [`Eq`] and [`Hash`] so it can key a [`Mapping`]; floats compare by normalized value
/// (see [`Float`]). Mappings and sets compare and hash without regard to entry order, as YAML
/// defines them; sequences are ordered.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{Parser, Value};
///
/// let doc = Parser::parse_str("n: 1\nok: true")?.unwrap();
/// let Value::Mapping(map) = doc else { unreachable!() };
/// assert_eq!(map.get(&Value::String("n".into())), Some(&Value::Int(1)));
/// assert_eq!(map.get(&Value::String("ok".into())), Some(&Value::Bool(true)));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    /// Null (`~`, `null`, or an empty node).
    Null,
    /// Boolean.
    Bool(bool),
    /// Integer that fits in `i64`.
    Int(i64),
    /// Integer outside `i64`.
    BigInt(BigInt),
    /// Floating-point number, including `.inf` and `.nan`.
    Float(Float),
    /// String.
    String(String),
    /// Ordered list of nodes.
    Sequence(Vec<Self>),
    /// Insertion-ordered key/value pairs.
    Mapping(Mapping),
    /// Insertion-ordered unique members of a `!!set`.
    Set(Set),
}

impl Value {
    /// Returns the text of a scalar used as a mapping key in string-keyed formats (JSON, JS objects).
    ///
    /// Null, booleans, integers and floats use their canonical text (floats as `f64` `Display`,
    /// so `1` and `1.0` share a key); a big integer uses its canonical decimal form.
    /// Returns `None` for sequences and mappings.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{Parser, Value};
    ///
    /// let Some(Value::Mapping(map)) = Parser::parse_str("+0x8000000000000000: x")? else {
    ///     unreachable!()
    /// };
    /// let key = map.keys().next().unwrap();
    /// assert_eq!(key.key_text().as_deref(), Some("9223372036854775808"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn key_text(&self) -> Option<Cow<'_, str>> {
        Some(match self {
            Self::Null => Cow::Borrowed("null"),
            Self::Bool(b) => Cow::Borrowed(if *b { "true" } else { "false" }),
            Self::Int(i) => Cow::Owned(i.to_string()),
            Self::BigInt(big) => Cow::Borrowed(big.canonical()),
            Self::Float(f) => Cow::Owned(f.get().to_string()),
            Self::String(s) => Cow::Borrowed(s),
            Self::Sequence(_) | Self::Mapping(_) | Self::Set(_) => return None,
        })
    }
}

/// Longest key prefix, in characters, that [`quote_key`] shows.
const MAX_SHOWN_KEY_CHARS: usize = 64;

/// Renders a mapping key for an error message: escaped like a Rust string and cut after 64
/// characters, so control characters and huge keys cannot reach a terminal or flood a log.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::value::quote_key;
///
/// assert_eq!(quote_key("a\u{1b}b"), r#""a\u{1b}b""#);
/// assert_eq!(quote_key(&"k".repeat(100)), format!("\"{}...\"", "k".repeat(64)));
/// ```
#[must_use]
pub fn quote_key(text: &str) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(MAX_SHOWN_KEY_CHARS).collect();
    let mut quoted = format!("{head:?}");
    if chars.next().is_some() {
        quoted.insert_str(quoted.len() - 1, "...");
    }
    quoted
}

impl From<ResolvedScalar<'_>> for Value {
    fn from(resolved: ResolvedScalar<'_>) -> Self {
        match resolved {
            ResolvedScalar::Null => Self::Null,
            ResolvedScalar::Bool(b) => Self::Bool(b),
            ResolvedScalar::Int(i) => Self::Int(i),
            ResolvedScalar::BigInt(big) => Self::BigInt(big.into()),
            ResolvedScalar::Float(f) => Self::Float(f.into()),
            ResolvedScalar::Str(s) => Self::String(s.to_owned()),
        }
    }
}

/// A YAML float, optionally remembering the text it was written as.
///
/// Equality and hashing use the normalized value only: every NaN is equal to every other NaN and
/// `-0.0 == 0.0`. The remembered text is carried so a float read from JSON keeps its spelling
/// (`1.0E+5`, `0.1000000000000000055`) when emitted as YAML; the YAML loader never sets it. The
/// text keeps its digits but is normalized for YAML 1.1 readers such as `PyYAML`: a dot-less
/// mantissa gets `.0` and an unsigned exponent gets `+` (`1e5` becomes `1.0e+5`), and a leading
/// dot gets a `0` (`-.5` becomes `-0.5`). As a result `a == b` does not imply that `a` and `b`
/// emit identically. There is deliberately no `Ord`, because NaN has no place in a total order.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::Float;
///
/// let plain = Float::from(100_000.0);
/// let spelled = Float::parse("1.0E5").unwrap();
/// assert_eq!(plain, spelled);
/// assert_eq!(plain.to_string(), "100000.0");
/// assert_eq!(spelled.to_string(), "1.0E+5");
/// assert_eq!(Float::from(1e300).to_string(), "1.0e+300");
/// assert_eq!(Float::from(f64::NAN), Float::from(-f64::NAN));
/// ```
#[derive(Debug, Clone)]
pub struct Float {
    value: f64,
    text: Option<Box<str>>,
}

impl Float {
    /// Creates a float without a remembered spelling.
    #[must_use]
    pub const fn new(value: f64) -> Self {
        Self { value, text: None }
    }

    /// Parses `text` as a YAML core-schema float and remembers its spelling.
    ///
    /// Returns `None` when `text` does not resolve to a float (for example an integer or a
    /// string), so the remembered text is always valid plain YAML that reads back as this value.
    /// The spelling is normalized for YAML 1.1 readers (see [`Float`]) and dropped when it equals
    /// the default formatting.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::Float;
    ///
    /// assert_eq!(Float::parse("2.50").unwrap().to_string(), "2.50");
    /// assert_eq!(Float::parse("2.5").unwrap().to_string(), "2.5");
    /// assert_eq!(Float::parse("-.5e3").unwrap().to_string(), "-0.5e+3");
    /// assert!(Float::parse("12").is_none());
    /// assert!(Float::parse("1.5x").is_none());
    /// ```
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let ResolvedScalar::Float(value) = resolve_scalar(text, ScalarStyle::Plain, None) else {
            return None;
        };
        let text = normalize_float_text(text);
        let default = Self::new(value);
        Some(if default.spells(&text) {
            default
        } else {
            Self {
                value,
                text: Some(text.into()),
            }
        })
    }

    /// Returns the numeric value.
    #[must_use]
    pub const fn get(&self) -> f64 {
        self.value
    }

    pub(crate) fn spelling(&self) -> Option<&str> {
        self.text.as_deref()
    }

    fn spells(&self, text: &str) -> bool {
        use fmt::Write as _;
        let mut matcher = PrefixMatcher(text);
        write!(matcher, "{self}").is_ok() && matcher.0.is_empty()
    }

    fn normalized_bits(&self) -> u64 {
        if self.value.is_nan() {
            f64::NAN.to_bits()
        } else if self.value == 0.0 {
            0.0f64.to_bits()
        } else {
            self.value.to_bits()
        }
    }
}

/// A [`fmt::Write`] sink that fails as soon as the written text stops matching the expected one.
struct PrefixMatcher<'a>(&'a str);

impl fmt::Write for PrefixMatcher<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0 = self.0.strip_prefix(s).ok_or(fmt::Error)?;
        Ok(())
    }
}

impl From<f64> for Float {
    fn from(value: f64) -> Self {
        Self::new(value)
    }
}

impl PartialEq for Float {
    fn eq(&self, other: &Self) -> bool {
        self.normalized_bits() == other.normalized_bits()
    }
}

impl Eq for Float {}

impl Hash for Float {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.normalized_bits().hash(state);
    }
}

impl fmt::Display for Float {
    /// Writes the remembered spelling, or YAML core-schema text (`.inf`, `-.inf`, `.nan`,
    /// otherwise the shortest round-trip form that YAML 1.1 and 1.2 readers both read as a float).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.text {
            Some(text) => f.write_str(text),
            None => with_default_text(self.value, |text| f.write_str(text)),
        }
    }
}

/// Fixed buffer that holds the shortest text of any finite `f64` without allocating.
struct FloatBuf {
    bytes: [u8; 40],
    len: usize,
}

impl fmt::Write for FloatBuf {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len + text.len();
        self.bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

/// Runs `use_text` on the default YAML text of `value` (`.inf`, `-.inf`, `.nan`, otherwise the
/// shortest round-trip form normalized for YAML 1.1 readers), allocating only when the text had
/// to be rewritten.
fn with_default_text<R>(value: f64, use_text: impl FnOnce(&str) -> R) -> R {
    if value.is_nan() {
        return use_text(".nan");
    }
    if value.is_infinite() {
        return use_text(if value > 0.0 { ".inf" } else { "-.inf" });
    }
    let mut buf = FloatBuf {
        bytes: [0; 40],
        len: 0,
    };
    if fmt::write(&mut buf, format_args!("{value:?}")).is_ok()
        && let Some(text) = buf
            .bytes
            .get(..buf.len)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
    {
        return use_text(&normalize_float_text(text));
    }
    use_text(&normalize_float_text(&format!("{value:?}")))
}

/// Rewrites the text of a numeric float (not `.inf` or `.nan`) so YAML 1.1 readers also resolve it as a float: a leading dot gets
/// a `0`, a mantissa with an exponent gets a dot, and an unsigned exponent gets `+`.
fn normalize_float_text(text: &str) -> Cow<'_, str> {
    let (sign, unsigned) = text.split_at(usize::from(text.starts_with(['+', '-'])));
    let numeric = unsigned.strip_prefix('.').unwrap_or(unsigned);
    if !numeric.starts_with(|c: char| c.is_ascii_digit()) {
        return Cow::Borrowed(text);
    }
    let (mantissa, exponent) = unsigned
        .find(['e', 'E'])
        .map_or((unsigned, ""), |at| unsigned.split_at(at));
    let (marker, digits) = exponent.split_at(exponent.len().min(1));
    let leading_dot = mantissa.starts_with('.');
    let dotless = !exponent.is_empty() && !mantissa.contains('.');
    let unsigned_exponent = !exponent.is_empty() && !digits.starts_with(['+', '-']);
    if !(leading_dot || dotless || unsigned_exponent) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 3);
    out.push_str(sign);
    if leading_dot {
        out.push('0');
    }
    out.push_str(mantissa);
    if dotless {
        out.push_str(".0");
    }
    out.push_str(marker);
    if unsigned_exponent {
        out.push('+');
    }
    out.push_str(digits);
    Cow::Owned(out)
}

/// Source spelling of a hexadecimal or octal integer.
#[derive(Debug, Clone)]
struct RadixSpelling {
    radix: IntRadix,
    text: Box<str>,
}

/// An integer outside the `i64` range.
///
/// Equality and hashing use the canonical decimal text only, so `0xFF…FF`, `0xff…ff` and the
/// decimal spelling of one number are the same mapping key. The hex or octal source spelling is
/// kept for arbitrary-precision parsers such as Python's `int(text, base)`.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{BigInt, IntRadix};
///
/// let hex = BigInt::parse("0xFFFFFFFFFFFFFFFFFF").unwrap();
/// let decimal = BigInt::parse("4722366482869645213695").unwrap();
/// assert_eq!(hex, decimal);
/// assert_eq!(hex.canonical(), "4722366482869645213695");
/// assert_eq!(hex.radix(), IntRadix::Hex);
/// assert_eq!(hex.radix_text(), Some("0xFFFFFFFFFFFFFFFFFF"));
/// assert_eq!(decimal.radix_text(), None);
/// assert!(BigInt::parse("42").is_none());
/// ```
#[derive(Debug, Clone)]
pub struct BigInt {
    canonical: Box<str>,
    radix_digits: Option<Box<RadixSpelling>>,
}

impl BigInt {
    /// Parses `text` as a core-schema integer that does not fit `i64`.
    ///
    /// Returns `None` for any other text, including integers that fit `i64` and radix literals
    /// beyond the supported size.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match resolve_scalar(text, ScalarStyle::Plain, None) {
            ResolvedScalar::BigInt(big) => Some(big.into()),
            _ => None,
        }
    }

    /// Returns the value as signed decimal text without `+`, leading zeros or radix prefix.
    #[must_use]
    pub fn canonical(&self) -> &str {
        &self.canonical
    }

    /// Returns the numeral system the integer was written in.
    #[must_use]
    pub fn radix(&self) -> IntRadix {
        self.radix_digits
            .as_ref()
            .map_or(IntRadix::Decimal, |spelling| spelling.radix)
    }

    /// Returns the hex or octal source text including sign and prefix, `None` for decimal.
    ///
    /// Pair it with [`radix`](Self::radix) for `int(text, base)`-style parsers.
    #[must_use]
    pub fn radix_text(&self) -> Option<&str> {
        self.radix_digits.as_ref().map(|spelling| &*spelling.text)
    }
}

impl From<BigIntRef<'_>> for BigInt {
    fn from(big: BigIntRef<'_>) -> Self {
        let radix = big.radix();
        Self {
            canonical: big.canonical().into_owned().into_boxed_str(),
            radix_digits: match radix {
                IntRadix::Decimal => None,
                IntRadix::Hex | IntRadix::Octal => Some(Box::new(RadixSpelling {
                    radix,
                    text: big.as_str().into(),
                })),
            },
        }
    }
}

impl PartialEq for BigInt {
    fn eq(&self, other: &Self) -> bool {
        self.canonical == other.canonical
    }
}

impl Eq for BigInt {}

impl Hash for BigInt {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.canonical.hash(state);
    }
}

/// Insertion-ordered YAML mapping.
///
/// Inserting an equal key keeps the original key and its position and replaces the value, so a
/// duplicate key in a document takes the last value in the first key's place. Iteration follows
/// insertion order, but equality and hashing ignore it: two mappings with the same entries in a
/// different order are equal and hash alike, as YAML defines mapping equality.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{Mapping, Value};
///
/// let key = |s: &str| Value::String(s.into());
/// let mut map = Mapping::new();
/// map.insert(key("b"), Value::Int(1));
/// map.insert(key("a"), Value::Int(2));
/// map.insert(key("b"), Value::Int(3));
///
/// let entries: Vec<_> = map.iter().collect();
/// assert_eq!(entries, [(&key("b"), &Value::Int(3)), (&key("a"), &Value::Int(2))]);
///
/// let reordered: Mapping = [(key("a"), Value::Int(2)), (key("b"), Value::Int(3))]
///     .into_iter()
///     .collect();
/// assert_eq!(map, reordered);
/// ```
#[derive(Debug, Clone, Default)]
pub struct Mapping(Box<IndexMap<Value, Value>>);

impl Mapping {
    /// Creates an empty mapping.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty mapping with room for `capacity` entries.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self(Box::new(IndexMap::with_capacity(capacity)))
    }

    /// Returns the number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` when the mapping has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the value stored under `key`.
    #[must_use]
    pub fn get(&self, key: &Value) -> Option<&Value> {
        self.0.get(key)
    }

    /// Returns `true` when `key` is present.
    #[must_use]
    pub fn contains_key(&self, key: &Value) -> bool {
        self.0.contains_key(key)
    }

    /// Stores `value` under `key` and returns the value it replaced.
    ///
    /// An equal key that is already present keeps its position and its original spelling.
    pub fn insert(&mut self, key: Value, value: Value) -> Option<Value> {
        self.0.insert(key, value)
    }

    /// Iterates over the entries in insertion order.
    pub fn iter(&self) -> Iter<'_> {
        Iter(self.0.iter())
    }

    /// Iterates over the keys in insertion order.
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.0.keys()
    }

    /// Iterates over the values in insertion order.
    pub fn values(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.0.values()
    }
}

/// Keys the per-entry hashes of [`Mapping`] and [`Set`], so entries that an attacker can make
/// collide under a fixed hasher still sum unpredictably.
static ENTRY_HASHER: LazyLock<RandomState> = LazyLock::new(RandomState::new);

impl PartialEq for Mapping {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Mapping {}

impl Hash for Mapping {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.len());
        let hasher = &*ENTRY_HASHER;
        let sum = self
            .iter()
            .fold(0u64, |sum, entry| sum.wrapping_add(hasher.hash_one(entry)));
        state.write_u64(sum);
    }
}

impl std::ops::Index<&Value> for Mapping {
    type Output = Value;

    /// Returns the value stored under `key`.
    ///
    /// # Panics
    ///
    /// Panics when `key` is absent; use [`Mapping::get`] for a fallible lookup.
    fn index(&self, key: &Value) -> &Value {
        self.get(key).expect("key not found in mapping")
    }
}

impl FromIterator<(Value, Value)> for Mapping {
    fn from_iter<I: IntoIterator<Item = (Value, Value)>>(iter: I) -> Self {
        let iter = iter.into_iter();
        let mut map = Self::with_capacity(iter.size_hint().0);
        for (key, value) in iter {
            map.insert(key, value);
        }
        map
    }
}

/// Insertion-ordered unique members of a YAML `!!set`.
///
/// A parsed `!!set` holds its keys only; the null values YAML writes for them are implied. Equal
/// members collapse to the first one. Equality and hashing ignore member order, like
/// [`Mapping`]'s.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{Parser, Set, Value};
///
/// let Some(Value::Set(set)) = Parser::parse_str("!!set {b, a, b}")? else { unreachable!() };
/// let members: Vec<_> = set.iter().collect();
/// assert_eq!(members, [&Value::String("b".into()), &Value::String("a".into())]);
/// assert!(set.contains(&Value::String("a".into())));
/// assert_eq!(Set::new().len(), 0);
///
/// let reordered: Set = [Value::String("a".into()), Value::String("b".into())]
///     .into_iter()
///     .collect();
/// assert_eq!(set, reordered);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct Set(Box<IndexSet<Value>>);

impl Set {
    /// Creates an empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of members.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` when the set has no members.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns `true` when `member` is present.
    #[must_use]
    pub fn contains(&self, member: &Value) -> bool {
        self.0.contains(member)
    }

    /// Adds `member`; returns `false` and keeps the existing member when an equal one is present.
    pub fn insert(&mut self, member: Value) -> bool {
        self.0.insert(member)
    }

    /// Iterates over the members in insertion order.
    pub fn iter(&self) -> SetIter<'_> {
        SetIter(self.0.iter())
    }
}

impl PartialEq for Set {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Set {}

impl Hash for Set {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.len());
        let hasher = &*ENTRY_HASHER;
        let sum = self.iter().fold(0u64, |sum, member| {
            sum.wrapping_add(hasher.hash_one(member))
        });
        state.write_u64(sum);
    }
}

impl FromIterator<Value> for Set {
    fn from_iter<I: IntoIterator<Item = Value>>(iter: I) -> Self {
        let mut set = Self::new();
        for member in iter {
            set.insert(member);
        }
        set
    }
}

/// Borrowing iterator over the members of a [`Set`].
#[derive(Debug, Clone)]
pub struct SetIter<'a>(indexmap::set::Iter<'a, Value>);

impl<'a> Iterator for SetIter<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for SetIter<'_> {}

/// Owning iterator over the members of a [`Set`].
#[derive(Debug)]
pub struct SetIntoIter(indexmap::set::IntoIter<Value>);

impl Iterator for SetIntoIter {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for SetIntoIter {}

impl<'a> IntoIterator for &'a Set {
    type Item = &'a Value;
    type IntoIter = SetIter<'a>;

    fn into_iter(self) -> SetIter<'a> {
        self.iter()
    }
}

impl IntoIterator for Set {
    type Item = Value;
    type IntoIter = SetIntoIter;

    fn into_iter(self) -> SetIntoIter {
        SetIntoIter(IndexSet::into_iter(*self.0))
    }
}

/// Borrowing iterator over the entries of a [`Mapping`].
#[derive(Debug, Clone)]
pub struct Iter<'a>(indexmap::map::Iter<'a, Value, Value>);

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a Value, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for Iter<'_> {}

/// Owning iterator over the entries of a [`Mapping`].
#[derive(Debug)]
pub struct IntoIter(indexmap::map::IntoIter<Value, Value>);

impl Iterator for IntoIter {
    type Item = (Value, Value);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for IntoIter {}

impl<'a> IntoIterator for &'a Mapping {
    type Item = (&'a Value, &'a Value);
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl IntoIterator for Mapping {
    type Item = (Value, Value);
    type IntoIter = IntoIter;

    fn into_iter(self) -> IntoIter {
        IntoIter(IndexMap::into_iter(*self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;

    fn text(s: &str) -> Value {
        Value::String(s.into())
    }

    fn hash_of(v: &impl Hash) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    #[test]
    fn value_stays_small() {
        assert!(size_of::<Value>() <= 32, "{}", size_of::<Value>());
        assert_eq!(size_of::<BigInt>(), 24);
    }

    #[test]
    fn float_equality_ignores_spelling_and_nan_payload() {
        let spelled = Float::parse("1e0").unwrap();
        assert_eq!(spelled, Float::new(1.0));
        assert_eq!(hash_of(&spelled), hash_of(&Float::new(1.0)));
        assert_eq!(Float::new(f64::NAN), Float::new(-f64::NAN));
        assert_eq!(
            hash_of(&Float::new(f64::NAN)),
            hash_of(&Float::new(-f64::NAN))
        );
        assert_eq!(Float::new(-0.0), Float::new(0.0));
        assert_eq!(hash_of(&Float::new(-0.0)), hash_of(&Float::new(0.0)));
        assert_ne!(Float::new(1.0), Float::new(2.0));
    }

    #[test]
    fn float_display_is_core_schema() {
        assert_eq!(Float::new(f64::INFINITY).to_string(), ".inf");
        assert_eq!(Float::new(f64::NEG_INFINITY).to_string(), "-.inf");
        assert_eq!(Float::new(f64::NAN).to_string(), ".nan");
        assert_eq!(Float::new(1.0).to_string(), "1.0");
        assert_eq!(Float::new(-0.0).to_string(), "-0.0");
        assert_eq!(Float::new(1e300).to_string(), "1.0e+300");
        assert_eq!(Float::new(1.5e-7).to_string(), "1.5e-7");
        assert_eq!(Float::new(1e-7).to_string(), "1.0e-7");
        assert_eq!(Float::new(-2.5e20).to_string(), "-2.5e+20");
    }

    #[test]
    fn float_spelling_is_normalized_for_yaml_11_readers() {
        for (written, shown) in [
            ("1.0E5", "1.0E+5"),
            ("1e5", "1.0e+5"),
            ("-.5", "-0.5"),
            (".5", "0.5"),
            ("-.5e+5", "-0.5e+5"),
            ("1.e5", "1.e+5"),
            ("1.5E-3", "1.5E-3"),
            ("0.10", "0.10"),
            (".nan", ".nan"),
            (".NaN", ".NaN"),
            (".NAN", ".NAN"),
            (".inf", ".inf"),
            ("+.inf", "+.inf"),
            (".Inf", ".Inf"),
            (".INF", ".INF"),
            ("-.INF", "-.INF"),
            ("-.Inf", "-.Inf"),
        ] {
            assert_eq!(
                Float::parse(written).unwrap().to_string(),
                shown,
                "{written}"
            );
        }
    }

    #[test]
    fn float_text_matches_the_yaml_11_float_pattern() {
        let yaml11 = |t: &str| {
            let (m, e) = t
                .split_once(['e', 'E'])
                .map_or((t, None), |(m, e)| (m, Some(e)));
            let m = m.strip_prefix(['+', '-']).unwrap_or(m);
            let mantissa = m.split_once('.').is_some_and(|(i, f)| {
                (i.is_empty() && !f.is_empty() || i.starts_with(|c: char| c.is_ascii_digit()))
                    && i.chars().chain(f.chars()).all(|c| c.is_ascii_digit())
            });
            mantissa
                && e.is_none_or(|e| {
                    e.strip_prefix(['+', '-'])
                        .is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
                })
        };
        for v in [
            1e300,
            1.5e-7,
            1e16,
            123_456.0,
            -0.0,
            1e-300,
            f64::MAX,
            f64::MIN_POSITIVE,
        ] {
            let text = Float::new(v).to_string();
            assert!(yaml11(&text), "{text}");
        }
        for written in ["1e5", "-.5e5", "+.5", "2E10", "1.e3"] {
            let text = Float::parse(written).unwrap().to_string();
            assert!(yaml11(&text), "{written} -> {text}");
        }
    }

    #[test]
    fn float_parse_keeps_only_a_distinct_spelling() {
        assert_eq!(Float::parse("1.0E5").unwrap().to_string(), "1.0E+5");
        assert_eq!(Float::parse(".inf").unwrap().to_string(), ".inf");
        assert_eq!(Float::parse("1.5").unwrap().text, None);
        for not_float in ["", "1", "abc", "0x1", "1.5.5"] {
            assert!(Float::parse(not_float).is_none(), "{not_float:?}");
        }
    }

    #[test]
    fn big_int_equality_is_by_value() {
        let hex = BigInt::parse("0xFFFFFFFFFFFFFFFFFF").unwrap();
        let lower = BigInt::parse("0xffffffffffffffffff").unwrap();
        let decimal = BigInt::parse("4722366482869645213695").unwrap();
        let padded = BigInt::parse("+00004722366482869645213695").unwrap();
        for other in [&lower, &decimal, &padded] {
            assert_eq!(&hex, other);
            assert_eq!(hash_of(&hex), hash_of(other));
        }
        assert_ne!(hex, BigInt::parse("-4722366482869645213695").unwrap());
    }

    #[test]
    fn big_int_keeps_radix_spelling() {
        let octal = BigInt::parse("-0o7777777777777777777777").unwrap();
        assert_eq!(octal.radix(), IntRadix::Octal);
        assert_eq!(octal.radix_text(), Some("-0o7777777777777777777777"));
        assert_eq!(octal.canonical(), "-73786976294838206463");
        assert_eq!(
            BigInt::parse("99999999999999999999").unwrap().radix(),
            IntRadix::Decimal
        );
    }

    #[test]
    fn mapping_insert_keeps_first_key_position_and_last_value() {
        let big = |s: &str| Value::BigInt(BigInt::parse(s).unwrap());
        let mut map = Mapping::new();
        map.insert(big("0xFFFFFFFFFFFFFFFFFF"), Value::Int(1));
        map.insert(text("x"), Value::Int(2));
        map.insert(big("4722366482869645213695"), Value::Int(3));
        assert_eq!(map.len(), 2);
        let Some((Value::BigInt(first), value)) = map.iter().next() else {
            panic!("big int key first");
        };
        assert_eq!(first.radix(), IntRadix::Hex);
        assert_eq!(value, &Value::Int(3));
    }

    fn pairs(entries: &[(&str, Value)]) -> Mapping {
        entries
            .iter()
            .map(|(key, value)| (text(key), value.clone()))
            .collect()
    }

    #[test]
    fn mapping_equality_and_hash_ignore_order() {
        let ab = pairs(&[("a", Value::Null), ("b", Value::Int(1))]);
        let ba = pairs(&[("b", Value::Int(1)), ("a", Value::Null)]);
        assert_eq!(ab, ba);
        assert_eq!(hash_of(&ab), hash_of(&ba));
        assert_eq!(
            ab.iter().next().map(|(key, _)| key),
            Some(&text("a")),
            "iteration keeps insertion order"
        );
        assert_ne!(ab, pairs(&[("a", Value::Null), ("b", Value::Int(2))]));
        assert_ne!(ab, pairs(&[("a", Value::Null)]));
        assert_eq!(ab.get(&text("b")), Some(&Value::Int(1)));
    }

    #[test]
    fn nested_mapping_keys_and_values_ignore_order() {
        let inner = |reversed: bool| {
            let mut entries = [("x", Value::Int(1)), ("y", Value::Int(2))];
            if reversed {
                entries.reverse();
            }
            Value::Mapping(pairs(&entries))
        };
        let a = Value::Mapping(pairs(&[("m", inner(false)), ("n", Value::Null)]));
        let b = Value::Mapping(pairs(&[("n", Value::Null), ("m", inner(true))]));
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        let mut keyed = Mapping::new();
        keyed.insert(inner(false), Value::Int(7));
        assert_eq!(keyed.get(&inner(true)), Some(&Value::Int(7)));
    }

    fn mapping_of(entries: &[(i64, i64)]) -> Mapping {
        entries
            .iter()
            .map(|&(k, v)| (Value::Int(k), Value::Int(v)))
            .collect()
    }

    proptest::proptest! {
        #[test]
        fn permuted_entries_stay_equal_and_hash_alike(
            entries in proptest::collection::vec((0i64..40, 0i64..40), 0..12),
            rotate in 0usize..12,
            reverse in proptest::bool::ANY,
        ) {
            let original = mapping_of(&entries);
            let mut unique: Vec<(i64, i64)> = original
                .iter()
                .map(|(k, v)| match (k, v) {
                    (Value::Int(k), Value::Int(v)) => (*k, *v),
                    _ => unreachable!(),
                })
                .collect();
            if !unique.is_empty() {
                let len = unique.len();
                unique.rotate_left(rotate % len);
            }
            if reverse {
                unique.reverse();
            }
            let permuted = mapping_of(&unique);
            proptest::prop_assert_eq!(&original, &permuted);
            proptest::prop_assert_eq!(hash_of(&original), hash_of(&permuted));
            if let Some(&(k, v)) = unique.first() {
                let mut changed = unique.clone();
                changed[0] = (k, v + 100);
                proptest::prop_assert_ne!(&original, &mapping_of(&changed));
            }

            let set_a: Set = unique.iter().map(|&(k, _)| Value::Int(k)).collect();
            let set_b: Set = entries.iter().map(|&(k, _)| Value::Int(k)).collect();
            proptest::prop_assert_eq!(&set_a, &set_b);
            proptest::prop_assert_eq!(hash_of(&set_a), hash_of(&set_b));
        }
    }

    #[test]
    fn nan_keys_and_set_keys_are_found_whatever_the_order() {
        let nan = Value::Float(Float::new(f64::NAN));
        let mut map = Mapping::new();
        map.insert(nan, Value::Int(1));
        assert_eq!(
            map.get(&Value::Float(Float::new(-f64::NAN))),
            Some(&Value::Int(1))
        );

        let ab: Set = [text("a"), text("b")].into_iter().collect();
        let ba: Set = [text("b"), text("a")].into_iter().collect();
        let mut keyed = Mapping::new();
        keyed.insert(Value::Set(ab), Value::Int(7));
        assert_eq!(keyed.get(&Value::Set(ba)), Some(&Value::Int(7)));
    }

    #[test]
    fn sequences_stay_ordered() {
        let ab = Value::Sequence(vec![text("a"), text("b")]);
        let ba = Value::Sequence(vec![text("b"), text("a")]);
        assert_ne!(ab, ba);
    }

    #[test]
    fn set_equality_and_hash_ignore_order() {
        let ab: Set = [text("a"), text("b")].into_iter().collect();
        let ba: Set = [text("b"), text("a")].into_iter().collect();
        assert_eq!(ab, ba);
        assert_eq!(hash_of(&ab), hash_of(&ba));
        assert_ne!(ab, std::iter::once(text("a")).collect());
        assert_ne!(ab, [text("a"), text("c")].into_iter().collect());
    }

    #[test]
    fn entries_with_swapped_keys_and_values_hash_apart() {
        let forward = pairs(&[("a", text("b")), ("b", text("a"))]);
        let swapped = pairs(&[("a", text("a")), ("b", text("b"))]);
        assert_ne!(forward, swapped);
        assert_ne!(hash_of(&forward), hash_of(&swapped));
    }

    #[test]
    fn entry_hashes_do_not_cancel_for_repeated_structure() {
        let mut seen = std::collections::HashSet::new();
        for i in 0..2000_i64 {
            let map: Mapping = [
                (Value::Int(i), Value::Int(i)),
                (Value::Int(i + 1), Value::Int(i)),
            ]
            .into_iter()
            .collect();
            seen.insert(hash_of(&map));
        }
        assert!(seen.len() > 1990, "{}", seen.len());
    }

    #[test]
    fn key_text_covers_scalars_only() {
        assert_eq!(Value::Null.key_text().as_deref(), Some("null"));
        assert_eq!(Value::Bool(true).key_text().as_deref(), Some("true"));
        assert_eq!(Value::Int(-3).key_text().as_deref(), Some("-3"));
        assert_eq!(
            Value::Float(Float::new(1.0)).key_text().as_deref(),
            Some("1")
        );
        assert_eq!(
            Value::Float(Float::new(1.5)).key_text().as_deref(),
            Some("1.5")
        );
        assert_eq!(text("s").key_text().as_deref(), Some("s"));
        assert!(Value::Sequence(Vec::new()).key_text().is_none());
        assert!(Value::Mapping(Mapping::new()).key_text().is_none());
    }

    #[test]
    fn key_text_of_big_int_is_canonical() {
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
            assert_eq!(key.key_text().as_deref(), Some(expected), "{raw}");
        }
    }

    #[test]
    fn quoted_big_int_key_stays_a_string() {
        let Some(Value::Mapping(map)) =
            crate::Parser::parse_str("\"+99999999999999999999\": x").unwrap()
        else {
            unreachable!()
        };
        let key = map.keys().next().unwrap();
        assert_eq!(key, &text("+99999999999999999999"));
    }

    #[test]
    fn parse_keeps_non_default_spellings() {
        for (text, shown) in [
            (".NaN", ".NaN"),
            (".NAN", ".NAN"),
            ("+.inf", "+.inf"),
            ("-.Inf", "-.Inf"),
            (".INF", ".INF"),
            ("1e400", "1.0e+400"),
            ("-1e400", "-1.0e+400"),
            ("5e-324", "5.0e-324"),
            ("1e-400", "1.0e-400"),
            ("1.", "1."),
            (".5", "0.5"),
            ("+1.5", "+1.5"),
            ("1.50", "1.50"),
            ("1.0E5", "1.0E+5"),
            ("-0.0e0", "-0.0e+0"),
        ] {
            let float = Float::parse(text).unwrap_or_else(|| panic!("{text} is a float"));
            assert_eq!(float.to_string(), shown, "{text}");
        }
    }

    #[test]
    fn parse_drops_spelling_equal_to_default_formatting() {
        for text in [".nan", ".inf", "-.inf", "1.5", "0.0", "-0.0", "100000.0"] {
            let float = Float::parse(text).unwrap();
            assert!(float.spelling().is_none(), "{text}");
            assert_eq!(float.to_string(), text);
        }
        assert!(Float::parse("1.50").unwrap().spelling().is_some());
        assert!(Float::parse(".NaN").unwrap().spelling().is_some());
    }

    #[test]
    fn parse_special_values_resolve() {
        assert!(Float::parse(".NAN").unwrap().get().is_nan());
        assert_eq!(Float::parse("+.inf").unwrap(), Float::new(f64::INFINITY));
        assert_eq!(
            Float::parse("-.Inf").unwrap(),
            Float::new(f64::NEG_INFINITY)
        );
        assert_eq!(Float::parse("1e400").unwrap(), Float::new(f64::INFINITY));
        assert_eq!(
            Float::parse("-1e400").unwrap(),
            Float::new(f64::NEG_INFINITY)
        );
        assert_eq!(Float::parse("5e-324").unwrap(), Float::new(5e-324));
        assert_eq!(Float::parse("1e-400").unwrap(), Float::new(0.0));
    }
}
