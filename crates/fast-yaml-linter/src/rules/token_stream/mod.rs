//! Rebuilds the token stream of `PyYAML`'s scanner from the node index and the source text.
//!
//! Rules that port a token-based yamllint rule (`indentation`, `hyphens`) read the same tokens
//! yamllint sees, which the parser's events do not carry.

pub mod scanner;
pub mod tokens;
