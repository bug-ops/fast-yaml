/// Exit codes for CLI application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ExitCode {
    Success = 0,
    ParseError = 1,
    LintErrors = 2,
    IoError = 3,
    InvalidArgs = 4,
}

impl ExitCode {
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

/// Format error with colored output (if enabled)
pub fn format_error(err: &anyhow::Error, use_color: bool) -> String {
    use std::fmt::Write;
    let mut output = String::new();

    #[cfg(feature = "colors")]
    if use_color {
        use colored::Colorize;
        let _ = writeln!(output, "{} {}", "error:".red().bold(), err);

        // Show error chain
        for (i, cause) in err.chain().skip(1).enumerate() {
            let _ = writeln!(
                output,
                "  {}{} {}",
                "caused by".dimmed(),
                format!("[{i}]").dimmed(),
                cause.to_string().dimmed()
            );
        }
        return output;
    }

    // Fallback for no-color or when colors feature is disabled
    let _ = writeln!(output, "error: {err}");

    for (i, cause) in err.chain().skip(1).enumerate() {
        let _ = writeln!(output, "  caused by[{i}] {cause}");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_code_values() {
        assert_eq!(ExitCode::Success.as_i32(), 0);
        assert_eq!(ExitCode::ParseError.as_i32(), 1);
        assert_eq!(ExitCode::LintErrors.as_i32(), 2);
        assert_eq!(ExitCode::IoError.as_i32(), 3);
        assert_eq!(ExitCode::InvalidArgs.as_i32(), 4);
    }

    #[test]
    fn test_format_error_no_color() {
        let err = anyhow::anyhow!("test error");
        let formatted = format_error(&err, false);
        assert!(formatted.contains("error: test error"));
    }

    #[test]
    fn test_format_error_with_chain() {
        use anyhow::Context;
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        // Context trait applies to Result, not Error directly
        let err: anyhow::Error = Err::<(), _>(io_err)
            .context("Failed to read config")
            .unwrap_err();
        let formatted = format_error(&err, false);
        assert!(formatted.contains("Failed to read config"));
        assert!(formatted.contains("caused by"));
    }
}
