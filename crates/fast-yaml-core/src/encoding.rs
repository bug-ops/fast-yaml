//! Byte-level input decoding shared by every entry point that reads raw bytes.
//!
//! Parsing APIs take `&str`, so UTF-16 and UTF-32 input is rejected while turning
//! bytes into text. [`decode_input`] is the single place that does this and
//! reports a precise [`DecodeError`] instead of a generic UTF-8 failure. The encoding is
//! recognised the way YAML 1.2.2 section 5.2 describes: from a byte order mark, or else from the
//! pattern of null bytes in the first four bytes.

use std::str::Utf8Error;

use thiserror::Error;

/// A non-UTF-8 encoding recognised from its byte order mark or its null-byte pattern.
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

/// What revealed an [`UnsupportedEncoding`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EncodingEvidence {
    /// The input starts with a byte order mark.
    ByteOrderMark,
    /// The first four bytes have the null-byte pattern of the encoding (YAML 1.2.2 section 5.2).
    NullBytePattern,
}

impl UnsupportedEncoding {
    /// Detects a UTF-16 or UTF-32 encoding from the start of `bytes`.
    ///
    /// Byte order marks win over null-byte patterns, and the UTF-32 forms are checked before the
    /// UTF-16 ones because the UTF-32LE mark starts with the UTF-16LE mark. Without a mark, the
    /// first four bytes decide, with `x` standing for any non-zero byte: `00 00 00 x` is UTF-32BE,
    /// `00 x 00 x` UTF-16BE, `x 00 00 00` UTF-32LE and `x 00 x 00` UTF-16LE. A UTF-8 byte order
    /// mark is not reported, and input shorter than four bytes has no pattern.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::{EncodingEvidence, UnsupportedEncoding};
    ///
    /// assert_eq!(
    ///     UnsupportedEncoding::detect(&[0xFF, 0xFE, 0x00, 0x00]),
    ///     Some((UnsupportedEncoding::Utf32Le, EncodingEvidence::ByteOrderMark)),
    /// );
    /// assert_eq!(
    ///     UnsupportedEncoding::detect(&[b'a', 0x00, b':', 0x00]),
    ///     Some((UnsupportedEncoding::Utf16Le, EncodingEvidence::NullBytePattern)),
    /// );
    /// assert_eq!(UnsupportedEncoding::detect(b"a: 1"), None);
    /// ```
    #[must_use]
    pub const fn detect(bytes: &[u8]) -> Option<(Self, EncodingEvidence)> {
        use EncodingEvidence::{ByteOrderMark, NullBytePattern};
        match bytes {
            [0xFF, 0xFE, 0x00, 0x00, ..] => Some((Self::Utf32Le, ByteOrderMark)),
            [0x00, 0x00, 0xFE, 0xFF, ..] => Some((Self::Utf32Be, ByteOrderMark)),
            [0xFF, 0xFE, ..] => Some((Self::Utf16Le, ByteOrderMark)),
            [0xFE, 0xFF, ..] => Some((Self::Utf16Be, ByteOrderMark)),
            [0x00, 0x00, 0x00, 1..=255, ..] => Some((Self::Utf32Be, NullBytePattern)),
            [1..=255, 0x00, 0x00, 0x00, ..] => Some((Self::Utf32Le, NullBytePattern)),
            [0x00, 1..=255, 0x00, 1..=255, ..] => Some((Self::Utf16Be, NullBytePattern)),
            [1..=255, 0x00, 1..=255, 0x00, ..] => Some((Self::Utf16Le, NullBytePattern)),
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
        f.write_str(self.name())
    }
}

impl EncodingEvidence {
    /// Describes how `encoding` was recognised, for error messages.
    fn describe(self, encoding: UnsupportedEncoding) -> String {
        match (self, encoding) {
            (Self::ByteOrderMark, UnsupportedEncoding::Utf32Le) => format!(
                "input starts with a {encoding} (or UTF-16LE text starting with a NUL character) byte order mark"
            ),
            (Self::ByteOrderMark, _) => {
                format!("input starts with a {encoding} byte order mark")
            }
            (Self::NullBytePattern, _) => {
                format!("input looks like {encoding}, judging by the null bytes at its start")
            }
        }
    }
}

/// Error returned when raw input bytes cannot be used as YAML text.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The input is UTF-16 or UTF-32, recognised from a byte order mark or a null-byte pattern.
    #[error(
        "unsupported encoding: {}; only UTF-8 is supported (convert the file to UTF-8, for example with iconv)",
        .evidence.describe(*.encoding)
    )]
    UnsupportedEncoding {
        /// The recognised encoding.
        encoding: UnsupportedEncoding,
        /// What revealed it.
        evidence: EncodingEvidence,
    },

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
/// [`NormalizedInput`](crate::NormalizedInput) where parsing requires it.
///
/// # Errors
///
/// Returns [`DecodeError::UnsupportedEncoding`] when the input is recognised as UTF-16 or
/// UTF-32 from a byte order mark or its leading null bytes, and [`DecodeError::InvalidUtf8`] when the
/// bytes are not valid UTF-8.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{DecodeError, EncodingEvidence, UnsupportedEncoding, decode_input};
///
/// assert_eq!(decode_input(b"a: 1")?, "a: 1");
///
/// let utf16 = [0xFF, 0xFE, b'a', 0x00];
/// assert_eq!(
///     decode_input(&utf16),
///     Err(DecodeError::UnsupportedEncoding {
///         encoding: UnsupportedEncoding::Utf16Le,
///         evidence: EncodingEvidence::ByteOrderMark,
///     }),
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
///     Err(DecodeError::UnsupportedEncoding { .. }),
/// ));
/// # Ok::<(), DecodeError>(())
/// ```
pub fn decode_input_owned(bytes: Vec<u8>) -> Result<String, DecodeError> {
    reject_unsupported_bom(&bytes)?;
    String::from_utf8(bytes).map_err(|err| DecodeError::InvalidUtf8(err.utf8_error()))
}

