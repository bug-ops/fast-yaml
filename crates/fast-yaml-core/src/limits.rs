//! Resource limits for parsing untrusted YAML.
//!
//! The limit guard observes the parser event stream before any tree is built, so
//! pathological input (deep nesting, alias amplification) is rejected while memory
//! and stack usage are still bounded.

use crate::error::{ParseError, ParseResult, SourcePosition, SyntaxError};
use saphyr_parser::{Event, ScanError, Span, Tag};
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use thiserror::Error;

/// A limit was constructed with a value outside its permitted range.
///
/// The `Display` form is `must be between {min} and {max}, got {value}`; callers prefix it with
/// the name of the option they are validating.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxDepth;
///
/// let err = MaxDepth::new(0).unwrap_err();
/// assert_eq!(err.to_string(), "must be between 1 and 512, got 0");
/// ```
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
#[error("must be between {min} and {max}, got {value}")]
pub struct LimitRangeError {
    /// The rejected value.
    pub value: usize,
    /// The smallest accepted value.
    pub min: usize,
    /// The largest accepted value.
    pub max: usize,
}

mod sealed {
    pub trait Sealed {}
}

/// Marker describing one configurable, range-checked limit: its largest value, default and name.
///
/// Sealed: the set of bounded limits is fixed by this crate. Every bounded limit accepts values
/// from `1` to [`MAX`](Self::MAX) inclusive, so zero can never be mistaken for "unlimited".
pub trait Bounds: sealed::Sealed {
    /// Largest accepted value.
    const MAX: usize;
    /// Value used when the caller does not choose one.
    const DEFAULT: usize;
    /// Type name shown by `Debug`.
    const NAME: &'static str;
}

/// A limit of kind `K`, guaranteed to lie between `1` and `K::MAX` inclusive.
///
/// Limits of different kinds are distinct types, so a [`MaxInputBytes`] can never be passed where
/// a [`MaxAliasBytes`] is expected. Use the aliases [`MaxDepth`], [`MaxAliasBytes`],
/// [`MaxInputBytes`] and [`MaxDocuments`] rather than naming `Bounded` directly.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxDocuments;
///
/// assert_eq!(MaxDocuments::default(), MaxDocuments::DEFAULT);
/// assert_eq!(MaxDocuments::new(3).unwrap().get(), 3);
/// assert!(MaxDocuments::new(0).is_err());
/// assert!(MaxDocuments::new(MaxDocuments::MAX.get() + 1).is_err());
/// ```
pub struct Bounded<K: Bounds>(usize, PhantomData<K>);

impl<K: Bounds> Bounded<K> {
    /// Default limit for this kind.
    pub const DEFAULT: Self = Self(K::DEFAULT, PhantomData);

    /// Largest accepted limit for this kind.
    pub const MAX: Self = Self(K::MAX, PhantomData);

    /// Smallest accepted limit: one.
    pub const MIN: Self = Self(1, PhantomData);

    pub(crate) const UNBOUNDED: Self = Self(usize::MAX, PhantomData);

    /// Creates a limit of `value`.
    ///
    /// # Errors
    ///
    /// Returns [`LimitRangeError`] when `value` is outside `1` to [`MAX`](Self::MAX) inclusive.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::limits::{LimitRangeError, MaxDepth};
    ///
    /// assert_eq!(MaxDepth::new(512), Ok(MaxDepth::MAX));
    /// assert_eq!(MaxDepth::new(513), Err(LimitRangeError { value: 513, min: 1, max: 512 }));
    /// ```
    pub const fn new(value: usize) -> Result<Self, LimitRangeError> {
        if value < 1 || value > K::MAX {
            Err(LimitRangeError {
                value,
                min: 1,
                max: K::MAX,
            })
        } else {
            Ok(Self(value, PhantomData))
        }
    }

    /// Returns the limit as a plain number.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl<K: Bounds> Clone for Bounded<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K: Bounds> Copy for Bounded<K> {}

impl<K: Bounds> PartialEq for Bounded<K> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<K: Bounds> Eq for Bounded<K> {}

impl<K: Bounds> fmt::Debug for Bounded<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple(K::NAME).field(&self.0).finish()
    }
}

