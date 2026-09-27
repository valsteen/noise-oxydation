# AGENTS.md

Instructions for AI coding agents working in this repository.

## Read First

- [ARCHITECTURE.md](ARCHITECTURE.md) owns the crate map, dependency direction, per-call ownership, the buffer and
  synchronization decision, the packet timing contract, calibration, and the audio-critical constraints. Keep it
  current in the same change as the code.
- [docs/reference-log.md](docs/reference-log.md) owns every difference from, or concern about, the Go behavior
  reference. Add an entry whenever you rely on, disagree with, or diverge from reference behavior. Never claim numerical
  parity without a measured entry.
- [docs/algorithms.md](docs/algorithms.md) owns the implemented equations, defaults, and units. Update it with any
  algorithm or default change.
- [docs/lint-exceptions.md](docs/lint-exceptions.md) lists every lint expectation.
- [docs/evaluation.md](docs/evaluation.md) and [docs/performance.md](docs/performance.md) own the measured quality and
  performance evidence. Rerun the affected evidence and update them when processing, timing or performance-relevant
  code changes.
- [docs/go-integration.md](docs/go-integration.md) owns how Go programs consume the library: the boundary choice, the
  `noiseox` API, errors, lifecycle and concurrency, the macOS and Linux build and link steps, and the call-stream
  example. Keep it current with `go/` and the C ABI crate.
- [docs/how-it-works.md](docs/how-it-works.md) owns the diagram grammar and the procedure for changing
  [HOW_IT_WORKS.md](HOW_IT_WORKS.md), which `crates/tools/how-it-works` generates.
- [docs/design-principles.md](docs/design-principles.md) lists the engineering principles this project follows and
  where each applies. Update it when a principle is adopted, changed or dropped.

## Layout

Workspace members are `crates/*/*`, grouped by role: `crates/core/noise-oxydation` is the library,
`crates/tools/noise-oxydation-eval` and `crates/tools/how-it-works` are development tools that depend on it (or on
nothing), and `crates/bindings/noise-oxydation-capi` is the C ABI static library for Go. Two Go modules live outside
the workspace: `go/` (the importable package `noiseox`, which links the C ABI crate, with its header
`go/noise_oxydation.h` and the example `go/examples/callstream`) and `tools/go-parity`. A new crate needs its own
dependency, lifecycle or reuse boundary (see ARCHITECTURE.md), a group folder, and an update of the crate-map seed in
`crates/tools/how-it-works/src/diagrams/crate_map.rs`; the renderer rejects a seed that differs from the manifests.

## Commands

Run from the repository root. Builds are locked: always pass `--locked`.

```bash
cargo +nightly-2026-09-25 fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo build --locked --workspace --release
```

CI also runs `cargo test --locked -p noise-oxydation --no-default-features` to keep the crate building without the
`log` feature, and `cargo run --locked -p how-it-works -- --check` to keep the generated guide current.
`--all-features` covers the `stage-timing` feature, the evaluation crate, the renderer and the C ABI crate.

The Go package links the static library, which must be built for its own package (a workspace build would unify
features and link `log` into it). CI runs these on Linux and macOS, plus a separate consumer module:

```bash
cargo build --locked --release -p noise-oxydation-capi
(cd go && gofmt -l . && go vet ./... && go test -race ./...)    # gofmt -l must print nothing
(cd go && go run ./examples/callstream -synthetic 3000)
```

Changing the C ABI means changing `go/noise_oxydation.h` in the same change; the crate's tests compare its constants
and layout assertions with the Rust definitions.

The visitor guide and its diagrams are generated:

```bash
cargo run --locked -p how-it-works               # regenerate HOW_IT_WORKS.md and docs/assets/how-it-works/*.svg
cargo run --locked -p how-it-works -- --check    # fail if a committed output is stale
```

Evidence runs download audio or need Go, so they are manual and never run in CI:

```bash
scripts/fetch-evaluation-audio.sh                                          # pinned sources into audio/sources/
cargo run --locked --release -p noise-oxydation-eval -- replay             # WAV files and metrics into audio/out/
cargo run --locked --release -p noise-oxydation-eval -- bench              # add --features stage-timing for stages
cargo run --locked --release -p noise-oxydation-eval -- compare A.ul B.ul  # byte and sample statistics
(cd go && go run ./examples/callstream -in ../audio/out/office-5db/noisy.ul -passes 20 -parallel 100)  # Go binding
(cd tools/go-parity && go run . enhance -in IN.ul -out OUT.ul)             # the Go reference, see evaluation.md
```

The stable toolchain is pinned in `rust-toolchain.toml`. Formatting intentionally uses the pinned nightly rustfmt
because `rustfmt.toml` uses unstable options.

## Hard Rules

- Keep `process_packet`, `drain`, and `reset` free of allocation, locks, blocking calls, I/O, and logging. Size storage
  at construction. The allocation-counting integration test must keep passing.
- The library crate, the evaluation crate's library and the guide renderer forbid `unsafe` code. `unsafe` is permitted
  in exactly three places: the two allocation-counting `GlobalAlloc` implementations that delegate to
  `std::alloc::System` and count per thread (the library's `allocation` test harness and the `noise-oxydation-eval`
  binary, `src/main.rs`), and the C ABI crate `crates/bindings/noise-oxydation-capi`, including its tests. Every
  `unsafe` block there carries a `// SAFETY:` comment, which the crate enforces with
  `#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]`. Do not add another site, keep the library
  free of `extern "C"` code, and keep explicit SIMD, if ever adopted, in safe abstractions.
- The C ABI and the Go package keep the packet path free of allocation: pass caller-owned fixed-size arrays, never
  convert packets to new slices or strings, and keep the `TestHotPathAllocatesNothing` check passing under `-race`.
- Treat compiler and Clippy (pedantic) warnings as work to fix. Do not add `#[allow]`/`#[expect]` or tool-level lint
  exceptions except a narrow `#[expect(..., reason = "...")]` in a crate's audited numeric-conversion module
  (`src/convert.rs` of the library or the evaluation crate), recorded in `docs/lint-exceptions.md`. If another
  exception looks necessary, stop and explain the tradeoff.
- Do not introduce macros (`macro_rules!` or procedural) without explicit human agreement.
- Do not add inline `mod tests { ... }` bodies to production files. Unit tests live in `tests/unit/<module>.rs` of the
  owning crate, wired with:

  ```rust
  #[cfg(test)]
  #[path = "../tests/unit/<module>.rs"]
  mod tests;
  ```

  The library's integration tests live under `tests/integration/`: `suite.rs` (the `integration` target) groups the
  packet lifecycle, calibration, reset, parallel-call, estimator and interference alternative, interference, and
  stage-timing tests, and `allocation.rs` (the `allocation` target) is the only test binary with the counting global
  allocator. Register a new integration target in the crate's `Cargo.toml`.
- Do not add tautological tests that restate constants or the production algorithm. Test observable behavior, failure
  handling, lifecycle, regressions, and independently derived expectations (closed-form math, reference-documented
  values).
- Production code must not expose behavior that exists only for tests.
- Do not copy Go reference code. Derive behavior from the pinned reference documentation and source, and log
  differences in `docs/reference-log.md`.
- Never edit `HOW_IT_WORKS.md` or `docs/assets/how-it-works/*.svg` by hand. Change the seeds or guide text in
  `crates/tools/how-it-works`, regenerate, render every changed diagram in both themes at about 880 px and look at it
  (for example with `rsvg-convert -w 880`), and commit seeds and outputs together. Update the guide text when a fact it
  quotes changes in ARCHITECTURE.md or the evidence documents.
- Documentation adapted from the Go reference's README carries an attribution line and is covered by
  [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md); Rust facts and `docs/reference-log.md` override the original where
  they differ.
- Keep downloaded audio and rendered audio out of Git: the fetch script and the replay write only under the ignored
  `/audio/` directory, and the DEMAND license forbids redistributing derived mixes. Evidence runs stay out of CI.
- Prefer type-safe representations over sentinel values, and typed error enums over strings. Map errors at crate
  boundaries while preserving their source.
