//! Validated value types shared by rule option structs.

use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU8;
use std::str::FromStr;

use crate::echo::{KEY_LIMIT, MESSAGE_LIMIT, echo};

use serde::de::{self, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Largest accepted indentation size in spaces.
const MAX_INDENT_SIZE: u8 = 16;

/// A numeric bound on a count of spaces or lines, or no bound at all.
///
/// Mirrors the yamllint convention where `-1` disables the check. Used for both
/// minimum and maximum options; the option name gives the direction.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::Limit;
///
/// assert!(Limit::Max(2).exceeded_by(3));
/// assert!(!Limit::Disabled.exceeded_by(1000));
/// assert!(Limit::Max(2).unmet_by(1));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// The check is skipped (`-1` in configuration).
    Disabled,
    /// The bound is this many spaces or lines.
    Max(u32),
}

impl Limit {
    /// Returns `true` when `count` is above this bound, treating the bound as a maximum.
    #[must_use]
    pub const fn exceeded_by(self, count: usize) -> bool {
        match self {
            Self::Disabled => false,
            Self::Max(bound) => count > bound as usize,
        }
    }

    /// Returns `true` when `count` is below this bound, treating the bound as a minimum.
    #[must_use]
    pub const fn unmet_by(self, count: usize) -> bool {
        match self {
            Self::Disabled => false,
            Self::Max(bound) => count < bound as usize,
        }
    }
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => f.write_str("-1"),
            Self::Max(bound) => write!(f, "{bound}"),
        }
    }
}

/// Converts a configuration integer where `-1` means "unset" into `None`.
fn unset_or_count<E: de::Error>(value: i64) -> Result<Option<u32>, E> {
    match value {
        -1 => Ok(None),
        0.. => u32::try_from(value).map(Some).map_err(|_| {
            E::invalid_value(
                Unexpected::Signed(value),
                &"an integer that fits in 32 bits",
            )
        }),
        _ => Err(E::invalid_value(
            Unexpected::Signed(value),
            &"-1 or a non-negative integer",
        )),
    }
}

struct SignedCountVisitor;

impl Visitor<'_> for SignedCountVisitor {
    type Value = Option<u32>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("-1 or a non-negative integer")
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        unset_or_count(value)
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        i64::try_from(value)
            .map_err(|_| E::invalid_value(Unexpected::Unsigned(value), &self))
            .and_then(unset_or_count)
    }
}

impl Serialize for Limit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Disabled => serializer.serialize_i64(-1),
            Self::Max(bound) => serializer.serialize_i64(i64::from(*bound)),
        }
    }
}

impl<'de> Deserialize<'de> for Limit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_any(SignedCountVisitor)
            .map(|bound| bound.map_or(Self::Disabled, Self::Max))
    }
}

/// Space limit for the inside of an empty flow collection.
///
/// In yamllint `-1` means "use the limit for non-empty collections", which is
/// [`EmptyInsideLimit::Inherit`]; it does not disable the check.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::{EmptyInsideLimit, Limit};
///
/// assert_eq!(EmptyInsideLimit::Inherit.resolve(Limit::Max(1)), Limit::Max(1));
/// assert_eq!(EmptyInsideLimit::Spaces(0).resolve(Limit::Max(1)), Limit::Max(0));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyInsideLimit {
    /// Fall back to the non-empty limit (`-1` in configuration).
    Inherit,
    /// Exactly this bound applies to empty collections.
    Spaces(u32),
}

impl EmptyInsideLimit {
    /// Returns the limit that applies to empty collections given the non-empty one.
    #[must_use]
    pub const fn resolve(self, inherited: Limit) -> Limit {
        match self {
            Self::Inherit => inherited,
            Self::Spaces(bound) => Limit::Max(bound),
        }
    }
}

impl Serialize for EmptyInsideLimit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Inherit => serializer.serialize_i64(-1),
            Self::Spaces(bound) => serializer.serialize_i64(i64::from(*bound)),
        }
    }
}

impl<'de> Deserialize<'de> for EmptyInsideLimit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_any(SignedCountVisitor)
            .map(|bound| bound.map_or(Self::Inherit, Self::Spaces))
    }
}

