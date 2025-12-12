use thiserror::Error;

/// Errors that can occur during YAML parsing.
#[derive(Error, Debug)]
pub enum ParseError {
    /// YAML syntax error with location information.
    #[error("YAML parse error at line {line}, column {column}: {message}")]
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },

    /// Invalid float value encountered.
    #[error("invalid float value '{value}': {source}")]
    InvalidFloat {
        value: String,
        #[source]
        source: std::num::ParseFloatError,
    },

    /// YAML scanner error from yaml-rust2.
    #[error("YAML scanner error: {0}")]
    Scanner(#[from] yaml_rust2::ScanError),
}

/// Errors that can occur during YAML emission.
#[derive(Error, Debug)]
pub enum EmitError {
    /// General emission error.
    #[error("failed to emit YAML: {0}")]
    Emit(String),

    /// Attempted to serialize an unsupported type.
    #[error("unsupported type for serialization: {0}")]
    UnsupportedType(String),
}

/// Result type for parsing operations.
pub type ParseResult<T> = std::result::Result<T, ParseError>;

/// Result type for emission operations.
pub type EmitResult<T> = std::result::Result<T, EmitError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_error_display() {
        let err = ParseError::Syntax {
            line: 10,
            column: 5,
            message: "unexpected token".to_string(),
        };
        assert!(err.to_string().contains("line 10"));
        assert!(err.to_string().contains("column 5"));
    }

    #[test]
    fn test_emit_error_display() {
        let err = EmitError::UnsupportedType("CustomType".to_string());
        assert!(err.to_string().contains("CustomType"));
    }
}
