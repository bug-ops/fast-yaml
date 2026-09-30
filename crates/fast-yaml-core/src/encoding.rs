//! Byte-level input decoding shared by every entry point that reads raw bytes.
//!
//! Parsing APIs take `&str`, so UTF-16 and UTF-32 input is rejected while turning
//! bytes into text. [`decode_input`] is the single place that does this and
//! reports a precise [`DecodeError`] instead of a generic UTF-8 failure.

use std::str::Utf8Error;

use thiserror::Error;

/// A non-UTF-8 encoding recognised from its byte order mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum UnsupportedEncoding {
    /// UTF-16 little endian (`FF FE`).
    Utf16Le,
    /// UTF-16 big endian (`FE FF`).
    Utf16Be,
    /// UTF-32 little endian (`FF FE 00 00`).
    Utf32Le,
    /// UTF-32 big endian (`00 00 FE FF`).
    Utf32Be,
}

impl UnsupportedEncoding {
    /// Detects a UTF-16 or UTF-32 byte order mark at the start of `bytes`.
    ///
    /// The UTF-32 marks are checked first because the UTF-32LE mark starts with
    /// the UTF-16LE mark. A UTF-8 byte order mark is not reported.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::UnsupportedEncoding;
    ///
    /// assert_eq!(
    ///     UnsupportedEncoding::detect(&[0xFF, 0xFE, 0x00, 0x00]),
    ///     Some(UnsupportedEncoding::Utf32Le),
    /// );
    /// assert_eq!(
    ///     UnsupportedEncoding::detect(&[0xFF, 0xFE, b'a', 0x00]),
    ///     Some(UnsupportedEncoding::Utf16Le),
    /// );
    /// assert_eq!(UnsupportedEncoding::detect(b"a: 1"), None);
    /// ```
    #[must_use]
    pub const fn detect(bytes: &[u8]) -> Option<Self> {
        match bytes {
            [0xFF, 0xFE, 0x00, 0x00, ..] => Some(Self::Utf32Le),
            [0x00, 0x00, 0xFE, 0xFF, ..] => Some(Self::Utf32Be),
            [0xFF, 0xFE, ..] => Some(Self::Utf16Le),
            [0xFE, 0xFF, ..] => Some(Self::Utf16Be),
            _ => None,
        }
    }

    /// Returns the conventional encoding name, for example `UTF-16LE`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Utf16Le => "UTF-16LE",
            Self::Utf16Be => "UTF-16BE",
            Self::Utf32Le => "UTF-32LE",
            Self::Utf32Be => "UTF-32BE",
        }
    }
}

impl std::fmt::Display for UnsupportedEncoding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Utf32Le => write!(
                f,
                "{} (or UTF-16LE text starting with a NUL character)",
                self.name()
            ),
            _ => f.write_str(self.name()),
        }
    }
}

/// Error returned when raw input bytes cannot be used as YAML text.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The input starts with a UTF-16 or UTF-32 byte order mark.
    #[error(
        "unsupported encoding: input starts with a {0} byte order mark; only UTF-8 is supported (convert the file to UTF-8, for example with iconv)"
    )]
    UnsupportedEncoding(UnsupportedEncoding),

    /// The input is not valid UTF-8.
    #[error("input is not valid UTF-8: {0}")]
    InvalidUtf8(Utf8Error),
}

impl From<Utf8Error> for DecodeError {
    fn from(err: Utf8Error) -> Self {
        Self::InvalidUtf8(err)
    }
}

/// Decodes raw input bytes into YAML text.
///
/// A UTF-8 byte order mark is kept in the returned text; it is stripped later by
/// [`strip_bom`](crate::strip_bom) where parsing requires it.
///
/// # Errors
///
/// Returns [`DecodeError::UnsupportedEncoding`] when the input starts with a
/// UTF-16 or UTF-32 byte order mark, and [`DecodeError::InvalidUtf8`] when the
/// bytes are not valid UTF-8.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{DecodeError, UnsupportedEncoding, decode_input};
///
/// assert_eq!(decode_input(b"a: 1")?, "a: 1");
///
/// let utf16 = [0xFF, 0xFE, b'a', 0x00];
/// assert_eq!(
///     decode_input(&utf16),
///     Err(DecodeError::UnsupportedEncoding(UnsupportedEncoding::Utf16Le)),
/// );
/// # Ok::<(), DecodeError>(())
/// ```
pub fn decode_input(bytes: &[u8]) -> Result<&str, DecodeError> {
    reject_unsupported_bom(bytes)?;
    Ok(std::str::from_utf8(bytes)?)
}

