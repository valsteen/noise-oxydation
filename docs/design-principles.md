# Design Principles

This page lists the engineering principles Noise Oxydation follows, where each one applies, and why. The binding rules
live in [AGENTS.md](../AGENTS.md) and [ARCHITECTURE.md](../ARCHITECTURE.md); this page explains them and links to them
rather than restating them as new rules.

## Summary

| Principle | Where it applies |
| --- | --- |
| [Crates split by dependency surface, grouped by role](#crate-grouping) | `crates/core`, `crates/tools`, `crates/bindings`; ARCHITECTURE.md crate map |
| [Dependencies point down only](#dependency-direction) | Tools and the C ABI crate depend on the library; checked by the crate-map drift check |
| [Ownership clarity and exact dependencies](#ownership-and-exact-dependencies) | `CallEnhancer` as composition root; stage and renderer signatures |
| [Naming by role and lifecycle](#naming) | Module and type names; current-design wording in docs |
| [Types over sentinels, no false reuse](#types-over-sentinels) | `PacketOutcome`, `CallPhase`, configuration enums, renderer types |
| [No name-only wrappers](#no-name-only-wrappers) | All crates |
| [No speculative abstraction](#no-speculative-abstraction) | Static dispatch over the implemented estimators |
| [Typed errors with provenance at boundaries](#errors) | Flat typed error enums with `source()`; structured C ABI detail and typed Go errors at the FFI boundary |
| [No test-only production surface](#no-test-only-production-surface) | AGENTS.md hard rule |
| [Test layout](#test-layout) | `tests/unit/<module>.rs`, `tests/integration/`; Go tests beside the package |
| [No tautological tests](#no-tautological-tests) | AGENTS.md hard rule |
| [Coherent shared state](#shared-state-and-synchronization) | No shared state today; coherent snapshots if ever needed |
| [Ring buffers and atomics only where they pay](#ring-buffers-and-atomics) | Ring buffer for output only; no atomics or locks |
| [Realtime constraints](#realtime-constraints) | `process_packet`, `drain`, `reset`, and their C ABI and Go wrappers |
| [Cheap logging, opt-in performance analysis](#logging-and-performance-analysis) | `log` feature; `stage-timing` feature |
| [Pinned toolchain, strict lints](#lint-and-format-policy) | `rust-toolchain.toml`, `rustfmt.toml`, workspace lints |
| [No macros](#no-macros) | All crates |
| [Evaluated dependencies, no utils crates](#dependencies) | Workspace manifests |
| [Integrity against authoritative bytes](#integrity-and-hashing) | Audio fetch script; freshness check |
| [One canonical home per fact](#documentation) | README.md, ARCHITECTURE.md, docs/ |

## Structure

### Crate grouping

Crates are split by dependency surface first, then grouped by role in folders: `crates/core/noise-oxydation` (the
library, `std` plus an optional `log`), `crates/tools/noise-oxydation-eval` (adds `hound` and a CLI),
`crates/tools/how-it-works` (the guide renderer, `std` only) and `crates/bindings/noise-oxydation-capi` (the C ABI
static library for Go, the only crate whose dependency surface is a foreign ABI and `unsafe` code). There is no shared
infrastructure crate because nothing needs one. The library stays one crate: every DSP stage has the same dependency
surface and the same lifecycle owner (the call), so per-algorithm crates would only add boundaries (see
[ARCHITECTURE.md](../ARCHITECTURE.md#crate-map-and-dependency-direction)).

### Dependency direction

Dependencies point down only. Tools and the C ABI crate depend on the library; the library depends on no tool,
binding, WAV, CLI or rendering crate. The Go package in `go/` sits above the C ABI crate and links it. The renderer's
crate-map drift check fails generation when the manifests gain an edge the diagram does not show, or when the Go
package's module path or link flags stop matching the diagram. `cargo tree -p noise-oxydation -e normal` lists only
`log`, and `cargo tree -p noise-oxydation-capi -e normal` only the library, without `log`.

### Ownership and exact dependencies

Ownership is clear over brief, functions receive the state they need rather than a larger object, and the runtime
graph is wired visibly at a composition root. `CallEnhancer` is the composition root of one call and owns every stage
as a field; stages receive exactly what they use (for example the analyzer receives the window, the FFT and the output
spectrum). The renderer receives one complete palette rather than reading theme colors itself. There is no event bus
or registry: the only runtime graph is the fixed stage order of one call.

### Naming

Values are named by role, lifecycle, source or destination, not with generic words such as `manager` or `state`, and
code and docs describe the current design without historical or planned-sibling qualifiers. Modules and types name the
stage or role (`noise_estimator`, `OutputQueue`, `CallPhase`). The attribution of the Go reference is the deliberate
exception to "no origin stories", because it is a license obligation.

### Types over sentinels

Unset, error and special states are encoded in types, and no type is reused where it would force `unreachable!` or a
"cannot happen" comment. Packet results are `PacketOutcome::{Priming, Emitted}`, the call phase is `CallPhase`, the
estimator and interference choices are enums, and the tonal detector is an `Option`. In the renderer a connector label
is one `Option<ConnectorLabel>` holding text and position, so a label without a position cannot be written, and
emphasis, stroke and anchor are enums rather than strings.

### No name-only wrappers

A function that only paraphrases one native expression is not an abstraction. Short functions own an invariant,
conversion or policy (for example `convert::narrow` owns the audited lossy conversion); plain constants replace
accessor functions.

### No speculative abstraction

Variants, traits and policy layers are added when production code needs them, not for imagined futures. The three
noise estimators and the interference setting exist because the reference documents them; they are enum variants
dispatched statically, with no trait objects. The renderer has no layout engine: each diagram is authored.

## Errors

Errors follow operation boundaries, and the code that recognizes a condition returns it as a typed value and keeps the
underlying source. Each crate owns flat, typed error enums for its small closed sets: `ConfigError` and `StreamError`
in the library, `EvalError` in the evaluation crate, and `ValidationError`, `ManifestError` and `GenerateError` in the
renderer. No hierarchy is needed: no error category forces a state transition beyond `StreamError::Drained`, which the
library enforces itself. Consumers map errors at their boundary and keep the cause through
`std::error::Error::source`, with the file path attached to I/O and WAV failures. The library rejects invalid
configuration instead of replacing it with defaults, so the provider, not the caller, guarantees a valid call.

At the FFI boundary provenance is carried as data, because a Rust error value cannot cross it. The C ABI crate maps
each `ConfigError` variant to a status plus a `nox_error` detail in caller-owned storage that holds the variant's kind
and every datum (fields, values, constraint and bounds, count bounds, duration), without allocation. The Go package
maps that detail to a `*ConfigError` with the same data, which unwraps to the category sentinel `ErrInvalidConfig`, and
its message is formatted by the Rust `Display` implementation through `nox_error_message`, so the text has one source.
Lifecycle statuses map to Go sentinels, and a Rust panic becomes a status instead of unwinding into Go.

## Tests

### No test-only production surface

No production API, flag or behavior exists only for tests (an AGENTS.md hard rule). The allocation-counting allocators
live in the `allocation` test binary and the evaluation binary, where `bench` needs them.

### Test layout

Production files contain no inline `mod tests { ... }`: unit tests live in `tests/unit/<module>.rs`, wired with a
`#[path]` hook, and integration tests under `tests/integration/`, in all four crates. The Go package keeps its tests
beside the code in `go/`, as Go requires.

### No tautological tests

Tests exercise behavior, failure handling, wiring, regressions or independently owned contracts, never restated
constants (an AGENTS.md hard rule). The library tests compare with closed-form math and reference-documented values;
the renderer tests exercise each validator rejection, determinism, the palette-only difference between themes, the
freshness check and manifest drift.

## Shared State, Synchronization And Realtime

### Shared state and synchronization

Before choosing atomics or locks, identify which fields form one logical state: values observed together share one
synchronization boundary, separate atomics are only for independently meaningful values, and the simplest mechanism
that keeps the invariants wins. [ARCHITECTURE.md](../ARCHITECTURE.md#per-call-ownership) records this as the rule for
any future cross-thread feature: values that must be observed together are published as one coherent snapshot. Today
there is nothing to synchronize.

### Ring buffers and atomics

Ring buffers and non-blocking synchronization are preferred where they cause no inconsistency and pay for themselves:

- **Output: a ring buffer.** `OutputQueue` is a fixed 512-byte circular FIFO with head and length indexes. Finished
  μ-law bytes enter at the tail as frames complete and leave one 160-byte packet at a time; nothing is shifted or
  allocated.
- **Analysis input and synthesis overlap: linear arrays, not ring buffers.** After each frame the analyzer shifts its
  256-sample buffer by 128 samples with `copy_within`, and the synthesizer shifts its overlap and weight buffers the
  same way. The [stage breakdown](performance.md#stage-breakdown) measures the whole analysis stage (window, FFT,
  power spectrum and the shift) at 1.57 µs per packet and the synthesis stage (inverse FFT, overlap-add and the
  shifts) at 1.57 µs, against 16.80 µs for all stages together (16.89 µs measured around `process_packet`). A packet
  completes 1.25 frames on average, so each stage costs about 1.26 µs per frame, of which the FFT alone is about
  1.2 µs: the shifts and the element-wise loops share the few hundredths of a microsecond that remain. A ring buffer
  would not remove the copy either: the FFT needs the frame contiguous and windowed, so a circular analysis buffer
  would still be gathered into scratch, and it would add wrap-around indexing to every access. With no measurable
  benefit, the simpler structure stays.
- **No atomics, no locks.** The library has no cross-thread boundary. Each call's state belongs to one
  `CallEnhancer`, which is `Send` and mutated through `&mut self`, so independent calls run in parallel without any
  synchronization at all: that is non-blocking by construction. Stage timings are per-call `Copy` snapshots that a tool
  adds up after its threads join, and the evaluation binary's allocation counter keeps per-thread counters rather than
  shared atomics. Adding atomics here would add a protocol without an invariant to protect.

Revisit this if a profile shows the shifts matter (for example with a much faster FFT), or when a feature introduces a
real cross-thread boundary such as publishing statistics to a monitoring thread.

### Realtime constraints

Realtime constraints are explicit in code and docs: no blocking locks, allocation or blocking calls on audio-critical
paths, fixed-capacity buffers by design, and a documented owner for each thread. `process_packet`, `drain` and `reset`
never allocate, lock, block, log or perform I/O; storage is sized at construction; the allocation integration test and
`noise-oxydation-eval bench` check it. The packet path is synchronous and runs on the caller's thread, which
ARCHITECTURE.md states; callers may drive calls from async tasks, but the library never awaits. The same holds through
the Go package: its packet methods pass the caller's fixed-size arrays to the C ABI, which writes into them, and
`testing.AllocsPerRun` checks zero Go allocations, also under the race detector.

### Logging and performance analysis

Logging is cheap and can be compiled out, and performance analysis is opt-in. The library emits one debug record
through the optional `log` facade, only from construction, and nothing on the packet path. Performance analysis is the
opt-in `stage-timing` feature, which accumulates per-call stage timings without allocation; `minitrace` was evaluated
and not adopted ([performance.md](performance.md#minitrace-decision)).

## Tooling

### Lint and format policy

The stable toolchain is pinned in `rust-toolchain.toml` (1.98.1), and formatting uses a nightly rustfmt pinned to a date
(`nightly-2026-09-25`) so that the unstable `rustfmt.toml` options format identically in CI and locally.
`clippy::pedantic` applies at workspace level and warnings are work to fix. The only lint exceptions are narrow
`#[expect(..., reason = "...")]` attributes confined to the audited numeric-conversion modules and listed in
[lint-exceptions.md](lint-exceptions.md). The C ABI crate adds stricter lints instead: it denies
`unsafe_op_in_unsafe_fn` and `clippy::undocumented_unsafe_blocks`, so every `unsafe` block carries its safety argument.
Every Cargo command runs with `--locked`; the Go package is checked with `gofmt` and `go vet`.

### No macros

No macros are introduced without agreement, and direct syntax is preferred where it is clearer. The workspace defines
no `macro_rules!` or procedural macros; it uses derives and ordinary standard macros such as `format!` and `assert!`.

### Dependencies

Existing crates are evaluated before writing generic utility code, an awkward fit is documented, there are no generic
`utils` crates, and dependency versions keep moving forward. `hound` handles WAV files. `rustfft` and `realfft` were
measured and rejected because they change the output bits
([performance.md](performance.md#vectorization-investigation)). The renderer depends on no SVG or TOML crate by
requirement: it writes SVG directly, and its manifest reader parses only the two dependency forms the workspace uses
and rejects any other. The locked versions are current.

### Integrity and hashing

Hashes are checked against independently authoritative bytes; a hash stored beside data by the same authority is
provenance, not protection; and there is no hardening without a named failure. `scripts/fetch-evaluation-audio.sh`
verifies each download against SHA-256 values taken from the pinned sources. The guide's freshness check compares exact
generated content instead of storing checksums.

## Documentation

Public docs explain the end-to-end flow, ownership and failure behavior, describe the current design directly, and give
every fact one canonical home; agent instructions stay concise and current, and planning artifacts stay out of the
public tree. ARCHITECTURE.md owns contracts, docs/algorithms.md equations, docs/reference-log.md differences from the
reference, and docs/evaluation.md and docs/performance.md the evidence; README.md and HOW_IT_WORKS.md summarize and
link. README.md links every document directly from one table. Work-tracking state and downloaded audio stay out of Git.