impl<K: Bounds> Default for Bounded<K> {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<K: Bounds> fmt::Display for Bounded<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Marker for [`MaxDepth`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {}

/// Marker for [`MaxAliasBytes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasBytes {}

/// Marker for [`MaxInputBytes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputBytes {}

/// Marker for [`MaxDocuments`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Documents {}

impl sealed::Sealed for Depth {}
impl sealed::Sealed for AliasBytes {}
impl sealed::Sealed for InputBytes {}
impl sealed::Sealed for Documents {}

impl Bounds for Depth {
    const MAX: usize = 512;
    const DEFAULT: usize = 256;
    const NAME: &'static str = "MaxDepth";
}

impl Bounds for AliasBytes {
    const MAX: usize = 1 << 30;
    const DEFAULT: usize = 64 * 1024 * 1024;
    const NAME: &'static str = "MaxAliasBytes";
}

impl Bounds for InputBytes {
    const MAX: usize = 1 << 30;
    const DEFAULT: usize = 100 * 1024 * 1024;
    const NAME: &'static str = "MaxInputBytes";
}

impl Bounds for Documents {
    const MAX: usize = 10_000_000;
    const DEFAULT: usize = 100_000;
    const NAME: &'static str = "MaxDocuments";
}

/// Maximum nesting depth of sequences and mappings.
///
/// Valid values lie between `1` and `MAX` (512) inclusive; the default of 256 matches saphyr's
/// flow-nesting cap and keeps recursive consumers well inside small thread stacks.
///
/// The calling thread needs about 1 MiB of stack at the maximum depth (worst case: nested tagged
/// block mappings, roughly 980 KiB measured in release). On 512 KiB or smaller stacks the process
/// can abort, and a stack overflow cannot be caught. The value emitter and the formatter take
/// their own limit from `EmitterConfig::max_depth` (256 by default), so data parsed deeper than
/// that needs a matching `max_depth` to dump; `Emitter::emit_str_with_config` recurses through
/// saphyr and needs about 2 MiB of stack at depth 512 (release).
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxDepth;
///
/// assert_eq!(MaxDepth::default(), MaxDepth::DEFAULT);
/// assert_eq!(MaxDepth::new(8).unwrap().get(), 8);
/// assert!(MaxDepth::new(0).is_err());
/// ```
pub type MaxDepth = Bounded<Depth>;

/// Maximum estimated memory, in bytes, materialized by alias expansion over a whole stream.
///
/// Each expanded node costs [`NODE_BYTES`] plus the length of its scalar and tag text, so both wide
/// and long-scalar amplification are bounded. The budget is shared by all documents of a
/// stream, not reset per document. Valid values lie between `1` and `MAX` (1 GiB, about 16 Mi
/// expanded nodes) inclusive; host objects built from the expansion can cost several times the
/// estimate. A collection that holds anchors is stored once per anchored level; those copies are
/// limited by this value and by [`ANCHOR_COPY_FACTOR`] times the source size.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxAliasBytes;
///
/// assert_eq!(MaxAliasBytes::default(), MaxAliasBytes::DEFAULT);
/// assert_eq!(MaxAliasBytes::new(10).unwrap().get(), 10);
/// assert!(MaxAliasBytes::new(0).is_err());
/// ```
pub type MaxAliasBytes = Bounded<AliasBytes>;

/// Maximum size, in bytes, of a source text accepted for processing.
///
/// Bounds the work done on oversized input. Checks on an in-memory source are not a memory bound;
/// file readers apply it before reading. Valid values lie between `1` and `MAX` (1 GiB)
/// inclusive; the default is 100 MiB.
///
/// # Memory amplification
///
/// This limit bounds input size, not parser memory. The saphyr scanner tokenizes a whole flow
/// collection before it emits the first event, and does so when an implicit key is possible at
/// that position: a stream-root `[..]`, a block-sequence entry `- [..]`, or a collection nested
/// in a flow collection (`{a: [..]}`). Such a collection costs up to about 190 times its input
/// size, reached with two-byte tokens (`[1,1,..]`: 4 MB of input peaks near 750 MB). A flow
/// sequence of one million integers (about 7 MB) peaks near 390 MB, roughly 56 times its input,
/// against about 24 MB for the same sequence after `---` or as a block mapping value
/// (`a: [..]`). The expansion happens
/// before any event is produced, so event-based limits (depth, node count, aliases)
/// cannot bound it; size the input limit with this factor in mind. See issue #553.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxInputBytes;
///
/// assert_eq!(MaxInputBytes::default(), MaxInputBytes::DEFAULT);
/// let max = MaxInputBytes::new(4).unwrap();
/// assert!(max.check(4).is_ok());
/// assert!(max.check(5).is_err());
/// assert!(MaxInputBytes::new(0).is_err());
/// ```
pub type MaxInputBytes = Bounded<InputBytes>;

/// Maximum number of YAML documents accepted from one stream.
///
/// Valid values lie between `1` and `MAX` (10 million) inclusive; the default is 100 000.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxDocuments;
///
/// assert_eq!(MaxDocuments::default().get(), 100_000);
/// assert!(MaxDocuments::new(0).is_err());
/// ```
pub type MaxDocuments = Bounded<Documents>;

impl MaxDepth {
    /// Enters one more container level below `depth` enclosing ones.
    ///
    /// # Errors
    ///
    /// Returns [`LimitKind::Depth`] when `depth` already equals the limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::limits::MaxDepth;
    ///
    /// let max = MaxDepth::new(2).unwrap();
    /// assert_eq!(max.descend(1), Ok(2));
    /// assert!(max.descend(2).is_err());
    /// ```
    pub const fn descend(self, depth: usize) -> Result<usize, LimitKind> {
        if depth >= self.0 {
            Err(LimitKind::Depth(self))
        } else {
            Ok(depth + 1)
        }
    }
}

/// Input larger than the configured [`MaxInputBytes`].
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxInputBytes;
///
/// let err = MaxInputBytes::new(4).unwrap().check(5).unwrap_err();
/// assert_eq!(err.to_string(), "input size 5 bytes exceeds maximum allowed 4 bytes");
/// ```
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
#[error("input size {size} bytes exceeds maximum allowed {limit} bytes")]
pub struct InputTooLarge {
    /// Size of the rejected input in bytes.
    pub size: u64,
    /// The limit that was exceeded.
    pub limit: MaxInputBytes,
}

impl MaxInputBytes {
    /// Checks a source length against the limit.
    ///
    /// # Errors
    ///
    /// Returns [`InputTooLarge`] when `len` exceeds the limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::limits::MaxInputBytes;
    ///
    /// let max = MaxInputBytes::new(8).unwrap();
    /// assert!(max.check(8).is_ok());
    /// assert_eq!(max.check(9).unwrap_err().size, 9);
    /// ```
    pub const fn check(self, len: usize) -> Result<(), InputTooLarge> {
        self.check_file_len(len as u64)
    }

    /// Checks a file length reported by the filesystem against the limit.
    ///
    /// # Errors
    ///
    /// Returns [`InputTooLarge`] when `len` exceeds the limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use fast_yaml_core::limits::MaxInputBytes;
    ///
    /// let max = MaxInputBytes::new(8).unwrap();
    /// assert!(max.check_file_len(8).is_ok());
    /// assert_eq!(max.check_file_len(u64::MAX).unwrap_err().size, u64::MAX);
    /// ```
    pub const fn check_file_len(self, len: u64) -> Result<(), InputTooLarge> {
        if len > self.0 as u64 {
            Err(InputTooLarge {
                size: len,
                limit: self,
            })
        } else {
            Ok(())
        }
    }
}

/// Indentation width of emitted block collections, in spaces.
///
/// Valid values lie between [`MIN`](Self::MIN) and [`MAX`](Self::MAX) inclusive: a block scalar
/// indentation indicator is a single digit.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::Indent;
///
/// assert_eq!(Indent::default(), Indent::DEFAULT);
/// assert_eq!(Indent::new(4).unwrap().get(), 4);
/// assert_eq!(Indent::new(9).unwrap().digit(), '9');
/// assert!(Indent::new(0).is_err());
/// assert!(Indent::new(10).is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Indent(u8);

impl Indent {
    /// Two spaces.
    pub const DEFAULT: Self = Self(2);

    /// Smallest accepted width.
    pub const MIN: Self = Self(1);

    /// Largest accepted width.
    pub const MAX: Self = Self(9);

    /// Creates an indentation of `width` spaces.
    ///
    /// # Errors
    ///
    /// Returns [`LimitRangeError`] when `width` is outside `1..=9`.
    pub const fn new(width: usize) -> Result<Self, LimitRangeError> {
        if width < Self::MIN.0 as usize || width > Self::MAX.0 as usize {
            return Err(LimitRangeError {
                value: width,
                min: Self::MIN.0 as usize,
                max: Self::MAX.0 as usize,
            });
        }
        #[allow(clippy::cast_possible_truncation)]
        Ok(Self(width as u8))
    }

    /// Returns the width as a plain number.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0 as usize
    }

    /// Returns the width as a byte.
    #[must_use]
    pub const fn to_u8(self) -> u8 {
        self.0
    }

    /// Returns the width as the digit of a block scalar indentation indicator.
    #[must_use]
    pub const fn digit(self) -> char {
        (b'0' + self.0) as char
    }
}

impl Default for Indent {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for Indent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Preferred maximum line width of emitted YAML, in characters.
///
/// Valid values lie between [`MIN`](Self::MIN) and [`MAX`](Self::MAX) inclusive.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::Width;
///
/// assert_eq!(Width::default(), Width::DEFAULT);
/// assert_eq!(Width::new(120).unwrap().get(), 120);
/// assert_eq!(Width::new(10).unwrap_err().to_string(), "must be between 20 and 1000, got 10");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Width(u16);

impl Width {
    /// Eighty characters.
    pub const DEFAULT: Self = Self(80);