/// Number of spaces per indentation level, between 1 and 16.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::IndentSize;
///
/// assert_eq!(IndentSize::default().get(), 2);
/// assert_eq!("4".parse::<IndentSize>().unwrap().get(), 4);
/// assert!(IndentSize::try_from(0u64).is_err());
/// assert!(IndentSize::try_from(17u64).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndentSize(NonZeroU8);

impl IndentSize {
    /// Returns the size in spaces.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0.get() as usize
    }
}

impl IndentSize {
    /// Converts a width to a valid size, raising 0 to 1 and lowering values above 16 to 16.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_linter::config::IndentSize;
    ///
    /// assert_eq!(IndentSize::saturating_from_u8(0).get(), 1);
    /// assert_eq!(IndentSize::saturating_from_u8(4).get(), 4);
    /// assert_eq!(IndentSize::saturating_from_u8(200).get(), 16);
    /// ```
    #[must_use]
    pub const fn saturating_from_u8(value: u8) -> Self {
        let clamped = if value > MAX_INDENT_SIZE {
            MAX_INDENT_SIZE
        } else {
            value
        };
        match NonZeroU8::new(clamped) {
            Some(size) => Self(size),
            None => Self(NonZeroU8::MIN),
        }
    }
}

impl Default for IndentSize {
    fn default() -> Self {
        Self(NonZeroU8::MIN.saturating_add(1))
    }
}

impl fmt::Display for IndentSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Error returned when an indentation size is outside `1..=16` or not a number.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
#[error(
    "indent size must be an integer between 1 and {MAX_INDENT_SIZE}, got '{}'",
    echo(.input, KEY_LIMIT)
)]
pub struct InvalidIndentSize {
    /// The rejected input.
    pub input: String,
}

impl TryFrom<u64> for IndentSize {
    type Error = InvalidIndentSize;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        u8::try_from(value)
            .ok()
            .filter(|size| *size <= MAX_INDENT_SIZE)
            .and_then(NonZeroU8::new)
            .map(Self)
            .ok_or_else(|| InvalidIndentSize {
                input: value.to_string(),
            })
    }
}

impl FromStr for IndentSize {
    type Err = InvalidIndentSize;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u64>()
            .map_err(|_| InvalidIndentSize {
                input: s.to_owned(),
            })
            .and_then(Self::try_from)
    }
}

struct IndentSizeVisitor;

impl Visitor<'_> for IndentSizeVisitor {
    type Value = IndentSize;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "an integer between 1 and {MAX_INDENT_SIZE}")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        IndentSize::try_from(value)
            .map_err(|_| E::invalid_value(Unexpected::Unsigned(value), &self))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        u64::try_from(value)
            .map_err(|_| E::invalid_value(Unexpected::Signed(value), &self))
            .and_then(|unsigned| self.visit_u64(unsigned))
    }
}

impl Serialize for IndentSize {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.0.get())
    }
}

impl<'de> Deserialize<'de> for IndentSize {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(IndentSizeVisitor)
    }
}

/// An option value that accepts either a YAML boolean or a closed set of names.
///
/// Implementors get a `Deserialize` impl through [`deserialize_bool_or_name`].
pub trait BoolOrName: Sized {
    /// Description of the accepted forms for error messages.
    const EXPECTING: &'static str;

    /// Maps a YAML boolean to a value, or explains why it is not accepted.
    fn from_bool(value: bool) -> Result<Self, &'static str>;

    /// Maps a name to a value, or returns `None` for unknown names.
    fn from_name(name: &str) -> Option<Self>;
}

struct BoolOrNameVisitor<T>(PhantomData<T>);

impl<T: BoolOrName> Visitor<'_> for BoolOrNameVisitor<T> {
    type Value = T;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(T::EXPECTING)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<T, E> {
        T::from_bool(value).map_err(E::custom)
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<T, E> {
        T::from_name(value).ok_or_else(|| E::invalid_value(Unexpected::Str(value), &self))
    }
}

/// Deserializes a [`BoolOrName`] value from a boolean or a string.
pub fn deserialize_bool_or_name<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: BoolOrName,
{
    deserializer.deserialize_any(BoolOrNameVisitor(PhantomData))
}

