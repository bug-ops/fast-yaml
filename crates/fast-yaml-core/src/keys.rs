//! Detection of mapping keys that YAML keeps distinct but a host format would merge.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt;

use thiserror::Error;

use crate::options::KeyDomain;
use crate::value::{Value, quote_key};

/// Type of a scalar key, as named in a [`KeyError`].
///
/// # Examples
///
/// ```
/// use fast_yaml_core::KeyKind;
///
/// assert_eq!(KeyKind::Float.to_string(), "float");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyKind {
    /// Null.
    Null,
    /// Boolean.
    Bool,
    /// Integer, including one outside `i64`.
    Int,
    /// Floating-point number.
    Float,
    /// String.
    String,
}

impl KeyKind {
    const fn of(key: &Value) -> Option<Self> {
        Some(match key {
            Value::Null => Self::Null,
            Value::Bool(_) => Self::Bool,
            Value::Int(_) | Value::BigInt(_) => Self::Int,
            Value::Float(_) => Self::Float,
            Value::String(_) => Self::String,
            Value::Sequence(_) | Value::Mapping(_) | Value::Set(_) => return None,
        })
    }
}

impl fmt::Display for KeyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Null => "null",
            Self::Bool => "bool",
            Self::Int => "int",
            Self::Float => "float",
            Self::String => "string",
        })
    }
}

/// Two keys that YAML keeps distinct but the target of the load would treat as one.
///
/// `merged` is set when one of the keys was absorbed from a `<<` source, which has no position of
/// its own: the error is then reported at the `<<` key.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{KeyDomain, KeyError, LoadOptions, ParseError, Parser};
/// use fast_yaml_core::limits::ParseLimits;
///
/// let options = LoadOptions::new().with_keys(KeyDomain::Python);
/// let err = Parser::parse_all_with_options("1: a\ntrue: b\n", &ParseLimits::default(), options)
///     .unwrap_err();
/// let ParseError::Key { error, .. } = err else { panic!("key error expected") };
/// assert!(matches!(error, KeyError::PythonCollision { merged: false, .. }));
/// assert!(error.to_string().contains("equal as a Python dict key"));
/// ```
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyError {
    /// Keys of different types share the text of a string key (JSON, JavaScript).
    #[error(
        "{incoming} key {key} is distinct in YAML but converts to the same string key as a key of type {kept}{}",
        MergeNote(*.merged)
    )]
    StringCollision {
        /// Type of the later key.
        incoming: KeyKind,
        /// Type of the earlier key it collides with.
        kept: KeyKind,
        /// The later key, rendered for a message.
        key: Box<str>,
        /// Whether a key came from a `<<` source.
        merged: bool,
    },
    /// Numerically equal keys of different types are one Python dict key.
    #[error(
        "{incoming} key {key} is distinct in YAML but equal as a Python dict key to a key of type {kept}{}",
        MergeNote(*.merged)
    )]
    PythonCollision {
        /// Type of the later key.
        incoming: KeyKind,
        /// Type of the earlier key it collides with.
        kept: KeyKind,
        /// The later key, rendered for a message.
        key: Box<str>,
        /// Whether a key came from a `<<` source.
        merged: bool,
    },
}

/// Renders " (through merge key `<<`)" for a collision that involves a merged key.
struct MergeNote(bool);

impl fmt::Display for MergeNote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 {
            f.write_str(" (through merge key `<<`)")
        } else {
            Ok(())
        }
    }
}

impl KeyError {
    /// Marks the collision as involving a key absorbed from a `<<` source.
    #[must_use]
    pub const fn through_merge(mut self) -> Self {
        let (Self::StringCollision { merged, .. } | Self::PythonCollision { merged, .. }) =
            &mut self;
        *merged = true;
        self
    }
}

/// What two keys must share to be one key in a domain, independent of their type.
#[derive(Debug, PartialEq, Eq, Hash)]
enum Group {
    Text(Box<str>),
    Int(i128),
    FloatBits(u64),
}

/// Collision table of one mapping or set.
///
/// A string-keyed table stays empty until a key that is not a string arrives, and a Python table
/// until a second numeric type appears, so the common mapping pays nothing; the table then
/// learns the keys already present.
#[derive(Debug)]
pub struct KeyGuard {
    domain: KeyDomain,
    seen: Option<HashMap<Group, KeyKind>>,
    numeric_types: u8,
}

