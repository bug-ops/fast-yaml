//! YAML 1.2 core-schema scalar resolution shared by the core loader and the language bindings.
//!
//! [`resolve_scalar`] is the single place that decides which type a scalar has, given its
//! text, style and tag. The core loader ([`Parser`]) and the Python bindings
//! both adapt its [`ResolvedScalar`] result to their own value types, so the rules cannot drift
//! between surfaces.

use std::borrow::Cow;

use crate::events::{ScalarStyle, Tag};

/// Numeral system of an integer literal.
///
/// The enum is exhaustive: a new radix must be handled by every consumer at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntRadix {
    /// Plain decimal digits.
    Decimal,
    /// `0x` / `0X` prefix.
    Hex,
    /// `0o` / `0O` prefix.
    Octal,
}

impl IntRadix {
    /// Returns the numeric base (10, 16 or 8), as accepted by `int(text, base)` and
    /// `u64::from_str_radix`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::IntRadix;
    ///
    /// assert_eq!(IntRadix::Hex.value(), 16);
    /// ```
    #[must_use]
    pub const fn value(self) -> u32 {
        match self {
            Self::Decimal => 10,
            Self::Hex => 16,
            Self::Octal => 8,
        }
    }
}

/// Largest significant bit length of a hex or octal literal accepted as an integer.
///
/// `2^14284 - 1` is the largest value whose decimal form has at most 4300 digits, `CPython`'s default
/// `int(str)` limit, so canonical decimal text of an accepted literal always loads in Python. This
/// also bounds the quadratic radix-to-decimal conversion; decimal literals need none and are not
/// capped. Longer literals stay strings.
const MAX_RADIX_BIG_BITS: usize = 14_284;

/// Namespace of the YAML core-schema tags, the target of the `!!` shorthand.
const CORE_TAG_PREFIX: &str = "tag:yaml.org,2002:";

/// Returns the core-schema name of `tag` (`int` for `!!int`), `None` for any other tag.
///
/// The shorthand `!!int`, a `%TAG` handle bound to the core prefix and the verbatim form
/// `!<tag:yaml.org,2002:int>` all name the same tag, and only this function knows all three.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::events::Tag;
/// use fast_yaml_core::scalar::core_tag_suffix;
///
/// assert_eq!(core_tag_suffix(&Tag::new("tag:yaml.org,2002:", "int")), Some("int"));
/// assert_eq!(core_tag_suffix(&Tag::new("", "tag:yaml.org,2002:int")), Some("int"));
/// assert_eq!(core_tag_suffix(&Tag::new("!", "int")), None);
/// assert_eq!(core_tag_suffix(&Tag::new("", "tag:example.com,2000:int")), None);
/// ```
#[must_use]
pub fn core_tag_suffix<'t>(tag: &'t Tag<'_>) -> Option<&'t str> {
    core_tag_suffix_raw(&tag.0)
}

pub(crate) fn core_tag_suffix_raw(tag: &saphyr_parser::Tag) -> Option<&str> {
    if tag.handle == CORE_TAG_PREFIX {
        Some(&tag.suffix)
    } else if tag.handle.is_empty() {
        tag.suffix.strip_prefix(CORE_TAG_PREFIX)
    } else {
        None
    }
}

/// Borrowed view of an integer literal that overflows `i64`, in decimal, hex or octal notation.
///
/// [`BigInt`](crate::BigInt) is the owned form.
///
/// Guarantees an optional sign, then (for non-decimal radixes) the matching `0x`/`0o` prefix, then
/// a non-empty run of digits valid for the radix; it can only be produced by [`resolve_scalar`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BigIntRef<'a> {
    text: &'a str,
    negative: bool,
    radix: IntRadix,
    digits: &'a str,
}

