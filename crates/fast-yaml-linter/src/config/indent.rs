//! Typed values of the indentation rule options.

use std::fmt;

use serde::de::{self, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::IndentSize;

/// How many spaces each indentation level uses.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::{IndentSize, IndentSpaces};
///
/// let fixed: IndentSpaces = serde_norway::from_str("4").unwrap();
/// assert_eq!(fixed, IndentSpaces::Fixed(IndentSize::try_from(4u64).unwrap()));
/// let consistent: IndentSpaces = serde_norway::from_str("consistent").unwrap();
/// assert_eq!(consistent, IndentSpaces::Consistent);
/// assert!(serde_norway::from_str::<IndentSpaces>("17").is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentSpaces {
    /// Every level is indented by exactly this many spaces.
    Fixed(IndentSize),
    /// The first indented level of a document sets the width the others must follow.
    Consistent,
}

impl Serialize for IndentSpaces {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Fixed(size) => size.serialize(serializer),
            Self::Consistent => serializer.serialize_str("consistent"),
        }
    }
}

struct IndentSpacesVisitor;

impl Visitor<'_> for IndentSpacesVisitor {
    type Value = IndentSpaces;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an integer between 1 and 16 or `consistent`")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        IndentSize::try_from(value)
            .map(IndentSpaces::Fixed)
            .map_err(|_| E::invalid_value(Unexpected::Unsigned(value), &self))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        u64::try_from(value)
            .map_err(|_| E::invalid_value(Unexpected::Signed(value), &self))
            .and_then(|unsigned| self.visit_u64(unsigned))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        match value {
            "consistent" => Ok(IndentSpaces::Consistent),
            _ => Err(E::invalid_value(Unexpected::Str(value), &self)),
        }
    }
}

impl<'de> Deserialize<'de> for IndentSpaces {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(IndentSpacesVisitor)
    }
}

/// Whether a sequence nested in a mapping is indented under its key.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::IndentSequences;
///
/// assert_eq!(IndentSequences::default(), IndentSequences::Indented);
/// let parsed: IndentSequences = serde_norway::from_str("consistent").unwrap();
/// assert_eq!(parsed, IndentSequences::Consistent);
/// assert_eq!(serde_norway::from_str::<IndentSequences>("false").unwrap(), IndentSequences::NotIndented);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IndentSequences {
    /// `- item` lines are indented relative to the key (`true`).
    #[default]
    Indented,
    /// `- item` lines sit at the key's column (`false`).
    NotIndented,
    /// Either style is accepted (`whatever`).
    Whatever,
    /// The style of the first nested sequence of a document is required of the others.
    Consistent,
}

impl Serialize for IndentSequences {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Indented => serializer.serialize_bool(true),
            Self::NotIndented => serializer.serialize_bool(false),
            Self::Whatever => serializer.serialize_str("whatever"),
            Self::Consistent => serializer.serialize_str("consistent"),
        }
    }
}

struct IndentSequencesVisitor;

impl Visitor<'_> for IndentSequencesVisitor {
    type Value = IndentSequences;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a boolean, `whatever` or `consistent`")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(if value {
            IndentSequences::Indented
        } else {
            IndentSequences::NotIndented
        })
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        match value {
            "whatever" => Ok(IndentSequences::Whatever),
            "consistent" => Ok(IndentSequences::Consistent),
            _ => Err(E::invalid_value(Unexpected::Str(value), &self)),
        }
    }
}

impl<'de> Deserialize<'de> for IndentSequences {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(IndentSequencesVisitor)
    }
}