    /// Smallest accepted width.
    pub const MIN: Self = Self(20);

    /// Largest accepted width.
    pub const MAX: Self = Self(1000);

    /// Creates a line width of `width` characters.
    ///
    /// # Errors
    ///
    /// Returns [`LimitRangeError`] when `width` is outside `20..=1000`.
    pub const fn new(width: usize) -> Result<Self, LimitRangeError> {
        if width < Self::MIN.0 as usize || width > Self::MAX.0 as usize {
            return Err(LimitRangeError {
                value: width,
                min: Self::MIN.0 as usize,
                max: Self::MAX.0 as usize,
            });
        }
        #[allow(clippy::cast_possible_truncation)]
        Ok(Self(width as u16))
    }

    /// Returns the width as a plain number.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0 as usize
    }
}

impl Default for Width {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for Width {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Maximum bytes of resolved `%TAG` prefix text the parser may materialize over a whole stream.
///
/// The parser copies the full prefix into every tagged node, so a long prefix reused by many
/// tags amplifies memory without any alias. Each tag is charged the length of its prefix above
/// [`TAG_PREFIX_ALLOWANCE`]; the budget is shared by all documents of a stream.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxTagBytes;
///
/// assert_eq!(MaxTagBytes::default(), MaxTagBytes::DEFAULT);
/// assert_eq!(MaxTagBytes::new(10).get(), 10);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxTagBytes(usize);

impl MaxTagBytes {
    /// Default budget: 64 MiB of expanded tag prefixes per stream.
    pub const DEFAULT: Self = Self(64 * 1024 * 1024);

    /// Creates a budget of `bytes` expanded prefix bytes.
    #[must_use]
    pub const fn new(bytes: usize) -> Self {
        Self(bytes)
    }

    /// Returns the budget as a plain number.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl Default for MaxTagBytes {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for MaxTagBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Maximum size, in bytes, of an emitted YAML document set.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::{LimitKind, MaxOutputBytes};
///
/// let max = MaxOutputBytes::new(4);
/// assert!(max.check(4).is_ok());
/// assert_eq!(max.check(5), Err(LimitKind::OutputBytes(max)));
/// assert_eq!(MaxOutputBytes::default(), MaxOutputBytes::DEFAULT);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxOutputBytes(usize);

impl MaxOutputBytes {
    /// Default limit: 100 MiB of emitted text.
    pub const DEFAULT: Self = Self(100 * 1024 * 1024);

    /// Creates a limit of `bytes` emitted bytes.
    #[must_use]
    pub const fn new(bytes: usize) -> Self {
        Self(bytes)
    }