impl<'a> BigIntRef<'a> {
    /// Returns the literal text as written, including sign and radix prefix.
    ///
    /// Pair it with [`radix`](Self::radix) for arbitrary-precision parsers such as Python's
    /// `int(text, base)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{ResolvedScalar, resolve_scalar};
    /// use fast_yaml_core::ScalarStyle;
    ///
    /// let ResolvedScalar::BigInt(big) =
    ///     resolve_scalar("0xFFFFFFFFFFFFFFFFFF", ScalarStyle::Plain, None)
    /// else {
    ///     unreachable!()
    /// };
    /// assert_eq!(big.as_str(), "0xFFFFFFFFFFFFFFFFFF");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.text
    }

    /// Returns the numeral system of the literal.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{IntRadix, ResolvedScalar, resolve_scalar};
    /// use fast_yaml_core::ScalarStyle;
    ///
    /// let ResolvedScalar::BigInt(big) =
    ///     resolve_scalar("0o7777777777777777777777", ScalarStyle::Plain, None)
    /// else {
    ///     unreachable!()
    /// };
    /// assert_eq!(big.radix(), IntRadix::Octal);
    /// ```
    #[must_use]
    pub const fn radix(self) -> IntRadix {
        self.radix
    }

    /// Returns the value as decimal text in JSON integer grammar: no leading `+`, no leading
    /// zeros, no radix prefix.
    ///
    /// Borrows the input when it is already canonical. Hex and octal literals are converted on
    /// every call, at cost quadratic in their length (bounded by the literal size cap).
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{ResolvedScalar, resolve_scalar};
    /// use fast_yaml_core::ScalarStyle;
    ///
    /// for (raw, decimal) in [
    ///     ("+0099999999999999999999", "99999999999999999999"),
    ///     ("0xFFFFFFFFFFFFFFFFFF", "4722366482869645213695"),
    ///     ("-0o7777777777777777777777", "-73786976294838206463"),
    /// ] {
    ///     let ResolvedScalar::BigInt(big) = resolve_scalar(raw, ScalarStyle::Plain, None) else {
    ///         unreachable!()
    ///     };
    ///     assert_eq!(big.canonical(), decimal);
    /// }
    /// ```
    #[must_use]
    pub fn canonical(self) -> Cow<'a, str> {
        match self.radix {
            IntRadix::Decimal => {
                let trimmed = self.digits.trim_start_matches('0');
                if self.text.len() == trimmed.len() + usize::from(self.negative) {
                    Cow::Borrowed(self.text)
                } else if self.negative {
                    Cow::Owned(format!("-{trimmed}"))
                } else {
                    Cow::Borrowed(trimmed)
                }
            }
            IntRadix::Hex => Cow::Owned(self.signed(radix_digits_to_decimal(self.digits, 4))),
            IntRadix::Octal => Cow::Owned(self.signed(radix_digits_to_decimal(self.digits, 3))),
        }
    }

    fn signed(self, decimal: String) -> String {
        if self.negative {
            format!("-{decimal}")
        } else {
            decimal
        }
    }
}

/// Value of one digit of a validated literal.
fn radix_digit(byte: u8, base: u32) -> u64 {
    char::from(byte).to_digit(base).map_or_else(
        || unreachable!("BigInt digits are validated by parse_int"),
        u64::from,
    )
}

/// Whether the literal's significant bit length exceeds [`MAX_RADIX_BIG_BITS`].
fn exceeds_bit_cap(digits: &str, bits_per_digit: u32) -> bool {
    let significant = digits.trim_start_matches('0');
    let Some(top) = significant
        .bytes()
        .next()
        .map(|b| radix_digit(b, 1 << bits_per_digit))
    else {
        return false;
    };
    let top_bits = u64::BITS - top.leading_zeros();
    (significant.len() - 1) * bits_per_digit as usize + top_bits as usize > MAX_RADIX_BIG_BITS
}

/// Converts a run of valid digits of base `2^bits_per_digit` to decimal text using base-1e9 limbs.
///
/// Digits are consumed in groups of at most 28 bits so each limb pass multiplies by up to `2^28`
/// (`limb * 2^28 + carry` stays below `2^64`). Leading zeros are free: the limb vector stays empty
/// until the first non-zero group.
#[allow(clippy::cast_possible_truncation)] // limbs are reduced below 1e9 before the cast
fn radix_digits_to_decimal(digits: &str, bits_per_digit: u32) -> String {
    use std::fmt::Write;

    const BASE: u64 = 1_000_000_000;
    let base = 1u32 << bits_per_digit;
    let mut limbs: Vec<u32> = Vec::new();
    for group in digits.as_bytes().chunks((28 / bits_per_digit) as usize) {
        let (multiplier, mut carry) = group.iter().fold((1u64, 0u64), |(mul, value), &b| {
            (
                mul * u64::from(base),
                value * u64::from(base) + radix_digit(b, base),
            )
        });
        for limb in &mut limbs {
            let acc = u64::from(*limb) * multiplier + carry;
            *limb = (acc % BASE) as u32;
            carry = acc / BASE;
        }
        if carry > 0 {
            limbs.push(carry as u32);
        }
    }
    let mut limbs = limbs.into_iter().rev();
    let mut out = limbs.next().map_or_else(String::new, |top| top.to_string());
    for limb in limbs {
        let _ = write!(out, "{limb:09}");
    }
    out
}

