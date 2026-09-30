//! Validation of JavaScript-supplied parse limits (`maxDepth`, `maxAliasBytes`).
//!
//! JavaScript numbers arrive as `f64`; every value is checked here so that `NaN`,
//! fractions, negatives, zero, and values above the core cap are rejected with the
//! same message shape as the core `LimitRangeError`.

use fast_yaml_core::limits::{LimitRangeError, MaxAliasBytes, MaxDepth, ParseLimits};
use napi::Result as NapiResult;

use crate::options::{checked_uint, range_error};

fn core_error(option: &str, e: LimitRangeError) -> napi::Error {
    range_error(option, 1, e.max as u64, e.value)
}

fn max_depth(value: Option<f64>) -> NapiResult<MaxDepth> {
    const OPTION: &str = "maxDepth";
    value.map_or_else(
        || Ok(MaxDepth::default()),
        |v| {
            let n = checked_uint(OPTION, v, 1, MaxDepth::MAX.get() as u64)?;
            MaxDepth::new(n).map_err(|e| core_error(OPTION, e))
        },
    )
}

fn max_alias_bytes(value: Option<f64>) -> NapiResult<MaxAliasBytes> {
    const OPTION: &str = "maxAliasBytes";
    value.map_or_else(
        || Ok(MaxAliasBytes::default()),
        |v| {
            let n = checked_uint(OPTION, v, 1, MaxAliasBytes::MAX.get() as u64)?;
            MaxAliasBytes::new(n).map_err(|e| core_error(OPTION, e))
        },
    )
}

/// Validates the optional `maxDepth` / `maxAliasBytes` pair into [`ParseLimits`].
///
/// Absent values keep the core defaults.
pub(crate) fn parse_limits(
    max_depth_opt: Option<f64>,
    max_alias_bytes_opt: Option<f64>,
) -> NapiResult<ParseLimits> {
    Ok(ParseLimits {
        max_depth: max_depth(max_depth_opt)?,
        max_alias_bytes: max_alias_bytes(max_alias_bytes_opt)?,
        ..ParseLimits::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_absent() {
        assert_eq!(parse_limits(None, None).unwrap(), ParseLimits::default());
    }

    #[test]
    fn accepts_bounds() {
        let l = parse_limits(Some(1.0), Some(1_073_741_824.0)).unwrap();
        assert_eq!(l.max_depth.get(), 1);
        assert_eq!(l.max_alias_bytes.get(), 1 << 30);
        assert_eq!(
            parse_limits(Some(512.0), None).unwrap().max_depth.get(),
            512
        );
    }

    #[test]
    fn rejects_invalid_depth() {
        for v in [0.0, -1.0, 1.5, f64::NAN, f64::INFINITY, 513.0, 1e300] {
            assert!(parse_limits(Some(v), None).is_err(), "{v}");
        }
    }

    #[test]
    fn rejects_invalid_alias_bytes() {
        for v in [0.0, -1.0, 2.5, f64::NAN, 1_073_741_825.0] {
            assert!(parse_limits(None, Some(v)).is_err(), "{v}");
        }
    }

    #[test]
    fn compact_number_rendering() {
        for (v, want) in [
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            (f64::NAN, "NaN"),
            (1e300, "1e300"),
            (1.5, "1.5"),
            (-1.0, "-1"),
        ] {
            let e = parse_limits(Some(v), None).unwrap_err();
            assert_eq!(
                e.reason,
                format!("maxDepth must be between 1 and 512, got {want}")
            );
        }
    }

    #[test]
    fn message_matches_core_shape() {
        let e = parse_limits(Some(0.0), None).unwrap_err();
        assert_eq!(e.reason, "maxDepth must be between 1 and 512, got 0");
    }
}