    /// Returns the limit as a plain number.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    /// Checks an emitted length against the limit.
    ///
    /// # Errors
    ///
    /// Returns [`LimitKind::OutputBytes`] when `len` exceeds the limit.
    pub const fn check(self, len: usize) -> Result<(), LimitKind> {
        if len > self.0 {
            Err(LimitKind::OutputBytes(self))
        } else {
            Ok(())
        }
    }
}

impl Default for MaxOutputBytes {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for MaxOutputBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Maximum number of YAML nodes materialized while converting host values for dumping.
///
/// The byte estimate of [`DumpBudget`] alone admits about 50M nodes, which costs gigabytes; this
/// count bounds the work and memory of a shared-reference expansion much earlier. A single
/// document of up to this many nodes still dumps.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::MaxDumpNodes;
///
/// assert_eq!(MaxDumpNodes::default(), MaxDumpNodes::DEFAULT);
/// assert_eq!(MaxDumpNodes::new(10).get(), 10);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxDumpNodes(usize);

impl MaxDumpNodes {
    /// Default limit: 16 Mi nodes per dump call.
    pub const DEFAULT: Self = Self(16 * 1024 * 1024);

    /// Creates a limit of `nodes` nodes.
    #[must_use]
    pub const fn new(nodes: usize) -> Self {
        Self(nodes)
    }

    /// Returns the limit as a plain number.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl Default for MaxDumpNodes {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for MaxDumpNodes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Minimum bytes any emitted node occupies: at least one character plus a separator.
pub const MIN_NODE_OUTPUT_BYTES: usize = 2;

/// Running spend of one dump call against [`MaxOutputBytes`] and [`MaxDumpNodes`].
///
/// Converting host values re-walks shared references once per reference, so a small object graph
/// can expand exponentially before anything is emitted. Charging every child node and every
/// scalar's text as it is converted, and every announced child count before allocating for it,
/// stops that expansion early. The byte estimate ([`MIN_NODE_OUTPUT_BYTES`] per node plus text)
/// never exceeds the real output, so it rejects nothing the output-size check would accept; the
/// node count is the tighter bound on work and memory.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::limits::{DumpBudget, LimitKind, MaxDumpNodes, MaxOutputBytes};
///
/// let max = MaxOutputBytes::new(10);
/// let mut budget = DumpBudget::new(max, MaxDumpNodes::new(4));
/// assert!(budget.charge_nodes(3).is_ok());
/// assert!(budget.charge(4).is_ok());
/// assert_eq!(budget.charge(1), Err(LimitKind::OutputBytes(max)));
///
/// let mut budget = DumpBudget::new(max, MaxDumpNodes::new(2));
/// assert_eq!(budget.charge_nodes(3), Err(LimitKind::DumpNodes(MaxDumpNodes::new(2))));
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct DumpBudget {
    bytes: usize,
    nodes: usize,
    max_bytes: MaxOutputBytes,
    max_nodes: MaxDumpNodes,
}

impl DumpBudget {
    /// Creates an unspent budget bounded by `max_bytes` and `max_nodes`.
    #[must_use]
    pub const fn new(max_bytes: MaxOutputBytes, max_nodes: MaxDumpNodes) -> Self {
        Self {
            bytes: 0,
            nodes: 0,
            max_bytes,
            max_nodes,
        }
    }

    /// Spends `bytes` of scalar text.
    ///
    /// # Errors
    ///
    /// Returns [`LimitKind::OutputBytes`] when the total spend would exceed the limit.
    pub const fn charge(&mut self, bytes: usize) -> Result<(), LimitKind> {
        self.bytes = self.bytes.saturating_add(bytes);
        self.max_bytes.check(self.bytes)
    }

    /// Spends `count` nodes, before any storage for them is allocated.
    ///
    /// # Errors
    ///
    /// Returns [`LimitKind::DumpNodes`] when the node total would exceed the count limit, or
    /// [`LimitKind::OutputBytes`] when their minimum output would exceed the byte limit.
    pub const fn charge_nodes(&mut self, count: usize) -> Result<(), LimitKind> {
        self.nodes = self.nodes.saturating_add(count);
        if self.nodes > self.max_nodes.get() {
            return Err(LimitKind::DumpNodes(self.max_nodes));
        }
        self.charge(MIN_NODE_OUTPUT_BYTES.saturating_mul(count))
    }
}

/// The set of limits enforced while parsing.
///
/// # Examples
///
/// ```
/// use fast_yaml_core::Parser;
/// use fast_yaml_core::limits::{MaxDepth, ParseLimits};
///
/// let limits = ParseLimits { max_depth: MaxDepth::new(2).unwrap(), ..ParseLimits::default() };
/// assert!(Parser::parse_str_with_limits("[[1]]", &limits).is_ok());
/// assert!(Parser::parse_str_with_limits("[[[1]]]", &limits).is_err());
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseLimits {
    /// Maximum nesting depth of collections.
    pub max_depth: MaxDepth,
    /// Maximum estimated bytes produced by alias expansion, per stream.
    pub max_alias_bytes: MaxAliasBytes,
    /// Maximum bytes of expanded `%TAG` prefixes, per stream.
    pub max_tag_bytes: MaxTagBytes,
}

/// Alias-expansion and tag-prefix budget shared by every document of one stream.
///
/// Cloning yields a handle to the same counter, not a copy: clones share one limit, so
/// chunks parsed on different threads draw from a single budget. A budget therefore belongs
/// to exactly one stream (one parse call); never reuse it across calls, create a fresh one
/// instead. Whether the stream fits is deterministic; which chunk observes the overrun first
/// depends on scheduling.
///
/// # Examples
///
/// Clones draw from one limit:
///
/// ```
/// use fast_yaml_core::Parser;
/// use fast_yaml_core::limits::{MaxAliasBytes, ParseLimits, StreamBudget};
///
/// let limits = ParseLimits { max_alias_bytes: MaxAliasBytes::new(100).unwrap(), ..ParseLimits::default() };
/// let budget = StreamBudget::new(limits);
/// let clone = budget.clone();
/// let doc = "- &a x\n- *a\n";
/// assert!(Parser::parse_all_with_budget(doc, &budget).is_ok());
/// assert!(Parser::parse_all_with_budget(doc, &clone).is_err());
/// ```
///
/// A fresh budget per call starts from zero:
///
/// ```
/// use fast_yaml_core::Parser;
/// use fast_yaml_core::limits::{MaxAliasBytes, ParseLimits, StreamBudget};
///
/// let limits = ParseLimits { max_alias_bytes: MaxAliasBytes::new(100).unwrap(), ..ParseLimits::default() };
/// let budget = StreamBudget::new(limits);
/// let doc = "- &a x\n- *a\n";
/// assert!(Parser::parse_all_with_budget(doc, &budget).is_ok());
/// assert!(Parser::parse_all_with_budget(doc, &budget).is_err());
/// ```
#[derive(Debug, Clone)]
pub struct StreamBudget {
    limits: ParseLimits,
    used: Arc<StreamUsage>,
}

#[derive(Debug, Default)]
struct StreamUsage {
    alias_bytes: AtomicUsize,
    tag_prefix_bytes: AtomicUsize,
}

fn charge(counter: &AtomicUsize, bytes: usize, max: usize) -> bool {
    let mut used = counter.load(Ordering::Relaxed);
    loop {
        let Some(total) = used.checked_add(bytes).filter(|total| *total <= max) else {
            return false;
        };
        match counter.compare_exchange_weak(used, total, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return true,
            Err(actual) => used = actual,
        }
    }
}

impl StreamBudget {
    /// Creates a fresh budget enforcing `limits`.
    #[must_use]
    pub fn new(limits: ParseLimits) -> Self {
        Self {
            limits,
            used: Arc::default(),
        }
    }

