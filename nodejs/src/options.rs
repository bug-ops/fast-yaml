//! Validation of numeric options received from JavaScript.
//!
//! napi coerces `u32` arguments with `napi_get_value_uint32`, which wraps negatives and
//! truncates fractions. Options are therefore declared as `f64` and validated here.

use std::fmt::Display;

use napi::Status;

/// Upper bound for options that were plain `u32` before validation was introduced.
pub(crate) const U32_MAX: u64 = u32::MAX as u64;

/// Builds the `InvalidArg` error for an option outside `min..=max`.
pub(crate) fn range_error(name: &str, min: u64, max: u64, got: impl Display) -> napi::Error {
    napi::Error::new(
        Status::InvalidArg,
        format!("{name} must be between {min} and {max}, got {got}"),
    )
}

/// Renders a JS number compactly: `Infinity`, `NaN`, and exponent form for huge magnitudes.
fn display_f64(value: f64) -> String {
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if value.abs() >= 1e21 {
        return format!("{value:e}");
    }
    value.to_string()
}

/// Validates that `value` is a finite integer within `min..=max`.
///
/// # Errors
///
/// Returns a `Status::InvalidArg` error naming the option when `value` is `NaN`, infinite,
/// fractional or outside `min..=max`.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub(crate) fn checked_uint(name: &str, value: f64, min: u64, max: u64) -> napi::Result<usize> {
    let in_range =
        value.is_finite() && value.fract() == 0.0 && value >= min as f64 && value <= max as f64;
    if !in_range {
        return Err(range_error(name, min, max, display_f64(value)));
    }
    usize::try_from(value as u64).map_err(|_| range_error(name, min, max, display_f64(value)))
}

/// Validates that `value` is an integer representable as `u32`.
///
/// # Errors
///
/// Same conditions as [`checked_uint`] with the range `0..=u32::MAX`.
pub(crate) fn checked_u32(name: &str, value: f64) -> napi::Result<u32> {
    let n = checked_uint(name, value, 0, U32_MAX)?;
    u32::try_from(n)
        .map_err(|_| napi::Error::new(Status::InvalidArg, format!("{name} exceeds {}", u32::MAX)))
}

/// Applies [`checked_uint`] to an optional option value.
///
/// # Errors
///
/// Propagates the error of [`checked_uint`] when a value is present and invalid.
pub(crate) fn checked_opt_uint(
    name: &str,
    value: Option<f64>,
    min: u64,
    max: u64,
) -> napi::Result<Option<usize>> {
    value.map(|v| checked_uint(name, v, min, max)).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bounds_and_interior() {
        assert_eq!(checked_uint("x", 1.0, 1, 10).unwrap(), 1);
        assert_eq!(checked_uint("x", 10.0, 1, 10).unwrap(), 10);
        assert_eq!(checked_uint("x", 0.0, 0, U32_MAX).unwrap(), 0);
        assert_eq!(
            checked_uint("x", f64::from(u32::MAX), 0, U32_MAX).unwrap(),
            u32::MAX as usize
        );
    }

    #[test]
    fn rejects_invalid_values() {
        for v in [
            -1.0,
            1.5,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.0,
            11.0,
        ] {
            let err = checked_uint("opt", v, 1, 10).unwrap_err();
            assert_eq!(err.status, Status::InvalidArg);
            assert!(err.reason.starts_with("opt must be between 1 and 10, got "));
        }
        assert!(checked_uint("x", 4_294_967_296.0, 0, U32_MAX).is_err());
        assert!(checked_uint("x", -0.5, 0, U32_MAX).is_err());
    }

    #[test]
    fn optional_passes_none_through() {
        assert_eq!(checked_opt_uint("x", None, 1, 2).unwrap(), None);
        assert_eq!(checked_opt_uint("x", Some(2.0), 1, 2).unwrap(), Some(2));
        assert!(checked_opt_uint("x", Some(3.0), 1, 2).is_err());
    }
}