/// The type a scalar resolves to under the YAML 1.2 core schema.
///
/// `Str` and `BigInt` always borrow the entire input text of the scalar, so callers that own
/// the input may reuse it instead of copying. The enum is intentionally exhaustive: adding a
/// variant must break every binding adapter at compile time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResolvedScalar<'a> {
    /// Null (`~`, `null`, `Null`, `NULL`, or empty).
    Null,
    /// Boolean.
    Bool(bool),
    /// Integer that fits in `i64`.
    Int(i64),
    /// Integer outside `i64`, in decimal, hex or octal notation.
    BigInt(BigIntRef<'a>),
    /// Floating-point number, including `.inf` and `.nan`.
    Float(f64),
    /// String; borrows the whole scalar text.
    Str(&'a str),
}

#[derive(Clone, Copy)]
enum CoreTag {
    Str,
    Int,
    Float,
    Bool,
    Null,
    Unsupported,
}

#[derive(Clone, Copy)]
enum TagClass {
    Untagged,
    NonSpecific,
    Core(CoreTag),
    Other,
}

impl TagClass {
    fn of(tag: Option<&saphyr_parser::Tag>) -> Self {
        let Some(tag) = tag else {
            return Self::Untagged;
        };
        if tag.handle.is_empty() && tag.suffix == "!" {
            return Self::NonSpecific;
        }
        let Some(suffix) = core_tag_suffix_raw(tag) else {
            return Self::Other;
        };
        Self::Core(match suffix {
            "str" => CoreTag::Str,
            "int" => CoreTag::Int,
            "float" => CoreTag::Float,
            "bool" => CoreTag::Bool,
            "null" => CoreTag::Null,
            _ => CoreTag::Unsupported,
        })
    }
}

/// Resolves a scalar to its core-schema type.
///
/// This is the sole implementation of scalar typing for the core loader and the bindings.
/// Rules, in order:
///
/// 1. A core-schema tag (`!!int`, `!!float`, `!!bool`, `!!null`, `!!str`) applies regardless of
///    the scalar style; if the text cannot be coerced the result is a string (YAML 1.2 §3.3.2:
///    implicit resolution applies only to non-specific tags). `!!int` accepts float text by
///    truncation toward zero.
/// 2. The non-specific tag `!` forces a string (§6.8.1).
/// 3. Otherwise non-plain scalars are strings, and plain scalars resolve implicitly: empty
///    and null forms, booleans, integers (decimal, `0x`, `0o`, [`ResolvedScalar::BigInt`] when
///    beyond `i64`), then floats; anything else is a string. Hex and octal literals with more
///    than 14284 significant bits (canonical decimal beyond 4300 digits) stay strings.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{ResolvedScalar, resolve_scalar};
/// use fast_yaml_core::ScalarStyle;
/// use fast_yaml_core::events::Tag;
///
/// // Quoted scalars are strings...
/// assert_eq!(
///     resolve_scalar("7", ScalarStyle::DoubleQuoted, None),
///     ResolvedScalar::Str("7")
/// );
///
/// // ...unless a core-schema tag says otherwise.
/// let int = Tag::new("tag:yaml.org,2002:", "int");
/// assert_eq!(
///     resolve_scalar("7", ScalarStyle::DoubleQuoted, Some(&int)),
///     ResolvedScalar::Int(7)
/// );
/// assert_eq!(
///     resolve_scalar("3.0", ScalarStyle::Plain, Some(&int)),
///     ResolvedScalar::Int(3)
/// );
///
/// // A tag that cannot be applied yields a string.
/// assert_eq!(
///     resolve_scalar("true", ScalarStyle::Plain, Some(&int)),
///     ResolvedScalar::Str("true")
/// );
///
/// // The non-specific tag `!` forces a string.
/// let bang = Tag::new("", "!");
/// assert_eq!(
///     resolve_scalar("42", ScalarStyle::Plain, Some(&bang)),
///     ResolvedScalar::Str("42")
/// );
/// ```
#[must_use]
pub fn resolve_scalar<'a>(
    s: &'a str,
    style: ScalarStyle,
    tag: Option<&Tag<'_>>,
) -> ResolvedScalar<'a> {
    resolve_scalar_raw(s, style, tag.map(|tag| &*tag.0))
}

pub(crate) fn resolve_scalar_raw<'a>(
    s: &'a str,
    style: ScalarStyle,
    tag: Option<&saphyr_parser::Tag>,
) -> ResolvedScalar<'a> {
    match TagClass::of(tag) {
        TagClass::Core(core) => coerce_core(core, s).unwrap_or(ResolvedScalar::Str(s)),
        TagClass::NonSpecific => ResolvedScalar::Str(s),
        TagClass::Untagged | TagClass::Other => {
            if style == ScalarStyle::Plain {
                resolve_implicit(s)
            } else {
                ResolvedScalar::Str(s)
            }
        }
    }
}