    /// Returns the limits this budget enforces.
    #[must_use]
    pub const fn limits(&self) -> &ParseLimits {
        &self.limits
    }

    fn charge_alias(&self, bytes: usize) -> Result<(), LimitKind> {
        let max = self.limits.max_alias_bytes;
        charge(&self.used.alias_bytes, bytes, max.get())
            .then_some(())
            .ok_or(LimitKind::AliasBytes(max))
    }

    fn charge_tag_prefix(&self, bytes: usize) -> Result<(), LimitKind> {
        let max = self.limits.max_tag_bytes;
        charge(&self.used.tag_prefix_bytes, bytes, max.get())
            .then_some(())
            .ok_or(LimitKind::TagBytes(max))
    }
}

/// Identifies which limit was exceeded, carrying its configured value.
#[derive(Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    /// Collections are nested deeper than the limit.
    #[error("nesting depth exceeds {0}")]
    Depth(MaxDepth),
    /// Alias expansion would produce more data than the budget.
    #[error("alias expansion exceeds {0} bytes")]
    AliasBytes(MaxAliasBytes),
    /// Anchored collections that hold other anchors are stored once per level and exceed the budget.
    #[error(
        "copies of anchored collections that hold other anchors exceed {0} bytes and {ANCHOR_COPY_FACTOR} times the source size"
    )]
    AnchorCopies(MaxAliasBytes),
    /// Tag prefix expansion would materialize more data than the budget.
    #[error("tag prefix expansion exceeds {0} bytes")]
    TagBytes(MaxTagBytes),
    /// The YAML being dumped is, or would be, larger than the limit.
    #[error("output size exceeds {0} bytes")]
    OutputBytes(MaxOutputBytes),
    /// The value being dumped expands to more nodes than the limit.
    #[error("dump node count exceeds {0}")]
    DumpNodes(MaxDumpNodes),
}

/// Estimated fixed cost of one expanded node, in bytes, charged on top of scalar and tag text.
pub const NODE_BYTES: usize = 64;

/// How many times the source size of a document the copies of its anchored collections may reach
/// before [`LimitKind::AnchorCopies`] applies (the copies must also pass the alias budget).
pub const ANCHOR_COPY_FACTOR: usize = 24;

/// Estimated memory of one cloned value node, before its scalar text.
const COPY_NODE_BYTES: usize = std::mem::size_of::<crate::value::Value>();

/// Source position of `span`'s end in characters, which never exceeds its byte offset.
#[allow(clippy::disallowed_methods)]
fn source_end(span: Span) -> usize {
    span.end.index()
}

/// Length of a resolved tag prefix, in bytes, that is not charged against [`MaxTagBytes`].
pub const TAG_PREFIX_ALLOWANCE: usize = 64;

/// Size of a completed subtree: expanded byte weight and collection height.
#[derive(Debug, Clone, Copy, Default)]
struct Subtree {
    bytes: usize,
    /// Memory a clone of the subtree occupies, estimated from the real size of a value.
    copy: usize,
    height: usize,
    /// Whether an anchor is defined inside, so every anchored ancestor stores its own copy.
    anchored: bool,
}

fn tag_bytes(tag: Option<&Tag>) -> usize {
    tag.map_or(0, |t| t.handle.len().saturating_add(t.suffix.len()))
}

#[derive(Debug)]
struct Frame {
    anchor: usize,
    tag_bytes: usize,
    /// Accumulated children: `bytes` is their sum, `height` their maximum.
    children: Subtree,
}

/// Streaming enforcer of [`ParseLimits`] over parser events.
///
/// Feed every event, in order, to [`observe`](Self::observe) before handing it to a
/// loader. It tracks open collections and the expanded size of every anchored node, so
/// an alias is rejected before the loader clones it.
///
/// Bindings reach it through [`EventStream`](crate::events::EventStream).
#[derive(Debug)]
pub(crate) struct LimitGuard {
    budget: StreamBudget,
    stack: Vec<Frame>,
    completed: HashMap<usize, Subtree>,
    max_anchor_seen: usize,
    /// Source position where the current document starts, in characters.
    doc_start: usize,
    /// Estimated memory of the copies of anchored collections holding anchors in the document.
    anchor_copies: usize,
    /// Whether the loader clones anchored nodes, so nested anchors multiply memory.
    clones_anchors: bool,
    // Anchors below this id belong to earlier documents and are out of scope.
    doc_anchor_floor: usize,
    cursor: DocumentCursor,
}

/// Tracks which document of a stream the next event belongs to.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DocumentCursor {
    started: usize,
    open: bool,
}

impl DocumentCursor {
    pub(crate) const fn observe(&mut self, event: &Event<'_>) {
        match event {
            Event::DocumentStart(_) => {
                self.started += 1;
                self.open = true;
            }
            Event::DocumentEnd => self.open = false,
            _ => {}
        }
    }