impl KeyGuard {
    /// Creates an empty table for one mapping or set loaded in `domain`.
    pub const fn new(domain: KeyDomain) -> Self {
        Self {
            domain,
            seen: None,
            numeric_types: 0,
        }
    }

    /// Records `key`; `prior` yields the keys already in the container.
    ///
    /// # Errors
    ///
    /// Returns the collision when `key` equals an earlier key of another type in the domain.
    pub fn check<'a>(
        &mut self,
        key: &Value,
        prior: impl Iterator<Item = &'a Value>,
    ) -> Result<(), KeyError> {
        let grouped = match self.domain {
            KeyDomain::Yaml => return Ok(()),
            KeyDomain::StringKeys => {
                if self.seen.is_none() && matches!(key, Value::String(_)) {
                    return Ok(());
                }
                self.seen
                    .get_or_insert_with(|| prior.filter_map(string_group).collect());
                string_group(key)
            }
            KeyDomain::Python => {
                let Some(bit) = numeric_type_bit(key) else {
                    return Ok(());
                };
                if self.seen.is_none() {
                    let single_type = self.numeric_types | bit == bit;
                    self.numeric_types |= bit;
                    if single_type {
                        return Ok(());
                    }
                    self.seen = Some(prior.filter_map(python_group).collect());
                }
                python_group(key)
            }
        };
        let Some((group, incoming)) = grouped else {
            return Ok(());
        };
        let domain = self.domain;
        match self.seen.get_or_insert_with(HashMap::new).entry(group) {
            Entry::Vacant(slot) => {
                slot.insert(incoming);
                Ok(())
            }
            Entry::Occupied(slot) if *slot.get() == incoming => Ok(()),
            Entry::Occupied(slot) => {
                let kept = *slot.get();
                let key = render(domain, key);
                Err(match domain {
                    KeyDomain::Python => KeyError::PythonCollision {
                        incoming,
                        kept,
                        key,
                        merged: false,
                    },
                    _ => KeyError::StringCollision {
                        incoming,
                        kept,
                        key,
                        merged: false,
                    },
                })
            }
        }
    }
}

fn render(domain: KeyDomain, key: &Value) -> Box<str> {
    match (domain, key) {
        (KeyDomain::Python, Value::Bool(b)) => b.to_string().into(),
        (KeyDomain::Python, Value::Float(f)) => python_float_repr(f.get()).into(),
        (KeyDomain::Python, Value::Int(i)) => i.to_string().into(),
        (KeyDomain::Python, Value::BigInt(big)) => big.canonical().into(),
        _ => quote_key(&key.key_text().unwrap_or_default()).into(),
    }
}

/// Python's `repr` of a float: `1e+300`, `9.223372036854776e+18`, `1.5e-07`.
fn python_float_repr(value: f64) -> String {
    let text = format!("{value:?}");
    let Some((mantissa, exponent)) = text.split_once('e') else {
        return text;
    };
    let (sign, digits) = exponent
        .strip_prefix('-')
        .map_or(('+', exponent), |digits| ('-', digits));
    format!("{mantissa}e{sign}{digits:0>2}")
}

fn string_group(key: &Value) -> Option<(Group, KeyKind)> {
    Some((
        Group::Text(key.key_text()?.into_owned().into_boxed_str()),
        KeyKind::of(key)?,
    ))
}

const fn numeric_type_bit(key: &Value) -> Option<u8> {
    match key {
        Value::Bool(_) => Some(1),
        Value::Int(_) | Value::BigInt(_) => Some(2),
        Value::Float(_) => Some(4),
        _ => None,
    }
}

/// Exclusive bound of the integers `i128` holds, as a float (2^127).
const I128_LIMIT: f64 = 170_141_183_460_469_231_731_687_303_715_884_105_728.0;

/// Group of a key under Python equality: numerically equal booleans, integers and integral
/// floats share a group, found without printing any number; other keys never equal a key of
/// another type.
fn python_group(key: &Value) -> Option<(Group, KeyKind)> {
    Some(match key {
        Value::Bool(b) => (Group::Int(i128::from(*b)), KeyKind::Bool),
        Value::Int(i) => (Group::Int(i128::from(*i)), KeyKind::Int),
        Value::BigInt(big) => (big_int_group(big.canonical()), KeyKind::Int),
        Value::Float(f) if f.get().is_finite() && f.get().fract() == 0.0 => {
            (float_group(f.get()), KeyKind::Float)
        }
        _ => return None,
    })
}

