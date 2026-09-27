# AGENTS.md

Instructions for AI coding agents working in this repository.

## Read First

- [ARCHITECTURE.md](ARCHITECTURE.md) owns the crate map, dependency direction, per-call ownership, the packet timing
  contract, calibration, and the audio-critical constraints. Keep it current in the same change as the code.
- [docs/reference-log.md](docs/reference-log.md) owns every difference from, or concern about, the Go behavior
  reference. Add an entry whenever you rely on, disagree with, or diverge from reference behavior. Never claim numerical
  parity without a measured entry.
- [docs/algorithms.md](docs/algorithms.md) owns the implemented equations, defaults, and units. Update it with any
  algorithm or default change.
- [docs/lint-exceptions.md](docs/lint-exceptions.md) lists every lint expectation.
- [docs/evaluation.md](docs/evaluation.md) and [docs/performance.md](docs/performance.md) own the measured quality and
  performance evidence. Rerun the affected evidence and update them when processing, timing or performance-relevant
  code changes.

## Commands

Run from the repository root. Builds are locked: always pass `--locked`.

```bash
cargo +nightly-2026-09-25 fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo build --locked --workspace --release
```

CI also runs `cargo test --locked -p noise-oxydation --no-default-features` to keep the crate building without the
`log` feature. `--all-features` covers the `stage-timing` feature and the evaluation crate.

Evidence runs download audio or need Go, so they are manual and never run in CI:

```bash
scripts/fetch-evaluation-audio.sh                                          # pinned sources into audio/sources/
cargo run --locked --release -p noise-oxydation-eval -- replay             # WAV files and metrics into audio/out/
cargo run --locked --release -p noise-oxydation-eval -- bench              # add --features stage-timing for stages
cargo run --locked --release -p noise-oxydation-eval -- compare A.ul B.ul  # byte and sample statistics
(cd tools/go-parity && go run . enhance -in IN.ul -out OUT.ul)             # the Go reference, see evaluation.md
```

The stable toolchain is pinned in `rust-toolchain.toml`. Formatting intentionally uses the pinned nightly rustfmt
because `rustfmt.toml` uses unstable options.

## Hard Rules

- Keep `process_packet`, `drain`, and `reset` free of allocation, locks, blocking calls, I/O, and logging. Size storage
  at construction. The allocation-counting integration test must keep passing.
- The library crate and the evaluation crate's library forbid `unsafe` code. The only permitted `unsafe` is in the two
  allocation-counting `GlobalAlloc` implementations that delegate to `std::alloc::System` and count per thread: the
  library's `allocation` test harness and the `noise-oxydation-eval` binary (`src/main.rs`). Do not add a third site,
  and keep explicit SIMD, if ever adopted, in safe abstractions.
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
- Keep downloaded audio and rendered audio out of Git: the fetch script and the replay write only under the ignored
  `/audio/` directory, and the DEMAND license forbids redistributing derived mixes. Evidence runs stay out of CI.
- Prefer type-safe representations over sentinel values, and typed error enums over strings. Map errors at crate
  boundaries while preserving their source.
