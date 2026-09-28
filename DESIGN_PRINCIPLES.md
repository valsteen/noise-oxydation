# Design decisions

These decisions adapt the reviewed project guidance to a bounded Rust packet processor. They describe the current code and accepted deferrals.

| Concern | Decision in this crate | Evidence and disposition |
| --- | --- | --- |
| Dependency and module surface | Keep packet framing, codec, filtering, transforms, calibration, gating, and lifecycle in one core crate. Keep `realfft` as the focused transform dependency. Add another owner only when a distinct replay-I/O or Go ABI consumer exists. | Reviewed guidance: `/Users/vincentalsteen/projects/bitwig-remote-project/client/agent-guides/architecture.md` and `/Users/vincentalsteen/projects/codex-repo-work/docs/engineering-style.md`. The accepted brief records these source pointers and the one-crate disposition. |
| Error provenance | Use typed errors for caller-visible capacity and lifecycle rejections. Do not make expected packet rejections depend on panic or hidden cross-layer translation. | The accepted brief records this error-ownership disposition for the packet boundary. |
| Per-call ownership | Keep mutable filter, calibration, frame, overlap, and stream state on one `Processor`. Prepare transform plans and scratch storage during construction. Keep initialized pushes and drains bounded, allocation-free, and free of blocking synchronization. | This follows the accepted call-local real-time contract. Independent processor tests exercise isolation and allocation behavior. |
| More elaborate suppression | Keep the selected minimum-noise spectral gate as the only current gate. Defer MCRA, SPP-MMSE, decision-directed SNR, Log-MMSE, and tonal/transient suppression to their accepted follow-on scope. | The accepted user choice and pinned-reference review define this boundary. No unmeasured parity or quality claim is made. |
| Measurement and instrumentation | Do not add SIMD, minitrace, logging, or opt-in performance instrumentation to this crate. Revisit performance work with the accepted replay and measurement evidence. | The accepted design disposition defers SIMD and tracing pending measurement and assigns replay and performance analysis to follow-on work. |
| Application-host architecture | Do not introduce MIDI, GUI, host, or transport architecture into this audio core. | The accepted design review rejects the Bitwig/MIDI host architecture for this independent packet processor. |
| Visual explanation | When a verified visitor guide is added, adapt its day/night layout to audio concepts; do not copy its unrelated subject matter. | Reviewed visual-language guidance: `docs/how_it_works/VISUAL_LANGUAGE.md` in codex-repo-work. Diagrams are outside the current work. |
| Rust formatting | Use the user-selected nightly formatter and the exact shared option values listed in `rustfmt.toml`. | Values are copied from `/Users/vincentalsteen/projects/bitwig-remote-project/client/rustfmt.toml`; stable 1.98.1 remains the build, test, and Clippy toolchain. |

The reviewed source set recorded by the accepted brief is:

- `/Users/vincentalsteen/projects/bitwig-remote-project/client/agent-guides/architecture.md`
- `/Users/vincentalsteen/projects/bitwig-remote-project/client/agent-guides/rust.md`
- `/Users/vincentalsteen/projects/bitwig-remote-project/docs/engineering-style.md`
- `/Users/vincentalsteen/projects/midi-bpm-detector/rust/agent-guides/architecture.md`
- `/Users/vincentalsteen/projects/midi-bpm-detector/rust/agent-guides/tooling.md`
- `/Users/vincentalsteen/projects/codex-repo-work/docs/how_it_works/VISUAL_LANGUAGE.md`

The accepted brief is the disposition record for these sources. Their project-specific rules are not copied wholesale into this repository.