    pub(crate) const fn index(self) -> usize {
        if self.open {
            self.started - 1
        } else {
            self.started
        }
    }
}

impl LimitGuard {
    /// Creates a guard enforcing `limits` from the start of a stream.
    #[must_use]
    pub fn new(limits: ParseLimits) -> Self {
        Self::with_budget(StreamBudget::new(limits))
    }

    /// Creates a guard drawing alias and tag-prefix bytes from `budget`, which may be shared with other guards.
    #[must_use]
    pub fn with_budget(budget: StreamBudget) -> Self {
        Self {
            budget,
            stack: Vec::new(),
            completed: HashMap::new(),
            max_anchor_seen: 0,
            doc_start: 0,
            anchor_copies: 0,
            clones_anchors: true,
            doc_anchor_floor: 0,
            cursor: DocumentCursor::default(),
        }
    }

    /// Zero-based index of the document the next event belongs to.
    ///
    /// Between two documents this is the index of the document that follows.
    #[must_use]
    pub const fn document(&self) -> usize {
        self.cursor.index()
    }

    /// Declares that the loader shares anchored nodes instead of copying them, which lifts the
    /// [`LimitKind::AnchorCopies`] check that only bounds copies.
    #[must_use]
    pub const fn sharing_anchors(mut self) -> Self {
        self.clones_anchors = false;
        self
    }

    /// Accounts for one parser event.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::LimitExceeded`] when the event breaks a limit, and
    /// [`ParseError::Syntax`] for an alias that refers to an anchor from a previous
    /// document (anchors do not cross document boundaries).
    pub fn observe(&mut self, event: &Event<'_>, span: Span) -> ParseResult<()> {
        self.cursor.observe(event);
        match event {
            Event::DocumentStart(_) => {
                self.completed.clear();
                self.doc_anchor_floor = self.max_anchor_seen + 1;
                self.doc_start = source_end(span);
                self.anchor_copies = 0;
            }
            Event::SequenceStart(anchor, tag) | Event::MappingStart(anchor, tag) => {
                self.charge_tag_prefix(tag.as_deref(), span)?;
                if self.stack.len() >= self.budget.limits.max_depth.get() {
                    return Err(self.exceeded(LimitKind::Depth(self.budget.limits.max_depth), span));
                }
                self.max_anchor_seen = self.max_anchor_seen.max(*anchor);
                self.stack.push(Frame {
                    anchor: *anchor,
                    tag_bytes: tag_bytes(tag.as_deref()),
                    children: Subtree::default(),
                });
            }
            Event::Scalar(text, _, anchor, tag) => {
                self.charge_tag_prefix(tag.as_deref(), span)?;
                self.max_anchor_seen = self.max_anchor_seen.max(*anchor);
                let bytes = NODE_BYTES
                    .saturating_add(text.len())
                    .saturating_add(tag_bytes(tag.as_deref()));
                self.complete(
                    *anchor,
                    Subtree {
                        bytes,
                        copy: COPY_NODE_BYTES.saturating_add(text.len()),
                        height: 0,
                        anchored: false,
                    },
                    span,
                )?;
            }
            Event::SequenceEnd | Event::MappingEnd => {
                if let Some(frame) = self.stack.pop() {
                    self.complete(
                        frame.anchor,
                        Subtree {
                            bytes: frame
                                .children
                                .bytes
                                .saturating_add(NODE_BYTES)
                                .saturating_add(frame.tag_bytes),
                            copy: frame.children.copy.saturating_add(COPY_NODE_BYTES),
                            height: frame.children.height + 1,
                            anchored: frame.children.anchored,
                        },
                        span,
                    )?;
                }
            }
            Event::Alias(id) => self.observe_alias(*id, span)?,
            _ => {}
        }
        Ok(())
    }

    fn charge_tag_prefix(&self, tag: Option<&Tag>, span: Span) -> ParseResult<()> {
        let Some(tag) = tag else { return Ok(()) };
        self.budget
            .charge_tag_prefix(tag.handle.len().saturating_sub(TAG_PREFIX_ALLOWANCE))
            .map_err(|kind| self.exceeded(kind, span))
    }

    fn observe_alias(&mut self, id: usize, span: Span) -> ParseResult<()> {
        if id < self.doc_anchor_floor {
            return Err(ParseError::scanner(
                &ScanError::new_str(span.start, "while parsing node, found unknown anchor"),
                self.document(),
            ));
        }
        // Absent means the anchor's collection is still open (`&a [*a]`).
        let subtree = self.completed.get(&id).copied().ok_or_else(|| {
            ParseError::Syntax(SyntaxError::recursive_alias(
                SourcePosition::from_span(span),
                self.document(),
            ))
        })?;
        self.budget
            .charge_alias(subtree.bytes)
            .map_err(|kind| self.exceeded(kind, span))?;
        if self.stack.len().saturating_add(subtree.height) > self.budget.limits.max_depth.get() {
            return Err(self.exceeded(LimitKind::Depth(self.budget.limits.max_depth), span));
        }
        self.complete(
            0,
            Subtree {
                anchored: false,
                ..subtree
            },
            span,
        )
    }

