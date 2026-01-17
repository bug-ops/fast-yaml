//! Trait abstractions for streaming formatter backends.
//!
//! This module defines the trait hierarchy that abstracts over different
//! memory allocation strategies (standard heap vs arena allocation).
//! All traits are module-private since they're implementation details.

use super::Context;

/// Operations on context stack abstraction.
///
/// Provides Vec-like operations for tracking YAML structure context
/// (Root, Sequence, `MappingKey`, `MappingValue`) during streaming.
pub(super) trait ContextStackOps {
    /// Pushes a new context onto the stack.
    fn push(&mut self, ctx: Context);

    /// Pops the top context from the stack.
    fn pop(&mut self) -> Option<Context>;

    /// Returns a reference to the last (top) context.
    fn last(&self) -> Option<&Context>;

    /// Returns a mutable reference to the last (top) context.
    fn last_mut(&mut self) -> Option<&mut Context>;

    /// Returns the number of contexts in the stack.
    fn len(&self) -> usize;

    /// Returns whether the stack is empty (should never happen in practice).
    #[inline]
    #[allow(dead_code)] // Keep for trait completeness
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Operations on anchor storage abstraction.
///
/// Provides storage and retrieval of anchor names for anchor/alias resolution.
/// Each anchor ID maps to a generated name like "anchor1", "anchor2", etc.
pub(super) trait AnchorStoreOps {
    /// Ensures storage can hold the given `anchor_id` (grows if needed).
    ///
    /// After calling this method, `get(anchor_id)` and `set_if_empty(anchor_id)`
    /// are guaranteed not to panic due to out-of-bounds access.
    fn ensure_capacity(&mut self, anchor_id: usize);

    /// Gets anchor name if it exists and is non-empty.
    ///
    /// Returns `None` if `anchor_id` is out of bounds or the slot is empty.
    fn get(&self, anchor_id: usize) -> Option<&str>;

    /// Sets anchor name to "anchorN" if slot is empty, returns reference to the name.
    ///
    /// This method generates and stores the anchor name if not already present,
    /// then returns a reference to it. This enables zero-allocation reuse.
    ///
    /// # Panics
    ///
    /// Panics if `anchor_id` is out of bounds. Call `ensure_capacity` first.
    fn set_if_empty(&mut self, anchor_id: usize) -> &str;

    /// Checks if the `anchor_id` slot is empty.
    ///
    /// Returns `true` if the slot doesn't exist or contains an empty string.
    #[allow(dead_code)] // Keep for trait completeness
    fn is_empty(&self, anchor_id: usize) -> bool {
        self.get(anchor_id).is_none_or(str::is_empty)
    }
}

/// Formatter backend abstraction combining context stack and anchor storage.
///
/// This trait defines the memory allocation strategy for temporary structures
/// used during YAML formatting. Implementations provide:
/// - `StdBackend`: Standard heap allocation (Vec<T>)
/// - `ArenaBackend`: Arena allocation (bumpalo collections)
///
/// The generic formatter `StreamingFormatter<B: FormatterBackend>` uses this
/// trait to abstract over allocation strategies while maintaining zero runtime
/// cost through monomorphization.
pub(super) trait FormatterBackend {
    /// Context stack type (Vec-like)
    type ContextStack: ContextStackOps;

    /// Anchor storage type (Vec-like)
    type AnchorStore: AnchorStoreOps;

    /// Returns an immutable reference to the context stack.
    fn context_stack(&self) -> &Self::ContextStack;

    /// Returns a mutable reference to the context stack.
    fn context_stack_mut(&mut self) -> &mut Self::ContextStack;

    /// Returns an immutable reference to the anchor store.
    fn anchor_store(&self) -> &Self::AnchorStore;

    /// Returns a mutable reference to the anchor store.
    fn anchor_store_mut(&mut self) -> &mut Self::AnchorStore;
}
