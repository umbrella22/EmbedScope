//! Caller-allocated C integration for EmbedScope. See `include/escope.h`.
//!
//! All calls on one context must be serialized. Callback reentry returns Busy.
//! The C caller supplies valid allocations; null, alignment, capacity, numeric
//! status and UTF-8 errors are checked at this boundary.
#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

mod backend;
mod context;
mod types;

pub use context::*;
pub use types::*;