    /// Records a finished node; an anchored subtree that holds anchors is stored once per level,
    /// which is bounded by the alias budget together with a multiple of the source size. The check
    /// runs on the subtree's closing event, before any loader clones it.
    fn complete(&mut self, anchor: usize, subtree: Subtree, span: Span) -> ParseResult<()> {
        if anchor > 0 {
            if subtree.anchored && self.clones_anchors {
                self.anchor_copies = self.anchor_copies.saturating_add(subtree.copy);
                let limit = self.budget.limits.max_alias_bytes;
                let source = source_end(span).saturating_sub(self.doc_start);
                if self.anchor_copies > limit.get()
                    && self.anchor_copies / ANCHOR_COPY_FACTOR > source
                {
                    return Err(self.exceeded(LimitKind::AnchorCopies(limit), span));
                }
            }
            self.completed.insert(anchor, subtree);
        }
        if let Some(parent) = self.stack.last_mut() {
            parent.children.bytes = parent.children.bytes.saturating_add(subtree.bytes);
            parent.children.copy = parent.children.copy.saturating_add(subtree.copy);
            parent.children.height = parent.children.height.max(subtree.height);
            parent.children.anchored |= subtree.anchored || anchor > 0;
        }
        Ok(())
    }

    fn exceeded(&self, kind: LimitKind, span: Span) -> ParseError {
        let SourcePosition { line, column } = SourcePosition::from_span(span);
        ParseError::LimitExceeded {
            kind,
            line,
            column,
            document: self.document(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saphyr_parser::{Marker, ScalarStyle};
    use std::borrow::Cow;

    #[test]
    fn bounded_debug_names_the_limit() {
        assert_eq!(format!("{:?}", MaxDepth::DEFAULT), "MaxDepth(256)");
        assert_eq!(format!("{:?}", MaxDocuments::MIN), "MaxDocuments(1)");
    }

    #[test]
    fn bounded_rejects_out_of_range_values() {
        assert_eq!(
            MaxDocuments::new(0),
            Err(LimitRangeError {
                value: 0,
                min: 1,
                max: 10_000_000
            })
        );
        assert!(MaxDocuments::new(10_000_001).is_err());
        assert_eq!(MaxDocuments::new(10_000_000), Ok(MaxDocuments::MAX));
    }

    #[test]
    fn check_file_len_does_not_truncate() {
        let max = MaxInputBytes::new(8).unwrap();
        assert_eq!(max.check_file_len(9).unwrap_err().size, 9);
        assert_eq!(max.check_file_len(1 << 40).unwrap_err().size, 1 << 40);
    }

    fn span() -> Span {
        Span::empty(Marker::new(0, 1, 0))
    }

    #[test]
    fn max_depth_new_enforces_range() {
        assert_eq!(MaxDepth::new(1), Ok(MaxDepth::MIN));
        assert_eq!(MaxDepth::new(MaxDepth::MAX.get()), Ok(MaxDepth::MAX));
        for value in [0, MaxDepth::MAX.get() + 1] {
            assert_eq!(
                MaxDepth::new(value),
                Err(LimitRangeError {
                    value,
                    min: 1,
                    max: MaxDepth::MAX.get()
                })
            );
        }
    }

    #[test]
    fn max_input_bytes_new_enforces_range() {
        assert_eq!(MaxInputBytes::new(1), Ok(MaxInputBytes::MIN));
        assert_eq!(
            MaxInputBytes::new(MaxInputBytes::MAX.get()),
            Ok(MaxInputBytes::MAX)
        );
        for value in [0, MaxInputBytes::MAX.get() + 1] {
            assert_eq!(
                MaxInputBytes::new(value),
                Err(LimitRangeError {
                    value,
                    min: 1,
                    max: MaxInputBytes::MAX.get()
                })
            );
        }
        assert_eq!(
            MaxInputBytes::new(MaxInputBytes::DEFAULT.get()),
            Ok(MaxInputBytes::DEFAULT)
        );
    }

    #[test]
    fn max_input_bytes_check_boundary() {
        let max = MaxInputBytes::new(8).unwrap();
        assert!(max.check(0).is_ok());
        assert!(max.check(8).is_ok());
        assert_eq!(
            max.check(9),
            Err(InputTooLarge {
                size: 9,
                limit: max
            })
        );
    }

    #[test]
    fn max_alias_bytes_new_enforces_range() {
        assert_eq!(MaxAliasBytes::MAX.get(), 1 << 30);
        assert_eq!(MaxAliasBytes::new(1), Ok(MaxAliasBytes::MIN));
        assert_eq!(
            MaxAliasBytes::new(MaxAliasBytes::MAX.get()),
            Ok(MaxAliasBytes::MAX)
        );
        for value in [0, MaxAliasBytes::MAX.get() + 1] {
            assert!(MaxAliasBytes::new(value).is_err());
        }
    }

    #[test]
    fn defaults_lie_within_range() {
        assert_eq!(
            MaxDepth::new(MaxDepth::DEFAULT.get()),
            Ok(MaxDepth::DEFAULT)
        );
        assert_eq!(
            MaxAliasBytes::new(MaxAliasBytes::DEFAULT.get()),
            Ok(MaxAliasBytes::DEFAULT)
        );
    }

    #[test]
    fn dump_budget_byte_boundary_is_exact() {
        let max = MaxOutputBytes::new(2 * MIN_NODE_OUTPUT_BYTES + 3);
        let mut budget = DumpBudget::new(max, MaxDumpNodes::DEFAULT);
        assert!(budget.charge_nodes(2).is_ok());
        assert!(budget.charge(3).is_ok());
        assert_eq!(budget.charge(1), Err(LimitKind::OutputBytes(max)));
    }

    #[test]
    fn dump_budget_node_boundary_is_exact() {
        let max_nodes = MaxDumpNodes::new(5);
        let mut budget = DumpBudget::new(MaxOutputBytes::DEFAULT, max_nodes);
        assert!(budget.charge_nodes(3).is_ok());
        assert!(budget.charge_nodes(2).is_ok());
        assert_eq!(budget.charge_nodes(1), Err(LimitKind::DumpNodes(max_nodes)));
    }

    #[test]
    fn dump_budget_saturates_on_huge_counts() {
        let mut budget = DumpBudget::default();
        assert!(budget.charge_nodes(usize::MAX).is_err());
        assert!(budget.charge_nodes(usize::MAX).is_err());
        assert!(budget.charge(usize::MAX).is_err());
    }

    #[test]
    fn dump_budget_accepts_default_node_cap_exactly() {
        let mut budget = DumpBudget::default();
        assert!(budget.charge_nodes(MaxDumpNodes::DEFAULT.get()).is_ok());
        assert!(budget.charge_nodes(1).is_err());
    }

    #[test]
    fn charge_alias_never_wraps() {
        let budget = StreamBudget::new(ParseLimits {
            max_alias_bytes: MaxAliasBytes::UNBOUNDED,
            ..ParseLimits::default()
        });
        assert!(budget.charge_alias(usize::MAX).is_ok());
        assert!(budget.charge_alias(1).is_err());
        assert!(budget.charge_alias(usize::MAX).is_err());
    }

    #[test]
    fn tag_budget_is_shared_between_clones() {
        let budget = StreamBudget::new(ParseLimits {
            max_tag_bytes: MaxTagBytes::new(10),
            ..ParseLimits::default()
        });
        let other = budget.clone();
        assert!(budget.charge_tag_prefix(6).is_ok());
        assert!(other.charge_tag_prefix(6).is_err());
        assert!(other.charge_tag_prefix(4).is_ok());
    }

    #[test]
    fn doubling_alias_chain_errors_instead_of_wrapping() {
        let limits = ParseLimits {
            max_alias_bytes: MaxAliasBytes::UNBOUNDED,
            max_depth: MaxDepth::MAX,
            ..ParseLimits::default()
        };
        let mut guard = LimitGuard::new(limits);
        let scalar = Event::Scalar(Cow::Borrowed("x"), ScalarStyle::Plain, 1, None);
        guard.observe(&Event::DocumentStart(false), span()).unwrap();
        guard.observe(&scalar, span()).unwrap();
        let mut outcome = Ok(());
        for id in 2..200 {
            guard
                .observe(&Event::SequenceStart(id, None), span())
                .unwrap();
            outcome = guard
                .observe(&Event::Alias(id - 1), span())
                .and_then(|()| guard.observe(&Event::Alias(id - 1), span()));
            if outcome.is_err() {
                break;
            }
            guard.observe(&Event::SequenceEnd, span()).unwrap();
        }
        assert!(matches!(
            outcome,
            Err(ParseError::LimitExceeded {
                kind: LimitKind::AliasBytes(_),
                ..
            })
        ));
    }

    fn tagged_scalar(prefix_len: usize) -> Event<'static> {
        let tag = Tag {
            handle: "p".repeat(prefix_len),
            suffix: "x".to_owned(),
        };
        Event::Scalar(
            Cow::Borrowed("v"),
            ScalarStyle::Plain,
            0,
            Some(Cow::Owned(tag)),
        )
    }

    fn tag_limits(bytes: usize) -> ParseLimits {
        ParseLimits {
            max_tag_bytes: MaxTagBytes::new(bytes),
            ..ParseLimits::default()
        }
    }

    #[test]
    fn prefix_within_allowance_is_free() {
        let mut guard = LimitGuard::new(tag_limits(0));
        for _ in 0..1_000 {
            guard
                .observe(&tagged_scalar(TAG_PREFIX_ALLOWANCE), span())
                .unwrap();
        }
    }

    #[test]
    fn prefix_charge_exceeding_budget_is_rejected() {
        let mut guard = LimitGuard::new(tag_limits(20));
        let event = tagged_scalar(TAG_PREFIX_ALLOWANCE + 10);
        guard.observe(&event, span()).unwrap();
        guard.observe(&event, span()).unwrap();
        let err = guard.observe(&event, span()).unwrap_err();
        assert!(matches!(
            err,
            ParseError::LimitExceeded {
                kind: LimitKind::TagBytes(_),
                ..
            }
        ));
    }

    #[test]
    fn tag_budget_persists_across_documents() {
        let mut guard = LimitGuard::new(tag_limits(10));
        let event = tagged_scalar(TAG_PREFIX_ALLOWANCE + 10);
        guard.observe(&Event::DocumentStart(false), span()).unwrap();
        guard.observe(&event, span()).unwrap();
        guard.observe(&Event::DocumentStart(false), span()).unwrap();
        assert!(guard.observe(&event, span()).is_err());
    }

    #[test]
    fn collection_tags_are_charged() {
        let mut guard = LimitGuard::new(tag_limits(0));
        let tag = Tag {
            handle: "p".repeat(TAG_PREFIX_ALLOWANCE + 1),
            suffix: String::new(),
        };
        let event = Event::MappingStart(0, Some(Cow::Owned(tag)));
        assert!(guard.observe(&event, span()).is_err());
    }

    #[test]
    fn unlimited_tag_budget_never_rejects() {
        let mut guard = LimitGuard::new(tag_limits(usize::MAX));
        let event = tagged_scalar(TAG_PREFIX_ALLOWANCE + 1_000);
        for _ in 0..1_000 {
            guard.observe(&event, span()).unwrap();
        }
    }
}
