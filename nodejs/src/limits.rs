//! Validation of JavaScript-supplied limits (`maxDepth`, `maxAliasBytes`, `maxScanAhead`,
//! `maxDocuments`, `maxInputBytes`).
//!
//! JavaScript numbers arrive as `f64`; every value is checked here so that `NaN`,
//! fractions, negatives, zero, and values above the core cap are rejected with the
//! same message shape as the core `LimitRangeError`.

use fast_yaml_core::limits::{
    Bounded, Bounds, LimitRangeError, MaxDocuments, MaxInputBytes, ParseLimits,
};
use napi::Result as NapiResult;

use crate::options::{checked_uint, range_error};

fn core_error(option: &str, e: LimitRangeError) -> napi::Error {
    range_error(option, e.min as u64, e.max as u64, e.value)
}

/// Validates an optional limit of kind `K`, defaulting to [`Bounded::DEFAULT`].
pub(crate) fn bounded<K: Bounds>(option: &str, value: Option<f64>) -> NapiResult<Bounded<K>> {
    value.map_or_else(
        || Ok(Bounded::default()),
        |v| {
            let n = checked_uint(option, v, 1, K::MAX as u64)?;
            Bounded::new(n).map_err(|e| core_error(option, e))
        },
    )
}

/// Rejects the removed `maxInputSize` option so a lowered limit is never silently ignored.
pub(crate) fn reject_legacy_max_input_size(value: Option<f64>) -> NapiResult<()> {
    value.map_or(Ok(()), |_| {
        Err(napi::Error::new(
            napi::Status::InvalidArg,
            "maxInputSize was renamed to maxInputBytes",
        ))
    })
}

/// Validates an optional `maxInputBytes` value, defaulting to [`MaxInputBytes::DEFAULT`].
pub(crate) fn max_input_bytes(value: Option<f64>) -> NapiResult<MaxInputBytes> {
    bounded("maxInputBytes", value)
}

/// Validates an optional `maxDocuments` value, defaulting to [`MaxDocuments::DEFAULT`].
pub(crate) fn max_documents(value: Option<f64>) -> NapiResult<MaxDocuments> {
    bounded("maxDocuments", value)
}

/// Validates the optional `maxDepth` / `maxAliasBytes` / `maxScanAhead` / `maxDocuments` values into [`ParseLimits`].
///
/// Absent values keep the core defaults.
pub(crate) fn parse_limits(
    max_depth_opt: Option<f64>,
    max_alias_bytes_opt: Option<f64>,
    max_scan_ahead_opt: Option<f64>,
    max_documents_opt: Option<f64>,
) -> NapiResult<ParseLimits> {
    Ok(ParseLimits {
        max_depth: bounded("maxDepth", max_depth_opt)?,
        max_alias_bytes: bounded("maxAliasBytes", max_alias_bytes_opt)?,
        max_scan_ahead: bounded("maxScanAhead", max_scan_ahead_opt)?,
        max_documents: max_documents(max_documents_opt)?,
        ..ParseLimits::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fast_yaml_core::limits::MaxScanAhead;

    #[test]
    fn defaults_when_absent() {
        assert_eq!(
            parse_limits(None, None, None, None).unwrap(),
            ParseLimits::default()
        );
    }

    #[test]
    fn accepts_bounds() {
        let l = parse_limits(Some(1.0), Some(1_073_741_824.0), None, None).unwrap();
        assert_eq!(l.max_depth.get(), 1);
        assert_eq!(l.max_alias_bytes.get(), 1 << 30);
        assert_eq!(
            parse_limits(Some(512.0), None, None, None)
                .unwrap()
                .max_depth
                .get(),
            512
        );
    }

    #[test]
    fn rejects_invalid_depth() {
        for v in [0.0, -1.0, 1.5, f64::NAN, f64::INFINITY, 513.0, 1e300] {
            assert!(parse_limits(Some(v), None, None, None).is_err(), "{v}");
        }
    }

    #[test]
    fn rejects_invalid_alias_bytes() {
        for v in [0.0, -1.0, 2.5, f64::NAN, 1_073_741_825.0] {
            assert!(parse_limits(None, Some(v), None, None).is_err(), "{v}");
        }
    }

    #[test]
    fn scan_ahead_defaults_and_bounds() {
        let limits = parse_limits(None, None, Some(1_073_741_824.0), None).unwrap();
        assert_eq!(limits.max_scan_ahead, MaxScanAhead::MAX);
        for v in [0.0, -1.0, 2.5, f64::NAN, 1_073_741_825.0] {
            assert!(parse_limits(None, None, Some(v), None).is_err(), "{v}");
        }
        assert_eq!(
            parse_limits(None, None, Some(0.0), None)
                .unwrap_err()
                .reason,
            "maxScanAhead must be between 1 and 1073741824, got 0"
        );
    }

    #[test]
    fn max_input_bytes_defaults_and_bounds() {
        assert_eq!(max_input_bytes(None).unwrap(), MaxInputBytes::DEFAULT);
        assert_eq!(max_input_bytes(Some(1.0)).unwrap(), MaxInputBytes::MIN);
        assert_eq!(
            max_input_bytes(Some(1_073_741_824.0)).unwrap(),
            MaxInputBytes::MAX
        );
        for v in [0.0, -1.0, 2.5, f64::NAN, 1_073_741_825.0] {
            assert!(max_input_bytes(Some(v)).is_err(), "{v}");
        }
        assert_eq!(
            max_input_bytes(Some(0.0)).unwrap_err().reason,
            "maxInputBytes must be between 1 and 1073741824, got 0"
        );
    }

    #[test]
    fn legacy_max_input_size_is_rejected() {
        assert!(reject_legacy_max_input_size(None).is_ok());
        assert_eq!(
            reject_legacy_max_input_size(Some(1.0)).unwrap_err().reason,
            "maxInputSize was renamed to maxInputBytes"
        );
    }

    #[test]
    fn parse_limits_carries_max_documents() {
        let limits = parse_limits(None, None, None, Some(7.0)).unwrap();
        assert_eq!(limits.max_documents.get(), 7);
        assert!(parse_limits(None, None, None, Some(0.0)).is_err());
    }

    #[test]
    fn max_documents_defaults_and_bounds() {
        assert_eq!(max_documents(None).unwrap(), MaxDocuments::DEFAULT);
        assert_eq!(
            max_documents(Some(10_000_000.0)).unwrap(),
            MaxDocuments::MAX
        );
        for v in [0.0, -1.0, 1.5, f64::NAN, 10_000_001.0] {
            assert!(max_documents(Some(v)).is_err(), "{v}");
        }
        assert_eq!(
            max_documents(Some(0.0)).unwrap_err().reason,
            "maxDocuments must be between 1 and 10000000, got 0"
        );
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
            let e = parse_limits(Some(v), None, None, None).unwrap_err();
            assert_eq!(
                e.reason,
                format!("maxDepth must be between 1 and 512, got {want}")
            );
        }
    }

    #[test]
    fn message_matches_core_shape() {
        let e = parse_limits(Some(0.0), None, None, None).unwrap_err();
        assert_eq!(e.reason, "maxDepth must be between 1 and 512, got 0");
    }
}