/// Whether a document boundary marker (`---` or `...`) is required, forbidden or allowed.
///
/// Accepts `true` (required), `false` (forbidden), `required`, `forbidden` and `allowed`.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::MarkerPresence;
///
/// assert_eq!(MarkerPresence::default(), MarkerPresence::Allowed);
/// let parsed: MarkerPresence = serde_norway::from_str("false").unwrap();
/// assert_eq!(parsed, MarkerPresence::Forbidden);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarkerPresence {
    /// The marker must be present.
    Required,
    /// The marker must be absent.
    Forbidden,
    /// Either form is accepted.
    #[default]
    Allowed,
}

impl BoolOrName for MarkerPresence {
    const EXPECTING: &'static str = "a boolean, 'required', 'forbidden' or 'allowed'";

    fn from_bool(value: bool) -> Result<Self, &'static str> {
        Ok(if value {
            Self::Required
        } else {
            Self::Forbidden
        })
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "required" => Some(Self::Required),
            "forbidden" => Some(Self::Forbidden),
            "allowed" => Some(Self::Allowed),
            _ => None,
        }
    }
}

impl Serialize for MarkerPresence {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Required => "required",
            Self::Forbidden => "forbidden",
            Self::Allowed => "allowed",
        })
    }
}

impl<'de> Deserialize<'de> for MarkerPresence {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize_bool_or_name(deserializer)
    }
}

/// A boolean option that is always `true`: yamllint accepts `false`, fast-yaml cannot honor it.
///
/// Deserializing `true` succeeds; `false` fails with an explanatory error instead of being
/// silently ignored.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::AlwaysTrue;
///
/// assert!(serde_norway::from_str::<AlwaysTrue>("true").is_ok());
/// assert!(serde_norway::from_str::<AlwaysTrue>("false").is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AlwaysTrue;

impl Serialize for AlwaysTrue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(true)
    }
}

impl<'de> Deserialize<'de> for AlwaysTrue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if bool::deserialize(deserializer)? {
            Ok(Self)
        } else {
            Err(de::Error::custom(
                "false is not supported: this check is always on (it is a parse error)",
            ))
        }
    }
}

/// Compiled-size budget of one regular expression and of a whole list, in bytes.
const SIZE_LIMIT: usize = 10 << 20;

/// Longest accepted regular expression source, in bytes.
const MAX_PATTERN_LEN: usize = 256;

/// Most patterns accepted in one [`PatternList`].
const MAX_PATTERNS: usize = 64;

const REGEX_SYNTAX_HINT: &str = "Rust regex syntax: look-around and backreferences are unsupported";

const SIZE_HINT: &str = "use ASCII classes such as [A-Za-z0-9_] instead of \\w to shrink it";

/// Why a single regular expression was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidRegexPattern {
    /// The source exceeds the length cap.
    #[error("pattern is {len} bytes long, the limit is {MAX_PATTERN_LEN}")]
    TooLong {
        /// Length of the rejected source in bytes.
        len: usize,
    },
    /// The expression does not parse.
    #[error(
        "invalid regular expression '{}': {} ({REGEX_SYNTAX_HINT})",
        echo(.pattern, MESSAGE_LIMIT),
        echo(.message, MESSAGE_LIMIT)
    )]
    Syntax {
        /// The rejected source.
        pattern: String,
        /// Compiler diagnostic on one line.
        message: String,
    },
    /// The expression compiles to more than the size budget.
    #[error(
        "regular expression '{}' compiles to more than {limit} bytes; {SIZE_HINT}",
        echo(.pattern, MESSAGE_LIMIT)
    )]
    TooBig {
        /// The rejected source.
        pattern: String,
        /// The exceeded budget in bytes.
        limit: usize,
    },
}

impl InvalidRegexPattern {
    fn from_build_error(pattern: &str, error: &regex::Error) -> Self {
        match error {
            regex::Error::CompiledTooBig(limit) => Self::TooBig {
                pattern: pattern.to_owned(),
                limit: *limit,
            },
            other => {
                let text = other.to_string();
                let message = text
                    .lines()
                    .rev()
                    .find_map(|line| line.strip_prefix("error: "))
                    .map_or_else(|| text.replace('\n', " "), str::to_owned);
                Self::Syntax {
                    pattern: pattern.to_owned(),
                    message,
                }
            }
        }
    }
}

