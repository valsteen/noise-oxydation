# Agent Guidance

- For answer, explanation, review assessment, and diagnosis requests, inspect and report without implementation writes. A human-owned pull-request review may record its separate review evidence without authorizing implementation edits.
- Before the first implementation write after a change request, state whether the work follows an existing or new Pinboard item or proceeds directly outside Pinboard. This route announcement grants no additional authority.
- Follow the installed `$pinboard` skill for the route choice and subsequent workflow.

## Rust workspace

- Format with `cargo +nightly-2026-09-25 fmt --all`.
- Run Clippy, tests, and builds with the locked stable Rust 1.98.1 toolchain; CI records the exact commands.
- Keep packet processing bounded and call-local, and preserve the caller-owned output-buffer contract.
