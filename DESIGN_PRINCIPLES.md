# Design decisions

These decisions adapt the reviewed project guidance to a bounded Rust packet processor. They describe current code and evidence limits.

| Concern | Decision in this crate | Evidence and disposition |
| --- | --- | --- |
| Dependency and module surface | Keep packet framing, codec, filtering, transforms, estimator and suppression state, calibration, overlap-add, and lifecycle in one core crate. Keep `realfft` as the focused transform dependency. Add another owner only when a distinct replay-I/O or Go ABI consumer exists. | Reviewed guidance: `/Users/vincentalsteen/projects/bitwig-remote-project/client/agent-guides/architecture.md` and `/Users/vincentalsteen/projects/codex-repo-work/docs/engineering-style.md`. The accepted brief records these source pointers and the one-crate disposition. |
| Error provenance | Use typed errors for caller-visible capacity and lifecycle rejections. Do not make expected packet rejections depend on panic or hidden cross-layer translation. | The accepted brief records this error-ownership disposition for the packet boundary. |
| Per-call ownership | Keep mutable filter, estimator, SNR, suppressor, tonal detector, calibration, frame, overlap, and stream state on one `Processor`. Prepare transform plans and scratch storage during construction. Keep initialized pushes and drains bounded, allocation-free, and free of blocking synchronization. | This follows the accepted call-local real-time contract. Independent processor tests exercise isolation and allocation behavior. |
| Suppression path | Use the README-derived minimum-noise moving-window estimator by default, with selectable source-backed MCRA and SPP-MMSE. Feed selected noise PSD and observed power through decision-directed SNR and Log-MMSE, then apply tonal-transient gain from original frame power. Keep the detector described as a spectral heuristic. | The accepted checkpoint and `docs/REFERENCE_NOTES.md` record source provenance and defaults. No unmeasured parity or speech-quality claim is made. |
| Replay and measurement | Keep audio parsing, synthetic mixing, packet replay, metrics, logs, timing, and allocation instrumentation in the dependency-free example. Keep initialized `Processor` calls free of those effects. Gate timing and separate allocation counting behind the example-only `performance-analysis` feature. | One release run on the documented host reports active-call latency and allocations; it is a host observation, not a performance guarantee. No portable SIMD candidate or tracing benefit was established, so neither SIMD nor minitrace was added. |
| Application-host architecture | Do not introduce MIDI, GUI, host, or transport architecture into this audio core. | The accepted design review rejects the Bitwig/MIDI host architecture for this independent packet processor. |
| Visual explanation | Maintain [HOW_IT_WORKS.md](HOW_IT_WORKS.md) as the visitor guide to the crate boundary and ordered packet path; generate matching day/night diagrams with the standard-library Rust example. | Adapt the user-approved visitor-guide grammar to audio concepts. Source: `/Users/vincentalsteen/projects/codex-repo-work/docs/how_it_works/VISUAL_LANGUAGE.md`, SHA-256 `eca1cbf7ba8e8da37d96832c6cf3fcc79d7c8a56b2c86d4bc98adeb896d9b152`. |
| Rust formatting | Use the user-selected nightly formatter and the exact shared option values listed in `rustfmt.toml`. | Values are copied from `/Users/vincentalsteen/projects/bitwig-remote-project/client/rustfmt.toml`; stable 1.98.1 remains the build, test, and Clippy toolchain. |

The reviewed source set recorded by the accepted brief is:

- `/Users/vincentalsteen/projects/bitwig-remote-project/client/agent-guides/architecture.md`
- `/Users/vincentalsteen/projects/bitwig-remote-project/client/agent-guides/rust.md`
- `/Users/vincentalsteen/projects/bitwig-remote-project/docs/engineering-style.md`
- `/Users/vincentalsteen/projects/midi-bpm-detector/rust/agent-guides/architecture.md`
- `/Users/vincentalsteen/projects/midi-bpm-detector/rust/agent-guides/tooling.md`
- `/Users/vincentalsteen/projects/codex-repo-work/docs/how_it_works/VISUAL_LANGUAGE.md`

The accepted brief is the disposition record for these sources. Their project-specific rules are not copied wholesale into this repository.
