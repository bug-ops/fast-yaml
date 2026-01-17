//! Arena allocation backend for streaming formatter.
//!
//! This module provides the arena-based backend implementation using bumpalo
//! for temporary allocations (context stack and anchor storage). The arena
//! is created and destroyed within the public API function, ensuring all
//! temporary allocations are freed at once.

use std::fmt::Write as FmtWrite;

use bumpalo::Bump;
use saphyr_parser::Parser;

use super::Context;
use super::formatter::StreamingFormatter;
use super::traits::{AnchorStoreOps, ContextStackOps, FormatterBackend};
use crate::emitter::EmitterConfig;
use crate::error::{EmitError, EmitResult};

/// Arena allocation backend.
///
/// Uses `bumpalo::collections::Vec<'bump, Context>` for context stack and
/// `bumpalo::collections::Vec<'bump, String<'bump>>` for anchor storage.
///
/// This backend requires the `arena` feature flag.
pub(super) struct ArenaBackend<'bump> {
    context_stack: bumpalo::collections::Vec<'bump, Context>,
    anchor_names: bumpalo::collections::Vec<'bump, bumpalo::collections::String<'bump>>,
    #[allow(dead_code)] // Keep arena reference for potential future use
    arena: &'bump Bump,
}

impl<'bump> ArenaBackend<'bump> {
    /// Creates a new arena backend with pre-allocated capacity.
    ///
    /// # Arguments
    ///
    /// * `context_capacity` - Initial capacity for context stack (typical: 16)
    /// * `arena` - Arena allocator for temporary allocations
    pub fn new(context_capacity: usize, arena: &'bump Bump) -> Self {
        let mut context_stack =
            bumpalo::collections::Vec::with_capacity_in(context_capacity, arena);
        context_stack.push(Context::Root);

        Self {
            context_stack,
            anchor_names: bumpalo::collections::Vec::new_in(arena),
            arena,
        }
    }
}

// Implementation of ContextStackOps for bumpalo::collections::Vec<'bump, Context>
#[allow(clippy::use_self)] // bumpalo Vec method names match trait names
impl ContextStackOps for bumpalo::collections::Vec<'_, Context> {
    #[inline]
    fn push(&mut self, ctx: Context) {
        bumpalo::collections::Vec::push(self, ctx);
    }

    #[inline]
    fn pop(&mut self) -> Option<Context> {
        bumpalo::collections::Vec::pop(self)
    }

    #[inline]
    fn last(&self) -> Option<&Context> {
        <[Context]>::last(self)
    }

    #[inline]
    fn last_mut(&mut self) -> Option<&mut Context> {
        <[Context]>::last_mut(self)
    }

    #[inline]
    fn len(&self) -> usize {
        bumpalo::collections::Vec::len(self)
    }
}

// Implementation of AnchorStoreOps for bumpalo::collections::Vec<'bump, String<'bump>>
#[allow(clippy::use_self)] // bumpalo Vec/slice method names used
impl<'bump> AnchorStoreOps
    for bumpalo::collections::Vec<'bump, bumpalo::collections::String<'bump>>
{
    fn ensure_capacity(&mut self, anchor_id: usize) {
        // bumpalo::collections::Vec lacks resize_with, use while loop
        while self.len() <= anchor_id {
            self.push(bumpalo::collections::String::new_in(self.bump()));
        }
    }

    fn get(&self, anchor_id: usize) -> Option<&str> {
        <[bumpalo::collections::String]>::get(self, anchor_id)
            .filter(|s| !s.is_empty())
            .map(bumpalo::collections::String::as_str)
    }

    fn set_if_empty(&mut self, anchor_id: usize) -> &str {
        if self[anchor_id].is_empty() {
            // Generate name directly into storage (single allocation)
            let _ = write!(self[anchor_id], "anchor{anchor_id}");
        }
        self[anchor_id].as_str()
    }
}

// Implementation of FormatterBackend for ArenaBackend
impl<'bump> FormatterBackend for ArenaBackend<'bump> {
    type ContextStack = bumpalo::collections::Vec<'bump, Context>;
    type AnchorStore = bumpalo::collections::Vec<'bump, bumpalo::collections::String<'bump>>;

    #[inline]
    fn context_stack(&self) -> &Self::ContextStack {
        &self.context_stack
    }

    #[inline]
    fn context_stack_mut(&mut self) -> &mut Self::ContextStack {
        &mut self.context_stack
    }

    #[inline]
    fn anchor_store(&self) -> &Self::AnchorStore {
        &self.anchor_names
    }

    #[inline]
    fn anchor_store_mut(&mut self) -> &mut Self::AnchorStore {
        &mut self.anchor_names
    }
}

