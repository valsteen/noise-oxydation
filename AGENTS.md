# AGENTS.md

Instructions for AI coding agents working in this repository.

## Read First

- [ARCHITECTURE.md](ARCHITECTURE.md) owns the crate map, dependency direction, per-call ownership, the packet timing
  contract, calibration, and the audio-critical constraints. Keep it current in the same change as the code.
- [docs/reference-log.md](docs/reference-log.md) owns every difference from, or concern about, the Go behavior reference.
  Add an entry whenever you rely on, disagree with, or diverge from reference behavior. Never claim numerical parity
  without a measured entry.
- [docs/lint-exceptions.md](docs/lint-exceptions.md) lists every lint expectation.

## Commands

Run from the repository root. Builds are locked: always pass `--locked`.

```bash
cargo +nightly-2026-09-25 fmt --all --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo build --locked --workspace --release
```

The stable toolchain is pinned in `rust-toolchain.toml`. Formatting intentionally uses the pinned nightly rustfmt
because `rustfmt.toml` uses unstable options.

## Hard Rules

- Keep `process_packet`, `drain`, and `reset` free of allocation, locks, blocking calls, I/O, and logging. Size storage
  at construction. The allocation-counting integration test must keep passing.
- The library crate forbids `unsafe` code. The only permitted `unsafe` is the test-only allocation-counting
  `GlobalAlloc` harness that delegates to `std::alloc::System`; it counts per thread so parallel tests stay independent.
- Treat compiler and Clippy (pedantic) warnings as work to fix. Do not add `#[allow]`/`#[expect]` or tool-level lint
  exceptions except a narrow `#[expect(..., reason = "...")]` in the audited numeric-conversion module, recorded in
  `docs/lint-exceptions.md`. If another exception looks necessary, stop and explain the tradeoff.
- Do not introduce macros (`macro_rules!` or procedural) without explicit human agreement.
- Do not add inline `mod tests { ... }` bodies to production files. Unit tests live in `tests/unit/<module>.rs` of the
  owning crate, wired with:

  ```rust
  #[cfg(test)]
  #[path = "../tests/unit/<module>.rs"]
  mod tests;
  ```

  Integration tests live under `tests/integration/`.
- Do not add tautological tests that restate constants or the production algorithm. Test observable behavior, failure
  handling, lifecycle, regressions, and independently derived expectations (closed-form math, reference-documented
  values).
- Production code must not expose behavior that exists only for tests.
- Do not copy Go reference code. Derive behavior from the pinned reference documentation and source, and log
  differences in `docs/reference-log.md`.
- Keep downloaded audio and rendered audio out of Git (`/audio/` is ignored).
- Prefer type-safe representations over sentinel values, and typed error enums over strings. Map errors at crate
  boundaries while preserving their source.