fn coerce_core(tag: CoreTag, s: &str) -> Option<ResolvedScalar<'_>> {
    match tag {
        CoreTag::Str => Some(ResolvedScalar::Str(s)),
        CoreTag::Int => parse_int(s).or_else(|| float_str_to_int(s).map(ResolvedScalar::Int)),
        CoreTag::Float => parse_core_schema_float(s).map(ResolvedScalar::Float),
        CoreTag::Bool => match s {
            "true" | "True" | "TRUE" => Some(ResolvedScalar::Bool(true)),
            "false" | "False" | "FALSE" => Some(ResolvedScalar::Bool(false)),
            _ => None,
        },
        CoreTag::Null => {
            matches!(s, "~" | "null" | "Null" | "NULL" | "").then_some(ResolvedScalar::Null)
        }
        CoreTag::Unsupported => None,
    }
}

fn resolve_implicit(s: &str) -> ResolvedScalar<'_> {
    match s {
        "" | "~" | "null" | "NULL" | "Null" => ResolvedScalar::Null,
        "true" | "True" | "TRUE" => ResolvedScalar::Bool(true),
        "false" | "False" | "FALSE" => ResolvedScalar::Bool(false),
        _ => parse_int(s)
            .or_else(|| parse_core_schema_float(s).map(ResolvedScalar::Float))
            .unwrap_or(ResolvedScalar::Str(s)),
    }
}

/// Parse a YAML core schema integer: decimal, hex (`0x`), or octal (`0o`), with an optional sign.
///
/// Returns `Int` when the value fits `i64`, `BigInt` when it does not, and `None` for non-integer
/// syntax or a hex/octal literal beyond [`MAX_RADIX_BIG_BITS`].
fn parse_int(s: &str) -> Option<ResolvedScalar<'_>> {
    let (negative, unsigned) = match s.as_bytes() {
        [b'-', ..] => (true, &s[1..]),
        [b'+', ..] => (false, &s[1..]),
        _ => (false, s),
    };
    let (radix, digits) = match unsigned.as_bytes() {
        [b'0', b'x' | b'X', ..] => (IntRadix::Hex, &unsigned[2..]),
        [b'0', b'o' | b'O', ..] => (IntRadix::Octal, &unsigned[2..]),
        _ => (IntRadix::Decimal, unsigned),
    };
    let base = radix.value();
    if digits.is_empty() || !digits.bytes().all(|b| char::from(b).is_digit(base)) {
        return None;
    }
    let fitted = u64::from_str_radix(digits, base)
        .ok()
        .and_then(|magnitude| {
            if negative {
                0i64.checked_sub_unsigned(magnitude)
            } else {
                i64::try_from(magnitude).ok()
            }
        });
    if let Some(int) = fitted {
        return Some(ResolvedScalar::Int(int));
    }
    let too_long = match radix {
        IntRadix::Decimal => false,
        IntRadix::Hex => exceeds_bit_cap(digits, 4),
        IntRadix::Octal => exceeds_bit_cap(digits, 3),
    };
    (!too_long).then_some(ResolvedScalar::BigInt(BigIntRef {
        text: s,
        negative,
        radix,
        digits,
    }))
}

/// Attempt to coerce a float string to `i64` via truncation toward zero (`PyYAML` convention).
///
/// Returns `None` for non-finite values (.nan, .inf) and values outside the `i64` range.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn float_str_to_int(s: &str) -> Option<i64> {
    parse_core_schema_float(s)
        .filter(|f| f.is_finite() && *f >= i64::MIN as f64 && *f < i64::MAX as f64)
        .map(|f| f as i64)
}

/// Parse a YAML core schema float, handling special values (.inf, .nan, etc.).
fn parse_core_schema_float(s: &str) -> Option<f64> {
    match s {
        ".inf" | "+.inf" | ".Inf" | "+.Inf" | ".INF" | "+.INF" => Some(f64::INFINITY),
        "-.inf" | "-.Inf" | "-.INF" => Some(f64::NEG_INFINITY),
        ".nan" | ".NaN" | ".NAN" => Some(f64::NAN),
        // Reject bare words like "infinity" or "nan" that Rust's f64::parse() accepts.
        other => {
            let s = other.strip_prefix(['+', '-']).unwrap_or(other);
            let has_mantissa = s.starts_with(|c: char| c.is_ascii_digit())
                || s.strip_prefix('.')
                    .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()));
            let looks_like_float = has_mantissa
                && s.chars().all(|c| {
                    c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-'
                });
            looks_like_float
                .then(|| other.parse::<f64>().ok())
                .flatten()
        }
    }
}