/// Format YAML using streaming parser events with arena allocation.
///
/// This function uses bumpalo arena allocation for temporary structures,
/// reducing heap allocation overhead. The arena is created and destroyed
/// within this function, ensuring all temporary allocations are freed at once.
///
/// # Performance
///
/// Arena allocation provides 5-14% speedup over standard allocation
/// (measured across document sizes from 100 to 50000 lines) by:
/// - Eliminating individual deallocation overhead
/// - Reducing allocator lock contention
/// - Improving memory locality
///
/// Note: Performance gains diminish for larger documents as parser overhead
/// dominates allocation costs.
///
/// # Errors
///
/// Returns `EmitError::Emit` if the parser encounters invalid YAML.
///
/// # Examples
///
/// ```
/// # #[cfg(all(feature = "streaming", feature = "arena"))]
/// # {
/// use fast_yaml_core::streaming::format_streaming_arena;
/// use fast_yaml_core::EmitterConfig;
///
/// let yaml = "key: value\nlist:\n  - item1\n  - item2\n";
/// let config = EmitterConfig::default();
/// let formatted = format_streaming_arena(yaml, &config).unwrap();
/// assert!(formatted.contains("key:"));
/// # }
/// ```
pub fn format_streaming_arena(input: &str, config: &EmitterConfig) -> EmitResult<String> {
    // Create arena sized for typical YAML overhead
    // 4KB minimum handles most documents; larger inputs get proportional arenas
    let arena_size = (input.len() / 4).max(4096);
    let arena = Bump::with_capacity(arena_size);

    let parser = Parser::new_from_str(input);

    // Output is typically 10-20% larger than input due to formatting
    let output_capacity = input.len() + (input.len() / 5);

    // Pre-allocate context stack in arena (16 levels handles 99% of cases)
    let context_capacity = 16;

    let backend = ArenaBackend::new(context_capacity, &arena);
    let mut formatter = StreamingFormatter::new(config, output_capacity, backend);

    for result in parser {
        let (event, span) = result.map_err(|e| EmitError::Emit(e.to_string()))?;
        formatter.format_event(event, span);
    }

    Ok(formatter.finish())
    // Arena dropped here - all temporary allocations freed at once
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_backend_creation() {
        let arena = Bump::new();
        let backend = ArenaBackend::new(16, &arena);
        assert_eq!(backend.context_stack.len(), 1);
        assert_eq!(backend.context_stack[0], Context::Root);
        assert_eq!(backend.anchor_names.len(), 0);
    }

    #[test]
    fn test_arena_context_stack_ops() {
        let arena = Bump::new();
        let mut stack = bumpalo::collections::Vec::with_capacity_in(16, &arena);
        stack.push(Context::Root);

        // Test push
        stack.push(Context::Sequence);
        assert_eq!(stack.len(), 2);

        // Test last
        assert_eq!(stack.last(), Some(&Context::Sequence));

        // Test last_mut
        if let Some(last) = stack.last_mut() {
            *last = Context::MappingKey;
        }
        assert_eq!(stack.last(), Some(&Context::MappingKey));

        // Test pop
        assert_eq!(stack.pop(), Some(Context::MappingKey));
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn test_arena_anchor_store_ops() {
        let arena = Bump::new();
        let mut store: bumpalo::collections::Vec<'_, bumpalo::collections::String<'_>> =
            bumpalo::collections::Vec::new_in(&arena);

        // Test ensure_capacity
        store.ensure_capacity(5);
        assert!(store.len() >= 6);

        // Test set_if_empty
        let name = store.set_if_empty(3);
        assert_eq!(name, "anchor3");

        // Test get
        assert_eq!(store.get(3), Some("anchor3"));
        assert_eq!(store.get(2), None); // Empty string
        assert_eq!(store.get(100), None); // Out of bounds

        // Test is_empty (from trait)
        assert!(AnchorStoreOps::is_empty(&store, 2));
        assert!(!AnchorStoreOps::is_empty(&store, 3));
        assert!(AnchorStoreOps::is_empty(&store, 100));
    }

    #[test]
    fn test_arena_set_if_empty_idempotent() {
        let arena = Bump::new();
        let mut store: bumpalo::collections::Vec<'_, bumpalo::collections::String<'_>> =
            bumpalo::collections::Vec::new_in(&arena);
        store.ensure_capacity(5);

        // First call generates name
        let name1 = store.set_if_empty(2).to_string();
        assert_eq!(name1, "anchor2");

        // Second call returns same name
        let name2 = store.set_if_empty(2);
        assert_eq!(name2, "anchor2");
        assert_eq!(name1, name2);
    }

    #[test]
    fn test_arena_formatter_backend_accessors() {
        let arena = Bump::new();
        let mut backend = ArenaBackend::new(16, &arena);

        // Test context_stack accessors
        assert_eq!(backend.context_stack().len(), 1);
        backend.context_stack_mut().push(Context::Sequence);
        assert_eq!(backend.context_stack().len(), 2);

        // Test anchor_store accessors
        assert_eq!(backend.anchor_store().len(), 0);
        backend.anchor_store_mut().ensure_capacity(3);
        assert!(backend.anchor_store().len() >= 4);
    }

    #[test]
    fn test_format_streaming_arena_simple() {
        let yaml = "key: value";
        let config = EmitterConfig::default();
        let result = format_streaming_arena(yaml, &config).unwrap();
        assert!(result.contains("key:"));
        assert!(result.contains("value"));
    }
}