/// An integral float is the integer it holds when that fits `i128`, else its bit pattern.
fn float_group(value: f64) -> Group {
    if (-I128_LIMIT..I128_LIMIT).contains(&value) {
        #[expect(clippy::cast_possible_truncation)]
        return Group::Int(value as i128);
    }
    Group::FloatBits(value.to_bits())
}

/// An integer beyond `i128` equals a float only when that float holds it exactly; the one exact
/// check prints a float, so it runs only for an even integer that rounds to a finite float.
fn big_int_group(canonical: &str) -> Group {
    if let Ok(small) = canonical.parse::<i128>() {
        return Group::Int(small);
    }
    let even = canonical.ends_with(['0', '2', '4', '6', '8']);
    match canonical.parse::<f64>() {
        Ok(rounded) if even && rounded.is_finite() && format!("{rounded:.0}") == canonical => {
            Group::FloatBits(rounded.to_bits())
        }
        _ => Group::Text(canonical.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{BigInt, Float};

    fn check_all(domain: KeyDomain, keys: &[Value]) -> Result<(), KeyError> {
        let mut guard = KeyGuard::new(domain);
        keys.iter()
            .enumerate()
            .try_for_each(|(at, key)| guard.check(key, keys[..at].iter()))
    }

    fn string(s: &str) -> Value {
        Value::String(s.into())
    }

    #[test]
    fn yaml_domain_never_collides() {
        let keys = [Value::Int(1), string("1"), Value::Bool(true)];
        assert_eq!(check_all(KeyDomain::Yaml, &keys), Ok(()));
    }

    #[test]
    fn string_domain_collides_on_equal_text_of_different_types() {
        for keys in [
            [Value::Int(1), string("1")],
            [string("1"), Value::Int(1)],
            [Value::Bool(true), string("true")],
            [Value::Null, string("null")],
            [Value::Int(1), Value::Float(Float::new(1.0))],
            [Value::Int(0), Value::Float(Float::new(-0.0))],
            [Value::Float(Float::new(f64::NAN)), string("NaN")],
            [Value::Float(Float::new(1e21)), string("1e+21")],
            [Value::Float(Float::new(f64::INFINITY)), string("Infinity")],
        ] {
            let err = check_all(KeyDomain::StringKeys, &keys).unwrap_err();
            assert!(matches!(err, KeyError::StringCollision { .. }), "{keys:?}");
        }
    }

    #[test]
    fn string_domain_accepts_equal_and_distinct_keys() {
        let keys = [string("a"), Value::Int(1), Value::Int(1), string("b")];
        assert_eq!(check_all(KeyDomain::StringKeys, &keys), Ok(()));
    }

    #[test]
    fn python_domain_follows_python_equality() {
        let big = |t: &str| Value::BigInt(BigInt::parse(t).unwrap());
        for keys in [
            [Value::Int(1), Value::Bool(true)],
            [Value::Bool(false), Value::Float(Float::new(-0.0))],
            [Value::Int(0), Value::Float(Float::new(-0.0))],
            [Value::Float(Float::new(1.0)), Value::Int(1)],
            [
                big("9223372036854775808"),
                Value::Float(Float::new(9_223_372_036_854_775_808.0)),
            ],
        ] {
            let err = check_all(KeyDomain::Python, &keys).unwrap_err();
            assert!(matches!(err, KeyError::PythonCollision { .. }), "{keys:?}");
        }
    }

    #[test]
    fn python_domain_keeps_unequal_numbers_apart() {
        let big = |t: &str| Value::BigInt(BigInt::parse(t).unwrap());
        let ten_pow_300 = format!("1{}", "0".repeat(300));
        for keys in [
            vec![Value::Int(1), Value::Float(Float::new(1.5))],
            vec![string("1"), Value::Int(1)],
            vec![Value::Int(2), Value::Bool(true)],
            vec![big(&ten_pow_300), Value::Float(Float::new(1e300))],
            vec![Value::Float(Float::new(f64::NAN)), Value::Int(0)],
        ] {
            assert_eq!(check_all(KeyDomain::Python, &keys), Ok(()), "{keys:?}");
        }
    }

    #[test]
    fn messages_name_both_types_and_the_key() {
        let err = check_all(KeyDomain::Python, &[Value::Int(1), Value::Bool(true)]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "bool key true is distinct in YAML but equal as a Python dict key to a key of type int"
        );
        let err = check_all(KeyDomain::StringKeys, &[Value::Int(1), string("1")]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "string key \"1\" is distinct in YAML but converts to the same string key as a key of type int"
        );
    }

    #[test]
    fn python_float_keys_use_python_repr_in_messages() {
        let big = |t: &str| Value::BigInt(BigInt::parse(t).unwrap());
        for (partner, value, shown) in [
            (
                big("9223372036854775808"),
                9_223_372_036_854_775_808.0,
                "9.223372036854776e+18",
            ),
            (big(&format!("{:.0}", 1e300)), 1e300, "1e+300"),
            (Value::Int(1), 1.0, "1.0"),
            (Value::Int(0), -0.0, "-0.0"),
        ] {
            let keys = [partner, Value::Float(Float::new(value))];
            let err = check_all(KeyDomain::Python, &keys).unwrap_err();
            assert!(
                err.to_string().contains(&format!("float key {shown} ")),
                "{err}"
            );
        }
        assert_eq!(python_float_repr(1.5e-7), "1.5e-07");
    }

    #[test]
    fn merged_collisions_say_so() {
        let err = check_all(KeyDomain::StringKeys, &[Value::Int(1), string("1")])
            .unwrap_err()
            .through_merge();
        assert!(
            err.to_string().ends_with("(through merge key `<<`)"),
            "{err}"
        );
    }

    #[test]
    fn python_exactness_holds_beyond_i128() {
        let big = |t: &str| Value::BigInt(BigInt::parse(t).unwrap());
        let ten_pow = |n: usize| format!("1{}", "0".repeat(n));
        let exact = format!("{:.0}", 1e308);
        let distinct = [
            [big(&ten_pow(308)), Value::Float(Float::new(1e308))],
            [big(&ten_pow(38)), Value::Float(Float::new(1e38))],
            [
                Value::Int(9_007_199_254_740_993),
                Value::Float(Float::new(9_007_199_254_740_992.0)),
            ],
        ];
        for keys in distinct {
            assert_eq!(check_all(KeyDomain::Python, &keys), Ok(()), "{keys:?}");
        }
        let colliding = [
            [big(&exact), Value::Float(Float::new(1e308))],
            [
                big("170141183460469231731687303715884105728"),
                Value::Float(Float::new(2f64.powi(127))),
            ],
            [
                big("-170141183460469231731687303715884105728"),
                Value::Float(Float::new(-(2f64.powi(127)))),
            ],
            [
                big("-170141183460469231731687303715884105730"),
                Value::Float(Float::new(-(2f64.powi(127)))),
            ],
        ];
        for keys in &colliding[..3] {
            assert!(check_all(KeyDomain::Python, keys).is_err(), "{keys:?}");
        }
        assert_eq!(check_all(KeyDomain::Python, &colliding[3]), Ok(()));
        for keys in [
            [
                Value::Int(i64::MIN),
                Value::Float(Float::new(-9_223_372_036_854_775_808.0)),
            ],
            [Value::Int(0), Value::Float(Float::new(-0.0))],
        ] {
            assert!(check_all(KeyDomain::Python, &keys).is_err(), "{keys:?}");
        }
        for keys in [
            [
                Value::Float(Float::new(f64::NAN)),
                Value::Float(Float::new(f64::NAN)),
            ],
            [Value::Float(Float::new(f64::INFINITY)), Value::Int(1)],
        ] {
            assert_eq!(check_all(KeyDomain::Python, &keys), Ok(()), "{keys:?}");
        }
    }

    #[test]
    fn python_table_waits_for_a_second_numeric_type() {
        let floats: Vec<Value> = (0..1000)
            .map(|i| Value::Float(Float::new(f64::from(i).mul_add(1e290, 1e302))))
            .collect();
        let mut guard = KeyGuard::new(KeyDomain::Python);
        for (at, key) in floats.iter().enumerate() {
            guard.check(key, floats[..at].iter()).unwrap();
        }
        assert!(guard.seen.is_none());
        assert!(guard.check(&Value::Bool(true), floats.iter()).is_ok());
        assert!(guard.seen.is_some());
    }

    #[test]
    fn python_work_stays_linear_for_huge_float_keys() {
        let mut keys: Vec<Value> = (0..200_000)
            .map(|i| Value::Float(Float::new(f64::from(i).mul_add(1e290, 1e302))))
            .collect();
        keys.push(Value::Bool(true));
        let started = std::time::Instant::now();
        assert_eq!(check_all(KeyDomain::Python, &keys), Ok(()));
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }
}
