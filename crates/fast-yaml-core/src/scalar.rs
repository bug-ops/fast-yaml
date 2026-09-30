//! YAML 1.2 core-schema scalar resolution shared by the core loader and the language bindings.
//!
//! [`resolve_scalar`] is the single place that decides which type a scalar has, given its
//! text, style and tag. The core loader ([`crate::canonicalize`]) and the Python bindings
//! both adapt its [`ResolvedScalar`] result to their own value types, so the rules cannot drift
//! between surfaces.

use saphyr_parser::{ScalarStyle, Tag};

/// A decimal integer literal that overflows `i64`.
///
/// Guarantees an optional sign followed by ASCII decimal digits only; it can only be
/// produced by [`resolve_scalar`]. Hex and octal overflows are never represented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecimalBigInt<'a>(&'a str);

impl<'a> DecimalBigInt<'a> {
    /// Returns the literal text, suitable for arbitrary-precision integer parsers.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{ResolvedScalar, resolve_scalar};
    /// use saphyr_parser::ScalarStyle;
    ///
    /// let ResolvedScalar::BigInt(big) =
    ///     resolve_scalar("99999999999999999999", ScalarStyle::Plain, None)
    /// else {
    ///     unreachable!()
    /// };
    /// assert_eq!(big.as_str(), "99999999999999999999");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.0
    }
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
    /// Decimal integer that overflows `i64`.
    BigInt(DecimalBigInt<'a>),
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
    fn of(tag: Option<&Tag>) -> Self {
        let Some(tag) = tag else {
            return Self::Untagged;
        };
        if tag.handle.is_empty() && tag.suffix == "!" {
            return Self::NonSpecific;
        }
        if !tag.is_yaml_core_schema() {
            return Self::Other;
        }
        Self::Core(match tag.suffix.as_str() {
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
///    and null forms, booleans, integers (decimal, `0x`, `0o`), decimal integers overflowing
///    `i64`, then floats; anything else is a string.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{ResolvedScalar, resolve_scalar};
/// use saphyr_parser::{ScalarStyle, Tag};
///
/// // Quoted scalars are strings...
/// assert_eq!(
///     resolve_scalar("7", ScalarStyle::DoubleQuoted, None),
///     ResolvedScalar::Str("7")
/// );
///
/// // ...unless a core-schema tag says otherwise.
/// let int = Tag { handle: "tag:yaml.org,2002:".into(), suffix: "int".into() };
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
/// let bang = Tag { handle: String::new(), suffix: "!".into() };
/// assert_eq!(
///     resolve_scalar("42", ScalarStyle::Plain, Some(&bang)),
///     ResolvedScalar::Str("42")
/// );
/// ```
#[must_use]
pub fn resolve_scalar<'a>(s: &'a str, style: ScalarStyle, tag: Option<&Tag>) -> ResolvedScalar<'a> {
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
        CoreTag::Int => parse_core_schema_int(s)
            .map(ResolvedScalar::Int)
            .or_else(|| decimal_bigint(s))
            .or_else(|| float_str_to_int(s).map(ResolvedScalar::Int)),
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
        _ => parse_core_schema_int(s)
            .map(ResolvedScalar::Int)
            .or_else(|| decimal_bigint(s))
            .or_else(|| parse_core_schema_float(s).map(ResolvedScalar::Float))
            .unwrap_or(ResolvedScalar::Str(s)),
    }
}

fn decimal_bigint(s: &str) -> Option<ResolvedScalar<'_>> {
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .then_some(ResolvedScalar::BigInt(DecimalBigInt(s)))
}

/// Parse a YAML core schema integer: decimal, hex (`0x`), or octal (`0o`).
///
/// Returns `None` for values that overflow `i64` or don't match integer syntax.
fn parse_core_schema_int(s: &str) -> Option<i64> {
    let (neg, digits) = s.strip_prefix('-').map_or_else(
        || (false, s.strip_prefix('+').unwrap_or(s)),
        |rest| (true, rest),
    );
    let radix_digits = |prefix: [&str; 2], radix: u32| {
        let rest = digits
            .strip_prefix(prefix[0])
            .or_else(|| digits.strip_prefix(prefix[1]))?;
        // `from_str_radix` would accept a second sign.
        let raw = rest
            .bytes()
            .all(|b| char::from(b).is_digit(radix))
            .then(|| i64::from_str_radix(rest, radix).ok())
            .flatten();
        Some(raw.and_then(|raw| if neg { raw.checked_neg() } else { Some(raw) }))
    };
    if let Some(parsed) = radix_digits(["0x", "0X"], 16).or_else(|| radix_digits(["0o", "0O"], 8)) {
        return parsed;
    }
    // Parsing the signed text keeps `i64::MIN` representable.
    s.parse::<i64>().ok()
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
        ".inf" | ".Inf" | ".INF" => Some(f64::INFINITY),
        "-.inf" | "-.Inf" | "-.INF" => Some(f64::NEG_INFINITY),
        ".nan" | ".NaN" | ".NAN" => Some(f64::NAN),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ResolvedScalar::{Bool, Float, Int, Null, Str};

    fn core(suffix: &str) -> Tag {
        Tag {
            handle: "tag:yaml.org,2002:".into(),
            suffix: suffix.into(),
        }
    }

    fn big(s: &str) -> ResolvedScalar<'_> {
        ResolvedScalar::BigInt(DecimalBigInt(s))
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
    fn plain_bigint_and_overflow() {
        assert_eq!(plain("99999999999999999999"), big("99999999999999999999"));
        assert_eq!(plain("-99999999999999999999"), big("-99999999999999999999"));
        assert_eq!(plain("+99999999999999999999"), big("+99999999999999999999"));
        assert_eq!(plain("0xFFFFFFFFFFFFFFFFFF"), Str("0xFFFFFFFFFFFFFFFFFF"));
        assert_eq!(
            plain("0o7777777777777777777777"),
            Str("0o7777777777777777777777")
        );
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
            Str("0xFFFFFFFFFFFFFFFFFF")
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
        let bang = Tag {
            handle: String::new(),
            suffix: "!".into(),
        };
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
        let local = Tag {
            handle: "!".into(),
            suffix: "foo".into(),
        };
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