/// Error returned when a pattern list cannot be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidPatternList {
    /// One pattern is not an accepted regular expression.
    #[error("pattern {index}: {reason}")]
    Pattern {
        /// Zero-based position in the list.
        index: usize,
        /// Why the pattern was rejected.
        reason: InvalidRegexPattern,
    },
    /// The list holds more than the allowed number of patterns.
    #[error("too many patterns, the limit is {MAX_PATTERNS}")]
    TooMany,
    /// The patterns are valid alone but too large to compile together.
    #[error("the patterns are too large to compile together: {message}")]
    TooLarge {
        /// Compiler diagnostic.
        message: String,
    },
}

/// An ordered list of regular expressions matched as "any of" with `re.search` semantics.
///
/// Uses Rust regex syntax: look-around and backreferences are unsupported. The count and the
/// length of each source are checked before anything is compiled; the list is then compiled
/// once into a single set.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::PatternList;
///
/// let list = PatternList::new(["^http", "\\.md$"]).unwrap();
/// assert!(list.is_match("http://x"));
/// assert!(list.is_match("README.md"));
/// assert!(!list.is_match("plain"));
/// assert!(PatternList::new(["(?=a)"]).is_err());
/// assert!(PatternList::default().is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct PatternList {
    sources: Vec<Box<str>>,
    set: regex::RegexSet,
}

impl Default for PatternList {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            set: regex::RegexSet::empty(),
        }
    }
}

impl PatternList {
    /// Builds a list, rejecting more than `MAX_PATTERNS` patterns before compiling any of them.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidPatternList`] when a pattern is invalid, the list is too long, or the
    /// combined expressions exceed the compiled-size budget.
    pub fn new<I, S>(sources: I) -> Result<Self, InvalidPatternList>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let sources = sources
            .into_iter()
            .take(MAX_PATTERNS + 1)
            .map(|source| Box::from(source.as_ref()))
            .collect::<Vec<Box<str>>>();
        Self::from_sources(sources)
    }

    fn from_sources(sources: Vec<Box<str>>) -> Result<Self, InvalidPatternList> {
        if sources.len() > MAX_PATTERNS {
            return Err(InvalidPatternList::TooMany);
        }
        if let Some((index, source)) = sources
            .iter()
            .enumerate()
            .find(|(_, source)| source.len() > MAX_PATTERN_LEN)
        {
            return Err(InvalidPatternList::Pattern {
                index,
                reason: InvalidRegexPattern::TooLong { len: source.len() },
            });
        }
        let set = regex::RegexSetBuilder::new(sources.iter().map(AsRef::<str>::as_ref))
            .size_limit(SIZE_LIMIT)
            .dfa_size_limit(SIZE_LIMIT)
            .build()
            .map_err(|error| Self::locate_failure(&sources, &error))?;
        Ok(Self { sources, set })
    }

    /// Finds the pattern that made the set fail; only runs on the error path.
    fn locate_failure(sources: &[Box<str>], error: &regex::Error) -> InvalidPatternList {
        sources
            .iter()
            .enumerate()
            .find_map(|(index, source)| {
                regex::RegexBuilder::new(source)
                    .size_limit(SIZE_LIMIT)
                    .dfa_size_limit(SIZE_LIMIT)
                    .build()
                    .err()
                    .map(|error| InvalidPatternList::Pattern {
                        index,
                        reason: InvalidRegexPattern::from_build_error(source, &error),
                    })
            })
            .unwrap_or_else(|| InvalidPatternList::TooLarge {
                message: error.to_string(),
            })
    }

    /// Returns `true` when any pattern matches anywhere in `haystack`.
    #[must_use]
    pub fn is_match(&self, haystack: &str) -> bool {
        self.set.is_match(haystack)
    }

    /// Returns `true` when the list holds no patterns.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Returns the pattern sources in order.
    pub fn sources(&self) -> impl ExactSizeIterator<Item = &str> {
        self.sources.iter().map(AsRef::as_ref)
    }
}

impl PartialEq for PatternList {
    fn eq(&self, other: &Self) -> bool {
        self.sources == other.sources
    }
}

