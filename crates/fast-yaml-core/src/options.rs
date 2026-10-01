//! Load-time policy: how mapping keys compare and what a repeated `<<` means.
//!
//! [`LoadOptions`] is passed to the loader so every surface reports a key collision or a repeated
//! merge key from the same place, with the position of the offending key.

/// Which keys a host format treats as the same key.
///
/// YAML keeps `1`, `"1"`, `true` and `1.0` distinct. JSON objects, JavaScript objects and Python
/// dicts do not, so converting to them would silently drop one of the entries. Loading with the
/// domain of the target makes the loader report such a pair as a positioned
/// [`KeyError`](crate::KeyError) instead.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{KeyDomain, LoadOptions, ParseError, Parser};
/// use fast_yaml_core::limits::ParseLimits;
///
/// let options = LoadOptions::new().with_keys(KeyDomain::StringKeys);
/// let err = Parser::parse_all_with_options("1: a\n\"1\": b\n", &ParseLimits::default(), options)
///     .unwrap_err();
/// assert!(matches!(err, ParseError::Key { line: 2, column: 1, .. }));
///
/// let yaml = Parser::parse_all("1: a\n\"1\": b\n").unwrap();
/// assert_eq!(yaml.len(), 1);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyDomain {
    /// YAML identity: keys of different types never collide.
    #[default]
    Yaml,
    /// String keys (JSON and JavaScript): keys and set members that share their canonical text
    /// (see [`Value::key_text`](crate::Value::key_text)) but differ in type collide.
    StringKeys,
    /// Python hashing: booleans, integers and integral floats that are numerically equal collide
    /// when they differ in type (`1`, `true` and `1.0`).
    Python,
}

/// How a mapping with more than one `<<` key is treated.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{DuplicateMergeKeys, LoadOptions, MergeError, ParseError, Parser};
/// use fast_yaml_core::limits::ParseLimits;
///
/// let yaml = "a: &a {x: 1}\nb: &b {y: 2}\nc: {<<: *a, <<: *b}\n";
/// let err = Parser::parse_all(yaml).unwrap_err();
/// assert!(matches!(err, ParseError::Merge { error: MergeError::DuplicateKey, .. }));
///
/// let lenient = LoadOptions::new().with_duplicate_merge_keys(DuplicateMergeKeys::LastWins);
/// assert!(Parser::parse_all_with_options(yaml, &ParseLimits::default(), lenient).is_ok());
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DuplicateMergeKeys {
    /// A repeated `<<` is a [`MergeError::DuplicateKey`](crate::MergeError::DuplicateKey).
    #[default]
    Reject,
    /// The last `<<` wins and the earlier ones are ignored, so a linter can report the repeat as
    /// a diagnostic instead of failing the whole file.
    LastWins,
}

/// Policy applied while loading documents.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::{DuplicateMergeKeys, KeyDomain, LoadOptions};
///
/// let options = LoadOptions::new().with_keys(KeyDomain::Python);
/// assert_eq!(options.keys, KeyDomain::Python);
/// assert_eq!(options.duplicate_merge_keys, DuplicateMergeKeys::Reject);
/// assert_eq!(LoadOptions::default(), LoadOptions::new());
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct LoadOptions {
    /// Which keys count as the same key.
    pub keys: KeyDomain,
    /// Treatment of a repeated `<<` key.
    pub duplicate_merge_keys: DuplicateMergeKeys,
}

impl LoadOptions {
    /// Creates the default options: YAML key identity, a repeated `<<` rejected.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            keys: KeyDomain::Yaml,
            duplicate_merge_keys: DuplicateMergeKeys::Reject,
        }
    }

    /// Sets the key domain.
    #[must_use]
    pub const fn with_keys(mut self, keys: KeyDomain) -> Self {
        self.keys = keys;
        self
    }

    /// Sets how a repeated `<<` is treated.
    #[must_use]
    pub const fn with_duplicate_merge_keys(mut self, policy: DuplicateMergeKeys) -> Self {
        self.duplicate_merge_keys = policy;
        self
    }
}
