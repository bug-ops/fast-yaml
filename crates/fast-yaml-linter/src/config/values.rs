//! Validated value types shared by rule option structs.

use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU8;
use std::str::FromStr;

use crate::echo::{KEY_LIMIT, echo};

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