impl Eq for PatternList {}

impl Serialize for PatternList {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.sources())
    }
}

struct PatternListVisitor;

impl<'de> Visitor<'de> for PatternListVisitor {
    type Value = PatternList;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a list of at most {MAX_PATTERNS} regular expressions")
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<PatternList, A::Error> {
        let mut sources = Vec::new();
        while let Some(source) = seq.next_element::<String>()? {
            if sources.len() == MAX_PATTERNS {
                return Err(de::Error::custom(InvalidPatternList::TooMany));
            }
            sources.push(source.into_boxed_str());
        }
        PatternList::from_sources(sources).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for PatternList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(PatternListVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse<T: for<'de> Deserialize<'de>>(yaml: &str) -> Result<T, serde_norway::Error> {
        serde_norway::from_str(yaml)
    }

    #[test]
    fn limit_minus_one_is_disabled() {
        assert_eq!(parse::<Limit>("-1").unwrap(), Limit::Disabled);
        assert_eq!(parse::<Limit>("0").unwrap(), Limit::Max(0));
        assert_eq!(parse::<Limit>("7").unwrap(), Limit::Max(7));
    }

    #[test]
    fn limit_rejects_below_minus_one_and_non_integers() {
        assert!(parse::<Limit>("-2").is_err());
        assert!(parse::<Limit>("yes").is_err());
        assert!(parse::<Limit>("1.5").is_err());
        assert!(parse::<Limit>("99999999999").is_err());
    }

    #[test]
    fn limit_serializes_to_accepted_form() {
        for limit in [Limit::Disabled, Limit::Max(0), Limit::Max(9)] {
            let value = serde_norway::to_value(limit).unwrap();
            assert_eq!(serde_norway::from_value::<Limit>(value).unwrap(), limit);
        }
    }

    #[test]
    fn limit_comparisons() {
        assert!(Limit::Max(1).exceeded_by(2));
        assert!(!Limit::Max(2).exceeded_by(2));
        assert!(Limit::Max(2).unmet_by(1));
        assert!(!Limit::Max(2).unmet_by(2));
        assert!(!Limit::Disabled.unmet_by(0));
    }

    #[test]
    fn empty_inside_limit_minus_one_inherits() {
        assert_eq!(
            parse::<EmptyInsideLimit>("-1").unwrap(),
            EmptyInsideLimit::Inherit
        );
        assert_eq!(
            parse::<EmptyInsideLimit>("0").unwrap(),
            EmptyInsideLimit::Spaces(0)
        );
        assert!(parse::<EmptyInsideLimit>("-2").is_err());
    }

    #[test]
    fn empty_inside_limit_round_trips() {
        for limit in [EmptyInsideLimit::Inherit, EmptyInsideLimit::Spaces(3)] {
            let value = serde_norway::to_value(limit).unwrap();
            assert_eq!(
                serde_norway::from_value::<EmptyInsideLimit>(value).unwrap(),
                limit
            );
        }
    }

    #[test]
    fn indent_size_bounds() {
        assert_eq!(parse::<IndentSize>("1").unwrap().get(), 1);
        assert_eq!(parse::<IndentSize>("16").unwrap().get(), 16);
        assert!(parse::<IndentSize>("0").is_err());
        assert!(parse::<IndentSize>("17").is_err());
        assert!(parse::<IndentSize>("-1").is_err());
        assert!(parse::<IndentSize>("two").is_err());
        assert!("x".parse::<IndentSize>().is_err());
        assert_eq!("3".parse::<IndentSize>().unwrap().get(), 3);
    }

    #[test]
    fn indent_size_saturates() {
        assert_eq!(IndentSize::saturating_from_u8(0).get(), 1);
        assert_eq!(IndentSize::saturating_from_u8(16).get(), 16);
        assert_eq!(IndentSize::saturating_from_u8(17).get(), 16);
    }

    #[test]
    fn indent_size_round_trips() {
        let size = IndentSize::try_from(8u64).unwrap();
        let value = serde_norway::to_value(size).unwrap();
        assert_eq!(serde_norway::from_value::<IndentSize>(value).unwrap(), size);
    }
}