/// Whether `c` is in the YAML 1.2.2 `c-printable` set (§5.1).
///
/// Everything else (C0 controls other than tab and line breaks, DEL, C1 controls from U+0086,
/// U+FFFE and U+FFFF) must be escaped in a double-quoted scalar to survive a round trip.
pub(crate) const fn is_c_printable(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\r' | '\u{20}'..='\u{7E}' | '\u{85}' | '\u{A0}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ResolvedScalar::{Bool, Float, Int, Null, Str};

    fn big_of(raw: &str) -> BigIntRef<'_> {
        let ResolvedScalar::BigInt(big) = resolve_scalar(raw, ScalarStyle::Plain, None) else {
            panic!("{raw} should be BigInt");
        };
        big
    }

    #[test]
    fn c_printable_matches_the_spec_set() {
        for c in [
            '\t',
            '\n',
            '\r',
            ' ',
            '~',
            '\u{85}',
            '\u{A0}',
            '\u{FFFD}',
            '\u{10000}',
        ] {
            assert!(is_c_printable(c), "{c:?}");
        }
        for c in [
            '\0', '\u{8}', '\u{B}', '\u{1F}', '\u{7F}', '\u{80}', '\u{86}', '\u{9F}', '\u{FFFE}',
            '\u{FFFF}',
        ] {
            assert!(!is_c_printable(c), "{c:?}");
        }
    }

    #[test]
    fn big_int_canonical_form() {
        for (raw, expected) in [
            ("9223372036854775808", "9223372036854775808"),
            ("+9223372036854775808", "9223372036854775808"),
            ("-9223372036854775809", "-9223372036854775809"),
            ("-0009223372036854775809", "-9223372036854775809"),
            (
                "0000000000000000000009223372036854775808",
                "9223372036854775808",
            ),
            ("0xFFFFFFFFFFFFFFFFFF", "4722366482869645213695"),
            ("+0XFF0000000000000000", "4703919738795935662080"),
            ("-0xFFFFFFFFFFFFFFFFFF", "-4722366482869645213695"),
            (
                "0x0000000000000000000000010000000000000000",
                "18446744073709551616",
            ),
            ("0x8000000000000000", "9223372036854775808"),
            ("-0x8000000000000001", "-9223372036854775809"),
            ("0o7777777777777777777777", "73786976294838206463"),
            ("0o1000000000000000000000", "9223372036854775808"),
            ("-0O1000000000000000000001", "-9223372036854775809"),
        ] {
            assert_eq!(big_of(raw).canonical(), expected, "{raw}");
        }
    }

    #[test]
    fn big_int_radix() {
        for (raw, radix) in [
            ("99999999999999999999", IntRadix::Decimal),
            ("-0xFFFFFFFFFFFFFFFFFF", IntRadix::Hex),
            ("+0XFFFFFFFFFFFFFFFFFF", IntRadix::Hex),
            ("0o7777777777777777777777", IntRadix::Octal),
            ("0O7777777777777777777777", IntRadix::Octal),
        ] {
            assert_eq!(big_of(raw).radix(), radix, "{raw}");
            assert_eq!(big_of(raw).as_str(), raw);
        }
    }

    #[test]
    fn canonical_decimal_is_a_fixed_point() {
        for raw in [
            "99999999999999999999",
            "-99999999999999999999",
            "0xFFFFFFFFFFFFFFFFFF",
            "-0o7777777777777777777777",
        ] {
            let canonical = big_of(raw).canonical().into_owned();
            let again = big_of(&canonical);
            assert_eq!(again.radix(), IntRadix::Decimal);
            assert!(matches!(again.canonical(), Cow::Borrowed(c) if c == canonical));
        }
    }

    #[test]
    fn radix_bit_length_cap() {
        // Bit lengths: hex 4 * 3571 = 14284, octal 3 * 4761 + 1 = 14284.
        let fits = [
            format!("0x{}", "F".repeat(3571)),
            format!("0x000{}", "F".repeat(3571)),
            format!("0x1{}", "0".repeat(3570)),
            format!("0o1{}", "7".repeat(4761)),
            format!("0o{}", "7".repeat(4761)),
            format!("-0o1{}", "7".repeat(4761)),
        ];
        let too_long = [
            format!("0x1{}", "0".repeat(3571)),
            format!("0x1{}", "F".repeat(3571)),
            format!("0o2{}", "0".repeat(4761)),
            format!("0o{}", "7".repeat(4762)),
            format!("-0x1{}", "0".repeat(3571)),
        ];
        for raw in &fits {
            assert!(
                matches!(plain(raw), ResolvedScalar::BigInt(_)),
                "plain {}",
                &raw[..8]
            );
            assert!(
                matches!(
                    tagged(raw, ScalarStyle::Plain, "int"),
                    ResolvedScalar::BigInt(_)
                ),
                "!!int {}",
                &raw[..8]
            );
            assert!(big_of(raw).canonical().trim_start_matches('-').len() <= 4300);
        }
        for raw in &too_long {
            assert_eq!(plain(raw), Str(raw), "plain {}", &raw[..8]);
            assert_eq!(
                tagged(raw, ScalarStyle::Plain, "int"),
                Str(raw),
                "!!int {}",
                &raw[..8]
            );
        }
    }

    #[test]
    fn largest_radix_int_has_exactly_4300_decimal_digits() {
        // 2^14284 - 1 has 4300 digits and ends in 5; 2^14285 - 1 (one more bit) has 4301.
        let canonical = big_of(&format!("0x{}", "F".repeat(3571)))
            .canonical()
            .into_owned();
        assert_eq!(canonical.len(), 4300);
        assert!(canonical.ends_with('5'));
    }

    #[test]
    fn radix_conversion_handles_partial_digit_groups() {
        for (raw, expected) in [
            ("0x1FFFFFFFFFFFFFFFFF", "590295810358705651711"),
            (
                "0x1FFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
                "166153499473114484112975882535043071",
            ),
            ("0o1777777777777777777777", "18446744073709551615"),
            (
                "0o177777777777777777777777777777",
                "309485009821345068724781055",
            ),
        ] {
            assert_eq!(big_of(raw).canonical(), expected, "{raw}");
        }
    }

    #[test]
    fn long_decimal_is_not_capped() {
        let raw = "9".repeat(10_000);
        assert_eq!(big_of(&raw).canonical(), raw);
    }

    fn core(suffix: &str) -> Tag<'static> {
        Tag::new("tag:yaml.org,2002:", suffix)
    }

    fn big(s: &str) -> ResolvedScalar<'_> {
        let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
        let (radix, digits) = match unsigned.get(..2) {
            Some("0x" | "0X") => (IntRadix::Hex, &unsigned[2..]),
            Some("0o" | "0O") => (IntRadix::Octal, &unsigned[2..]),
            _ => (IntRadix::Decimal, unsigned),
        };
        ResolvedScalar::BigInt(BigIntRef {
            text: s,
            negative: s.starts_with('-'),
            radix,
            digits,
        })
    }

    fn plain(s: &str) -> ResolvedScalar<'_> {
        resolve_scalar(s, ScalarStyle::Plain, None)
    }

    fn tagged<'a>(s: &'a str, style: ScalarStyle, suffix: &str) -> ResolvedScalar<'a> {
        resolve_scalar(s, style, Some(&core(suffix)))
    }

    #[test]
    fn plain_implicit_resolution() {
        for s in ["~", "null", "Null", "NULL", ""] {
            assert_eq!(plain(s), Null, "{s:?}");
        }
        for s in ["true", "True", "TRUE"] {
            assert_eq!(plain(s), Bool(true), "{s:?}");
        }
        for s in ["false", "False", "FALSE"] {
            assert_eq!(plain(s), Bool(false), "{s:?}");
        }
        assert_eq!(plain("0o17"), Int(15));
        assert_eq!(plain("0O17"), Int(15));
        assert_eq!(plain("0x1F"), Int(31));
        assert_eq!(plain("+42"), Int(42));
        assert_eq!(plain("007"), Int(7));
        assert_eq!(plain("1.5e10"), Float(1.5e10));
        assert_eq!(plain(".inf"), Float(f64::INFINITY));
        assert_eq!(plain(".Inf"), Float(f64::INFINITY));
        assert_eq!(plain("-.inf"), Float(f64::NEG_INFINITY));
        assert!(matches!(plain(".nan"), Float(f) if f.is_nan()));
        assert_eq!(plain("yes"), Str("yes"));
    }

    #[test]
    fn leading_dot_floats_and_signed_infinity() {
        assert_eq!(plain(".5"), Float(0.5));
        assert_eq!(plain("-.5"), Float(-0.5));
        assert_eq!(plain("+.5e1"), Float(5.0));
        for s in ["+.inf", "+.Inf", "+.INF"] {
            assert_eq!(plain(s), Float(f64::INFINITY), "{s:?}");
        }
        assert_eq!(
            tagged("+.inf", ScalarStyle::Plain, "float"),
            Float(f64::INFINITY)
        );
        assert_eq!(tagged(".5", ScalarStyle::Plain, "int"), Int(0));
        for s in ["+.nan", ".", "-.", ".e5", "..5", ".5.5", "+.infx"] {
            assert_eq!(plain(s), Str(s), "{s:?}");
        }
    }

    #[test]
    fn plain_bigint_and_overflow() {
        assert_eq!(plain("99999999999999999999"), big("99999999999999999999"));
        assert_eq!(plain("-99999999999999999999"), big("-99999999999999999999"));
        assert_eq!(plain("+99999999999999999999"), big("+99999999999999999999"));
        assert_eq!(plain("0xFFFFFFFFFFFFFFFFFF"), big("0xFFFFFFFFFFFFFFFFFF"));
        assert_eq!(plain("+0xFFFFFFFFFFFFFFFFFF"), big("+0xFFFFFFFFFFFFFFFFFF"));
        assert_eq!(
            plain("0o7777777777777777777777"),
            big("0o7777777777777777777777")
        );
    }

    #[test]
    fn invalid_radix_digits_stay_strings() {
        for s in [
            "0o9",
            "0o78",
            "0o7777777777777777777778",
            "0xG",
            "0xFF_FF",
            "0x1_0",
            "0xFFFFFFFFFFFFFFFFFFG",
        ] {
            assert_eq!(plain(s), Str(s), "plain {s}");
            assert_eq!(tagged(s, ScalarStyle::Plain, "int"), Str(s), "!!int {s}");
        }
        assert_eq!(
            tagged("0xFFFFFFFFFFFFFFFFFF", ScalarStyle::Plain, "float"),
            Str("0xFFFFFFFFFFFFFFFFFF")
        );
    }

    #[test]
    fn radix_leading_zeros_are_insignificant() {
        assert_eq!(plain("0x0000000000000000000000FF"), Int(255));
        assert_eq!(plain("0o000000000000000000000017"), Int(15));
        assert_eq!(plain("0x0"), Int(0));
        assert_eq!(plain("-0x0"), Int(0));
    }

    #[test]
    fn quoted_scalars_are_strings() {
        for style in [
            ScalarStyle::SingleQuoted,
            ScalarStyle::DoubleQuoted,
            ScalarStyle::Literal,
            ScalarStyle::Folded,
        ] {
            assert_eq!(resolve_scalar("true", style, None), Str("true"));
            assert_eq!(resolve_scalar("7", style, None), Str("7"));
            assert_eq!(resolve_scalar("", style, None), Str(""));
        }
    }

    #[test]
    fn tagged_int_truncates_floats() {
        let p = ScalarStyle::Plain;
        assert_eq!(tagged("3.0", p, "int"), Int(3));
        assert_eq!(tagged("-2.7", p, "int"), Int(-2));
        assert_eq!(tagged("1.0e2", p, "int"), Int(100));
        assert_eq!(tagged(".nan", p, "int"), Str(".nan"));
        assert_eq!(tagged(".inf", p, "int"), Str(".inf"));
        assert_eq!(tagged("1.0e20", p, "int"), Str("1.0e20"));
    }

    #[test]
    fn tagged_bigint() {
        let p = ScalarStyle::Plain;
        assert_eq!(
            tagged("99999999999999999999", p, "int"),
            big("99999999999999999999")
        );
        assert_eq!(
            tagged("-99999999999999999999", p, "int"),
            big("-99999999999999999999")
        );
        assert_eq!(
            tagged("0xFFFFFFFFFFFFFFFFFF", p, "int"),
            big("0xFFFFFFFFFFFFFFFFFF")
        );
    }

    #[test]
    fn failed_core_coercion_yields_string() {
        let p = ScalarStyle::Plain;
        assert_eq!(tagged("true", p, "int"), Str("true"));
        assert_eq!(tagged("1", p, "null"), Str("1"));
        assert_eq!(tagged("12", p, "binary"), Str("12"));
        assert_eq!(tagged("0x1F", p, "float"), Str("0x1F"));
        assert_eq!(tagged("yes", p, "bool"), Str("yes"));
    }

    #[test]
    fn non_specific_tag_forces_string() {
        let bang = Tag::new("", "!");
        for (s, style) in [
            ("42", ScalarStyle::Plain),
            ("x", ScalarStyle::DoubleQuoted),
            ("", ScalarStyle::Plain),
        ] {
            assert_eq!(resolve_scalar(s, style, Some(&bang)), Str(s));
        }
    }

    #[test]
    fn local_tag_falls_back_to_implicit() {
        let local = Tag::new("!", "foo");
        for (style, expected) in [
            (ScalarStyle::Plain, Int(42)),
            (ScalarStyle::DoubleQuoted, Str("42")),
            (ScalarStyle::SingleQuoted, Str("42")),
        ] {
            assert_eq!(
                resolve_scalar("42", style, Some(&local)),
                expected,
                "{style:?}"
            );
        }
    }

    #[test]
    fn i64_boundaries() {
        const STYLES: [ScalarStyle; 3] = [
            ScalarStyle::Plain,
            ScalarStyle::DoubleQuoted,
            ScalarStyle::SingleQuoted,
        ];
        let cases = [
            ("9223372036854775807", Int(i64::MAX)),
            ("+9223372036854775807", Int(i64::MAX)),
            ("9223372036854775808", big("9223372036854775808")),
            ("+9223372036854775808", big("+9223372036854775808")),
            ("-9223372036854775808", Int(i64::MIN)),
            ("-9223372036854775809", big("-9223372036854775809")),
            ("9223372036854776000", big("9223372036854776000")),
            ("0x7FFFFFFFFFFFFFFF", Int(i64::MAX)),
            ("0x8000000000000000", big("0x8000000000000000")),
            ("-0x8000000000000000", Int(i64::MIN)),
            ("-0x8000000000000001", big("-0x8000000000000001")),
            ("0o777777777777777777777", Int(i64::MAX)),
            ("0o1000000000000000000000", big("0o1000000000000000000000")),
            ("-0o1000000000000000000000", Int(i64::MIN)),
        ];
        for (s, expected) in cases {
            for style in STYLES {
                let resolved = resolve_scalar(s, style, Some(&core("int")));
                assert_eq!(resolved, expected, "!!int {s} {style:?}");
            }
            assert_eq!(plain(s), expected, "plain {s}");
        }
        assert_eq!(
            resolve_scalar("9223372036854775808", ScalarStyle::DoubleQuoted, None),
            Str("9223372036854775808")
        );
        assert_eq!(
            tagged("9.2233720368547758e18", ScalarStyle::Plain, "int"),
            Str("9.2233720368547758e18")
        );
        assert_eq!(
            tagged("-9.2233720368547758e18", ScalarStyle::Plain, "int"),
            Int(i64::MIN)
        );
    }

    #[test]
    fn sign_leniency_rejected() {
        for s in [
            "+-5", "-+5", "--5", "++5", "0x-1", "0x+1", "-0x-1", "0o-7", "0o+7", "+", "-", "0x",
            "0o",
        ] {
            assert_eq!(plain(s), Str(s), "plain {s}");
            assert_eq!(tagged(s, ScalarStyle::Plain, "int"), Str(s), "!!int {s}");
        }
        assert_eq!(plain("-0x1F"), Int(-31));
        assert_eq!(plain("+0o17"), Int(15));
    }

    #[test]
    fn core_tags_apply_on_every_style() {
        for style in [
            ScalarStyle::Plain,
            ScalarStyle::SingleQuoted,
            ScalarStyle::DoubleQuoted,
            ScalarStyle::Literal,
            ScalarStyle::Folded,
        ] {
            assert_eq!(tagged("7", style, "int"), Int(7));
            assert_eq!(tagged("1.5", style, "float"), Float(1.5));
            assert_eq!(tagged("true", style, "bool"), Bool(true));
            assert_eq!(tagged("null", style, "null"), Null);
            assert_eq!(tagged("7", style, "str"), Str("7"));
            assert_eq!(tagged("True", style, "bool"), Bool(true));
            assert_eq!(tagged("FALSE", style, "bool"), Bool(false));
            assert_eq!(tagged("NULL", style, "null"), Null);
        }
    }

    #[test]
    fn empty_value_with_core_tag() {
        let p = ScalarStyle::Plain;
        assert_eq!(tagged("", p, "null"), Null);
        assert_eq!(tagged("", p, "str"), Str(""));
        for suffix in ["int", "bool", "float"] {
            assert_eq!(tagged("", p, suffix), Str(""), "!!{suffix}");
        }
    }

    #[test]
    fn unsupported_core_suffix_yields_string() {
        for suffix in ["foo", "seq", "timestamp", "python/name:foo"] {
            assert_eq!(
                tagged("7", ScalarStyle::Plain, suffix),
                Str("7"),
                "!!{suffix}"
            );
        }
    }
}
