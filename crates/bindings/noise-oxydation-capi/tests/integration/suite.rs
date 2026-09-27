//! Integration tests of the exported C functions, called through the crate's `rlib`.
#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]

mod config_errors;
mod equality;
mod lifecycle;
mod support;
