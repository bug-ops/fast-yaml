use thiserror::Error;

/// Errors that can occur during FFI conversions.
#[derive(Error, Debug)]
pub enum FfiError {
    /// Failed to convert between Rust and foreign types.
    #[error("conversion error: {0}")]
    Conversion(String),

    /// Unsupported type encountered during conversion.
    #[error("unsupported type: {0}")]
    UnsupportedType(String),

    /// Type mismatch during conversion.
    #[error("type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        /// The expected type name.
        expected: String,
        /// The actual type name encountered.
        actual: String,
    },
}

/// Result type for FFI operations.
pub type FfiResult<T> = std::result::Result<T, FfiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_error_display() {
        let err = FfiError::Conversion("test error".to_string());
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn test_type_mismatch_display() {
        let err = FfiError::TypeMismatch {
            expected: "String".to_string(),
            actual: "Integer".to_string(),
        };
        assert!(err.to_string().contains("String"));
        assert!(err.to_string().contains("Integer"));
    }
}
