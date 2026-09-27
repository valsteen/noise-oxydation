//! Offline evaluation workflows for `noise-oxydation`: real-speech replay with objective metrics, byte-level
//! comparison of two μ-law outputs, and packet-path benchmarks.
//!
//! The library holds every workflow so that it stays free of `unsafe` code. The `noise-oxydation-eval` binary adds only
//! the allocation-counting global allocator that the `bench` command reads through [`HeapCounter`]. Everything here
//! is an evidence tool: it reads and writes files, prints plain reports to stdout, and is never linked into the
//! library it measures.
#![forbid(unsafe_code)]

mod audio_io;
mod bench;
mod cli;
mod compare;
mod convert;
mod error;
mod heap;
mod json;
mod metrics;
mod mulaw;
mod replay;
mod resample;
mod scenario;
mod spectrum;

pub use crate::{
    cli::run,
    error::EvalError,
    heap::{HeapActivity, HeapCounter},
};
