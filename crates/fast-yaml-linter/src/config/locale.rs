//! The top-level `locale` key of a config file.

/// A `locale` value, as far as `key-ordering` can honor it.
///
/// yamllint orders keys with the collation of the locale; fast-yaml compares code points, which
/// is what the `C`, `POSIX` and `C.UTF-8` locales do. Any other locale is kept as written and
/// rejected only when `key-ordering` is enabled, because no other rule reads it.
///
/// # Examples
///
/// ```
/// use fast_yaml_linter::config::LocaleName;
///
/// assert!(LocaleName::from("C.utf8").is_code_point_order());
/// assert!(LocaleName::from("posix").is_code_point_order());
/// assert!(!LocaleName::from("en_US.UTF-8").is_code_point_order());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocaleName {
    /// The `C` locale.
    C,
    /// The `POSIX` locale.
    Posix,
    /// The `C.UTF-8` locale, also spelled `C.utf8`.
    CUtf8,
    /// Any other locale, as written.
    Other(String),
}

impl LocaleName {
    /// Returns whether the locale orders strings by code point, as `key-ordering` does.
    #[must_use]
    pub const fn is_code_point_order(&self) -> bool {
        !matches!(self, Self::Other(_))
    }
}

impl From<&str> for LocaleName {
    fn from(name: &str) -> Self {
        if name.eq_ignore_ascii_case("c") {
            Self::C
        } else if name.eq_ignore_ascii_case("posix") {
            Self::Posix
        } else if name.eq_ignore_ascii_case("c.utf-8") || name.eq_ignore_ascii_case("c.utf8") {
            Self::CUtf8
        } else {
            Self::Other(name.to_owned())
        }
    }
}