fn reject_unsupported_bom(bytes: &[u8]) -> Result<(), DecodeError> {
    UnsupportedEncoding::detect(bytes).map_or(Ok(()), |(encoding, evidence)| {
        Err(DecodeError::UnsupportedEncoding { encoding, evidence })
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
                Err(DecodeError::UnsupportedEncoding {
                    encoding: expected,
                    evidence: EncodingEvidence::ByteOrderMark
                }),
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
                Err(DecodeError::UnsupportedEncoding {
                    encoding: expected,
                    evidence: EncodingEvidence::ByteOrderMark
                })
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
            Err(DecodeError::UnsupportedEncoding {
                encoding: UnsupportedEncoding::Utf16Le,
                evidence: EncodingEvidence::ByteOrderMark
            })
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

    #[test]
    fn detects_each_null_byte_pattern() {
        use EncodingEvidence::NullBytePattern;
        for (bytes, expected) in [
            (&[0x00, 0x00, 0x00, b'a'][..], UnsupportedEncoding::Utf32Be),
            (&[b'a', 0x00, 0x00, 0x00], UnsupportedEncoding::Utf32Le),
            (&[0x00, b'a', 0x00, b':'], UnsupportedEncoding::Utf16Be),
            (&[b'a', 0x00, b':', 0x00], UnsupportedEncoding::Utf16Le),
            (
                &[0x00, 0x00, 0x00, 0x01, 0, 0, 0, 0x0A],
                UnsupportedEncoding::Utf32Be,
            ),
            (&[b'a', 0, b'\n', 0, b'b', 0], UnsupportedEncoding::Utf16Le),
        ] {
            assert_eq!(
                decode_input(bytes),
                Err(DecodeError::UnsupportedEncoding {
                    encoding: expected,
                    evidence: NullBytePattern
                }),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn byte_order_marks_win_over_patterns() {
        assert_eq!(
            UnsupportedEncoding::detect(&[0xFF, 0xFE, b'a', 0x00]),
            Some((
                UnsupportedEncoding::Utf16Le,
                EncodingEvidence::ByteOrderMark
            ))
        );
        assert_eq!(
            UnsupportedEncoding::detect(&[0x00, 0x00, 0xFE, 0xFF]),
            Some((
                UnsupportedEncoding::Utf32Be,
                EncodingEvidence::ByteOrderMark
            ))
        );
    }

    #[test]
    fn inputs_without_a_pattern_are_not_reported_as_an_encoding() {
        for bytes in [
            &b"a: 1"[..],
            b"",
            b"a\0",
            b"a\0b",
            b"\0\0\0",
            b"\0\0\0\0",
            b"\0a\0",
            b"a\0\0b",
            b"\0a: 1",
            b"\0\0a:",
        ] {
            assert!(
                !matches!(
                    decode_input(bytes),
                    Err(DecodeError::UnsupportedEncoding { .. })
                ),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn null_pattern_message_names_the_encoding_and_the_evidence() {
        let err = decode_input(&[b'a', 0, b':', 0]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "unsupported encoding: input looks like UTF-16LE, judging by the null bytes at its start; only UTF-8 is supported (convert the file to UTF-8, for example with iconv)"
        );
    }
}
