# Design Principles

This page is a review record. It lists the engineering principles of two earlier projects by the same maintainer and
records, for each one that is relevant here, whether Noise Oxydation **carries** it as written, **adapts** it,
**rejects** it, or has **not adopted** it yet, where it applies, and why. It exists so that the maintainer can confirm
or change each decision. The binding rules themselves live in [AGENTS.md](../AGENTS.md) and
[ARCHITECTURE.md](../ARCHITECTURE.md); this page does not restate them as new rules.

| Source | Guidance read |
| --- | --- |
| `bitwig-remote-project` (preferred: more recent) | `AGENTS.md`, `docs/engineering-style.md`, `client/agent-guides/architecture.md`, `client/agent-guides/rust.md`, `client/agent-guides/tooling.md` |
| `midi-bpm-detector` | `rust/agent-guides/architecture.md`, `rust/agent-guides/tooling.md`, `rust/architecture.md` |

Below, *Bitwig* and *MIDI* abbreviate the two projects.

## Summary

| Principle | Decision | Where it applies |
| --- | --- | --- |
| [Crates split by dependency surface, grouped by theme](#crate-grouping) | Adapted | `crates/core`, `crates/tools`, `crates/bindings`; ARCHITECTURE.md crate map |
| [Dependencies point down only](#dependency-direction) | Carried | Tools and the C ABI crate depend on the library; checked by the crate-map drift check |
| [Ownership clarity and exact dependencies](#ownership-and-exact-dependencies) | Carried | `CallEnhancer` as composition root; stage and renderer signatures |
| [Naming by role and lifecycle](#naming) | Carried | Module and type names; current-design wording in docs |
| [Types over sentinels, no false reuse](#types-over-sentinels) | Carried | `PacketOutcome`, `CallPhase`, configuration enums, renderer types |
| [Name-only wrappers](#name-only-wrappers) | Carried | All crates |
| [No speculative abstraction](#no-speculative-abstraction) | Carried | Static dispatch over the implemented estimators |
| [Error taxonomy and provenance at boundaries](#errors) | Adapted | Flat typed error enums with `source()`; structured C ABI detail and typed Go errors at the FFI boundary |
| [Production surface versus tests](#production-surface-versus-tests) | Carried | AGENTS.md hard rule |
| [Test layout](#test-layout) | Carried | `tests/unit/<module>.rs`, `tests/integration/`; Go tests beside the package |
| [No tautological tests](#tautological-tests) | Carried | AGENTS.md hard rule |
| [Shared state and synchronization](#shared-state-and-synchronization) | Carried | No shared state today; coherent snapshots if ever needed |
| [Ring buffers and atomics](#ring-buffers-and-atomics) | Adapted | Ring buffer for output only; no atomics or locks |
| [Realtime constraints](#realtime-constraints) | Carried | `process_packet`, `drain`, `reset`, and their C ABI and Go wrappers |
| [Logging and performance analysis](#logging-and-performance-analysis) | Adapted | `log` feature; `stage-timing` feature |
| [Lint and format policy](#lint-and-format-policy) | Carried, exceptions adapted | `rust-toolchain.toml`, `rustfmt.toml`, workspace lints |
| [No macros](#no-macros) | Carried | All crates |
| [Import qualification](#import-qualification) | Not adopted | Open for decision |
| [Dependency evaluation, no utils crates](#dependencies) | Carried | Workspace manifests |
| [Integrity and hashing](#integrity-and-hashing) | Carried | Audio fetch script; freshness check |
| [Documentation rules](#documentation) | Carried, hub structure rejected | README.md, ARCHITECTURE.md, docs/ |
| [App-specific rules](#rejected-app-specific-rules) | Rejected | Not applicable |

## Structure

### Crate grouping

*Bitwig* `client/agent-guides/architecture.md` splits crates "primarily by dependency surface, then refine by
responsibility". *MIDI* groups crates in thematic folders (`crates/entrypoints/`, `crates/bpm/`, `crates/support/`,
`crates/tools/`, `crates/foundation/`).

**Adapted.** Crates are split by dependency surface and grouped by role: `crates/core/noise-oxydation` (the library,
`std` plus an optional `log`), `crates/tools/noise-oxydation-eval` (adds `hound` and a CLI) and
`crates/tools/how-it-works` (the guide renderer, `std` only) and `crates/bindings/noise-oxydation-capi` (the C ABI
static library for Go, the only crate whose dependency surface is a foreign ABI and `unsafe` code). There are no `support` or `foundation` groups because no shared infrastructure
crate exists. The library stays one crate: every DSP stage has the same dependency surface and the same lifecycle
owner (the call), so per-algorithm crates would only add boundaries (see
[ARCHITECTURE.md](../ARCHITECTURE.md#crate-map-and-dependency-direction)).

### Dependency direction

*MIDI* `rust/agent-guides/architecture.md`: product crates depend down into foundation crates, never back up.

**Carried.** Tools and the C ABI crate depend on the library; the library depends on no tool, binding, WAV, CLI or
rendering crate. The Go package in `go/` sits above the C ABI crate and links it. The renderer's crate-map drift check
fails generation when the manifests gain an edge the diagram does not show, or when the Go package's module path or
link flags stop matching the diagram. `cargo tree -p noise-oxydation -e normal` lists only `log`, and
`cargo tree -p noise-oxydation-capi -e normal` only the library, without `log`.

### Ownership and exact dependencies

*Bitwig* `docs/engineering-style.md` (Core Principles, Boundaries): ownership clarity over brevity; functions receive
the state they need, not a larger object; stable producers and consumers are wired visibly at a composition root.
*MIDI* adds that bootstrap should read as the static runtime graph.

**Carried.** `CallEnhancer` is the composition root of one call and owns every stage as a field; stages receive exactly
what they use (for example the analyzer receives the window, the FFT and the output spectrum). The renderer receives
one complete palette rather than reading theme colors itself. There is no event bus or registry: the only runtime
graph is the fixed stage order of one call.

### Naming

*Bitwig* `docs/engineering-style.md` (Naming): name values by role, lifecycle, source or destination; avoid generic
`manager`, `state`, `shared`; no historical or planned-sibling qualifiers in landed code and current docs.

**Carried.** Modules and types name the stage or role (`noise_estimator`, `OutputQueue`, `CallPhase`), and the docs
describe the current design. This page and the attribution of the Go reference are the deliberate exceptions to "no
origin stories": one is a review record, the other a license obligation.

### Types over sentinels

*Bitwig* `client/agent-guides/rust.md` and *MIDI* `rust/agent-guides/tooling.md`: encode unset, error and special
states in types; avoid false reuse that forces `unreachable!` or "cannot happen" comments.

**Carried.** Packet results are `PacketOutcome::{Priming, Emitted}`, the call phase is `CallPhase`, the estimator and
interference choices are enums, and the tonal detector is an `Option`. In the renderer a connector label is one
`Option<ConnectorLabel>` holding text and position, so a label without a position cannot be written, and emphasis,
stroke and anchor are enums rather than strings.

### Name-only wrappers

*Bitwig* `docs/engineering-style.md` (Name-Only Wrappers): a function that only paraphrases one native expression is
not an abstraction.

**Carried.** Short functions own an invariant, conversion or policy (for example `convert::narrow` owns the audited
lossy conversion); plain constants replace accessor functions.

### No speculative abstraction

*Bitwig* `docs/engineering-style.md` and *MIDI* `rust/agent-guides/tooling.md`: add variants, traits and policy layers
when production code needs them, not for imagined futures.

**Carried.** The three noise estimators and the interference setting exist because the reference documents them; they
are enum variants dispatched statically, with no trait objects. The renderer has no layout engine: each diagram is
authored.

## Errors

*Bitwig* `docs/engineering-style.md` (Error Taxonomy And Recovery) and `AGENTS.md` (Provider Boundary Evidence): errors
follow operation boundaries; a lifecycle hierarchy only when stable and operationally important; the provider that
recognizes a condition returns it as a typed value and keeps the foreign source.

**Adapted.** Each crate owns flat, typed error enums for its small closed sets: `ConfigError` and `StreamError` in the
library, `EvalError` in the evaluation crate, and `ValidationError`, `ManifestError` and `GenerateError` in the
renderer. No hierarchy is needed yet: no error category forces a state transition beyond `StreamError::Drained`, which
the library enforces itself. Consumers map errors at their boundary and keep the cause through
`std::error::Error::source`, with the file path attached to I/O and WAV failures. The library rejects invalid
configuration instead of replacing it with defaults, so the provider, not the caller, guarantees a valid call.

At the FFI boundary provenance is carried as data, because a Rust error value cannot cross it. The C ABI crate maps
each `ConfigError` variant to a status plus a `nox_error` detail in caller-owned storage that holds the variant's kind
and every datum (fields, values, constraint and bounds, count bounds, duration), without allocation. The Go package
maps that detail to a `*ConfigError` with the same data, which unwraps to the category sentinel `ErrInvalidConfig`, and
its message is formatted by the Rust `Display` implementation through `nox_error_message`, so the text has one source.
Lifecycle statuses map to Go sentinels, and a Rust panic becomes a status instead of unwinding into Go.

## Tests

### Production surface versus tests

*Bitwig* `docs/engineering-style.md` (Production Surface And Tests): no production API, flag or behavior that exists
only for tests.

**Carried** as an AGENTS.md hard rule. The allocation-counting allocators live in the `allocation` test binary and the
evaluation binary, where `bench` needs them.

### Test layout

*Bitwig* `client/agent-guides/rust.md` and *MIDI* `rust/agent-guides/tooling.md`: no inline `mod tests { ... }`; unit
tests in `tests/unit/<module>.rs` wired with a `#[path]` hook; integration tests under `tests/integration/`.

**Carried** in all four crates, including the renderer and the C ABI crate. The Go package keeps its tests beside the
code in `go/`, as Go requires.

### Tautological tests

*Bitwig* `AGENTS.md`: tests exercise behavior, failure handling, wiring, regressions or independently owned contracts,
never restated constants.

**Carried** as an AGENTS.md hard rule. The library tests compare with closed-form math and reference-documented values;
the renderer tests exercise each validator rejection, determinism, the palette-only difference between themes, the
freshness check and manifest drift.

## Shared State, Synchronization And Realtime

### Shared state and synchronization

*Bitwig* `docs/engineering-style.md` (Shared State And Synchronization) and *MIDI* `rust/agent-guides/architecture.md`
(Communication Patterns): identify which fields form one logical state before choosing atomics or locks; values observed
together share one synchronization boundary; separate atomics only for independently meaningful values; choose the
simplest mechanism that keeps the invariants.

**Carried** as the rule for any future cross-thread feature, recorded in
[ARCHITECTURE.md](../ARCHITECTURE.md#per-call-ownership): values that must be observed together are published as one
coherent snapshot. Today there is nothing to synchronize.

### Ring buffers and atomics

*MIDI* `rust/architecture.md` (Realtime Constraints): the plugin callback pushes events into a fixed ring buffer with
`try_push`, and state crossing the callback boundary uses atomics, fixed buffers or non-blocking handoff. The maintainer
also stated a preference for ring buffers and non-blocking synchronization where they cause no inconsistency.

**Adapted, decided explicitly:**

- **Output: a ring buffer.** `OutputQueue` is a fixed 512-byte circular FIFO with head and length indexes. Finished
  μ-law bytes enter at the tail as frames complete and leave one 160-byte packet at a time; nothing is shifted or
  allocated.
- **Analysis input and synthesis overlap: linear arrays, not ring buffers.** After each frame the analyzer shifts its
  256-sample buffer by 128 samples with `copy_within`, and the synthesizer shifts its overlap and weight buffers the
  same way. The [stage breakdown](performance.md#stage-breakdown) measures the whole analysis stage (window, FFT,
  power spectrum and the shift) at 1.57 µs per packet and the synthesis stage (inverse FFT, overlap-add and the
  shifts) at 1.57 µs, against 16.80 µs for all stages together (16.89 µs measured around `process_packet`). A packet completes 1.25 frames on average, so each stage
  costs about 1.26 µs per frame, of which the FFT alone is about 1.2 µs: the shifts and the element-wise loops share
  the few hundredths of a microsecond that remain. A ring buffer would not remove the copy either: the FFT needs the
  frame contiguous and windowed, so a circular analysis buffer would still be gathered into scratch, and it would add
  wrap-around indexing to every access. With no measurable benefit, the simpler structure stays.
- **No atomics, no locks.** The library has no cross-thread boundary. Each call's state belongs to one
  `CallEnhancer`, which is `Send` and mutated through `&mut self`, so independent calls run in parallel without any
  synchronization at all: that is non-blocking by construction. Stage timings are per-call `Copy` snapshots that a tool
  adds up after its threads join, and the evaluation binary's allocation counter keeps per-thread counters rather than
  shared atomics. Adding atomics here would add a protocol without an invariant to protect.

Revisit this decision if a profile shows the shifts matter (for example with a much faster FFT), or when a feature
introduces a real cross-thread boundary such as publishing statistics to a monitoring thread.

### Realtime constraints

*MIDI* `rust/agent-guides/architecture.md` (Realtime Constraints): keep constraints explicit in code and docs; no
blocking locks, allocation or blocking reads on audio-critical paths; fixed-capacity buffers are intentional; document
which side owns a thread. *Bitwig* `client/agent-guides/architecture.md`: realtime peers stay async-free.

**Carried.** `process_packet`, `drain` and `reset` never allocate, lock, block, log or perform I/O; storage is sized at
construction; the allocation integration test and `noise-oxydation-eval bench` check it. The packet path is synchronous
and runs on the caller's thread, which ARCHITECTURE.md states; callers may drive calls from async tasks, but the
library never awaits. The same holds through the Go package: its packet methods pass the caller's fixed-size arrays to
the C ABI, which writes into them, and `testing.AllocsPerRun` checks zero Go allocations, also under the race
detector.

### Logging and performance analysis

*MIDI* `rust/architecture.md` centralizes error reporting, logging and tracing helpers in a `support/errors` crate.

**Adapted.** There is no support crate: the library emits one debug record through the optional `log` facade, only
from construction, and nothing on the packet path. Performance analysis is the opt-in `stage-timing` feature, which
accumulates per-call stage timings without allocation; `minitrace` was evaluated and not adopted
([performance.md](performance.md#minitrace-decision)).

## Tooling

### Lint and format policy

*Bitwig* `client/agent-guides/rust.md` and *MIDI* `rust/agent-guides/tooling.md`: pinned stable toolchain; formatting
with nightly rustfmt; `clippy::pedantic` at workspace level; warnings are work to fix; no `#[allow]` or tool-level lint
exceptions without explicit human confirmation.

**Carried**, with two adaptations. The toolchain is pinned in `rust-toolchain.toml` (1.98.1), and the nightly rustfmt is
pinned to a date (`nightly-2026-09-25`) so that the unstable `rustfmt.toml` options format identically in CI and
locally. The only lint exceptions are narrow `#[expect(..., reason = "...")]` attributes confined to the audited
numeric-conversion modules and listed in [lint-exceptions.md](lint-exceptions.md). The C ABI crate adds stricter
lints instead: it denies `unsafe_op_in_unsafe_fn` and `clippy::undocumented_unsafe_blocks`, so every `unsafe` block
carries its safety argument. Every Cargo command runs with `--locked`; the Go package is checked with `gofmt` and
`go vet`.

### No macros

*Bitwig* `AGENTS.md`, `client/agent-guides/rust.md` and *MIDI* `rust/agent-guides/tooling.md`: introduce no macros
without agreement; prefer direct syntax over macro calls where it is clearer.

**Carried.** The workspace defines no `macro_rules!` or procedural macros; it uses derives and ordinary standard macros
such as `format!` and `assert!`.

### Import qualification

*Bitwig* `client/agent-guides/rust.md`: when seven or more items come from one module, import the module and qualify
the names at their use sites.

**Not adopted.** Several modules import seven or more items from one module: the library's public re-exports in
`lib.rs`, integration-test support imports, the evaluation replay, and the renderer's diagram seeds and validator.
Adopting the rule would mostly change the seeds to `model::Card { .. }` style. Open for your decision.

### Dependencies

*Bitwig* `client/agent-guides/tooling.md` and *MIDI* `rust/agent-guides/tooling.md`: evaluate existing crates before
writing generic utility code and document an awkward fit; no generic `utils` crates; keep dependency versions moving
forward.

**Carried.** `hound` handles WAV files. `rustfft` and `realfft` were measured and rejected because they change the
output bits ([performance.md](performance.md#vectorization-investigation)). The renderer depends on no SVG or TOML crate
by requirement: it writes SVG directly, and its manifest reader parses only the two dependency forms the workspace
uses and rejects any other. There is no `utils` crate, and the locked versions are current.

### Integrity and hashing

*Bitwig* `AGENTS.md` (Local Integrity And Assurance): hash against independently authoritative bytes; a hash stored
beside data by the same authority is provenance, not protection; no hardening without a named failure.

**Carried.** `scripts/fetch-evaluation-audio.sh` verifies each download against SHA-256 values taken from the pinned
sources. The guide's freshness check compares exact generated content instead of storing checksums.

## Documentation

*Bitwig* `AGENTS.md` and `agent-guides/documentation.md`: current public docs explain end-to-end flow, ownership and
failure behavior; describe the current design directly; one canonical home per fact; keep agent instructions concise
and current, fixing stale instructions rather than adding compatibility; keep planning artifacts out of the public tree;
route through a task-oriented `docs/README.md` index with hubs and leaves.

**Carried, except the hub structure.** ARCHITECTURE.md owns contracts, docs/algorithms.md equations,
docs/reference-log.md differences from the reference, and docs/evaluation.md and docs/performance.md the evidence;
README.md and HOW_IT_WORKS.md summarize and link. Work-tracking state and downloaded audio stay out of Git.
**Rejected:** the index-hub-leaf structure. README.md links all eleven documents directly from one table, so an index
would add a layer without a routing problem to solve. Revisit if the docs grow beyond a directly linkable set.

## Rejected App-Specific Rules

| Rule | Source | Why it does not apply |
| --- | --- | --- |
| Tauri shell, IPC events and channels, TypeScript and web rules | *Bitwig* `client/agent-guides/architecture.md`, `tooling.md` | No desktop app or user interface |
| Protocol ownership, request correlation, stream dispatchers, messaging patterns | *Bitwig* `docs/engineering-style.md`, `client/agent-guides/architecture.md` | No wire protocol: the packet API is an in-process call, also from Go through the C ABI |
| Bitwig host projections, domain handles, private API binding workflow | *Bitwig* `AGENTS.md`, `docs/engineering-style.md` | No Bitwig host |
| Plugin, desktop and WASM modes, the nice-plug fork policy, the GUI phase contract | *MIDI* `rust/architecture.md`, `rust/agent-guides/architecture.md` | No plugin host or GUI; the realtime analogue, the 20 ms packet cadence, is covered under [Realtime constraints](#realtime-constraints) |
| A `scripts/dev.sh` command helper | *Bitwig* `client/agent-guides/tooling.md`; *MIDI* `rust/agent-guides/tooling.md` | The Cargo commands are few and listed verbatim in AGENTS.md and README.md; a wrapper would be a second entry point to keep current |
| The task-oriented docs index with hubs and leaves | *Bitwig* `agent-guides/documentation.md` | See [Documentation](#documentation) |
| Kotlin and Java rules | *Bitwig* `AGENTS.md` | No JVM code |
| Repository-work and user-update wording rules | *Bitwig* `AGENTS.md` | Agent workflow conventions, not design principles of this code |