/// Decodes owned input bytes into YAML text without copying them.
///
/// Behaves like [`decode_input`] but reuses the allocation of `bytes`, so large
/// inputs are not held in memory twice.
///
/// # Errors
///
/// Returns the same errors as [`decode_input`].
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{DecodeError, decode_input_owned};
///
/// assert_eq!(decode_input_owned(b"a: 1".to_vec())?, "a: 1");
/// assert!(matches!(
///     decode_input_owned(vec![0xFE, 0xFF]),
///     Err(DecodeError::UnsupportedEncoding(_)),
/// ));
/// # Ok::<(), DecodeError>(())
/// ```
pub fn decode_input_owned(bytes: Vec<u8>) -> Result<String, DecodeError> {
    reject_unsupported_bom(&bytes)?;
    String::from_utf8(bytes).map_err(|err| DecodeError::InvalidUtf8(err.utf8_error()))
}

fn reject_unsupported_bom(bytes: &[u8]) -> Result<(), DecodeError> {
    UnsupportedEncoding::detect(bytes).map_or(Ok(()), |encoding| {
        Err(DecodeError::UnsupportedEncoding(encoding))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_each_bom() {
        for (bytes, expected) in [
            (&[0xFF, 0xFE, b'a', 0x00][..], UnsupportedEncoding::Utf16Le),
            (&[0xFE, 0xFF, 0x00, b'a'], UnsupportedEncoding::Utf16Be),
            (
                &[0xFF, 0xFE, 0x00, 0x00, b'a', 0, 0, 0],
                UnsupportedEncoding::Utf32Le,
            ),
            (
                &[0x00, 0x00, 0xFE, 0xFF, 0, 0, 0, b'a'],
                UnsupportedEncoding::Utf32Be,
            ),
        ] {
            assert_eq!(
                decode_input(bytes),
                Err(DecodeError::UnsupportedEncoding(expected)),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn bom_only_input_is_rejected() {
        for (bytes, expected) in [
            (&[0xFF, 0xFE][..], UnsupportedEncoding::Utf16Le),
            (&[0xFE, 0xFF], UnsupportedEncoding::Utf16Be),
            (&[0xFF, 0xFE, 0x00, 0x00], UnsupportedEncoding::Utf32Le),
            (&[0x00, 0x00, 0xFE, 0xFF], UnsupportedEncoding::Utf32Be),
        ] {
            assert_eq!(
                decode_input(bytes),
                Err(DecodeError::UnsupportedEncoding(expected))
            );
        }
    }

    #[test]
    fn truncated_marks_are_invalid_utf8() {
        for bytes in [&[0xFE][..], &[0xFF], &[0x00, 0x00, 0xFE]] {
            assert!(
                matches!(decode_input(bytes), Err(DecodeError::InvalidUtf8(_))),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn owned_decode_matches_borrowed() {
        assert_eq!(decode_input_owned(b"a: 1".to_vec()).unwrap(), "a: 1");
        assert_eq!(
            decode_input_owned(b"\xEF\xBB\xBFa: 1".to_vec()).unwrap(),
            "\u{FEFF}a: 1"
        );
        assert_eq!(
            decode_input_owned(vec![0xFF, 0xFE, b'a', 0x00]),
            Err(DecodeError::UnsupportedEncoding(
                UnsupportedEncoding::Utf16Le
            ))
        );
        assert!(matches!(
            decode_input_owned(b"a: \xC3\x28".to_vec()),
            Err(DecodeError::InvalidUtf8(_))
        ));
    }

    #[test]
    fn utf8_bom_is_kept() {
        assert_eq!(decode_input(b"\xEF\xBB\xBFa: 1").unwrap(), "\u{FEFF}a: 1");
        assert_eq!(decode_input(b"\xEF\xBB\xBF").unwrap(), "\u{FEFF}");
    }

    #[test]
    fn plain_and_empty_input_decode() {
        assert_eq!(decode_input(b"").unwrap(), "");
        assert_eq!(
            decode_input("ключ: значение".as_bytes()).unwrap(),
            "ключ: значение"
        );
    }

    #[test]
    fn invalid_utf8_is_not_reported_as_encoding() {
        assert!(matches!(
            decode_input(b"a: \xC3\x28"),
            Err(DecodeError::InvalidUtf8(_))
        ));
        assert!(matches!(
            decode_input(b"\xFF"),
            Err(DecodeError::InvalidUtf8(_))
        ));
    }

    #[test]
    fn message_names_the_encoding() {
        let err = decode_input(&[0xFF, 0xFE, 0x00, 0x00]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "unsupported encoding: input starts with a UTF-32LE (or UTF-16LE text starting with a NUL character) byte order mark; only UTF-8 is supported (convert the file to UTF-8, for example with iconv)"
        );
    }
}
