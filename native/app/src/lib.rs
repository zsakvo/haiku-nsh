//! Small platform and theme entry points; application screens live in TSX.
use nsh_core::prelude::*;
mod core;
mod platform;
pub use core::{Core, CoreBridge};

/// The native API imported by the TSX entry.
#[native_module("haiku/native")]
fn native(registry: &mut Registry) {
    registry.class::<Core>();
}
