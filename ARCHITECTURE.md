# Architecture

## Product boundary

The library enhances 8 kHz mono speech carried in 160-byte G.711 μ-law packets. One pipeline instance owns the state for one call. Callers may run independent instances in parallel. The library does not own a microphone, Twilio connection, UI, or worker pool.

The first checkpoint will provide a packet-in/packet-out enhancement path and document its startup delay and flush behavior. Later checkpoints will add the remaining documented estimators and suppressors. A packet output must never silently claim that buffered samples have been processed when they have not.

## Dependency direction

The intended workspace has three roles, each grouped by dependency surface:

- `codec` owns G.711 μ-law conversion and has no DSP or runtime dependencies.
- `dsp` owns the high-pass filter, STFT/ISTFT, noise estimators, SNR estimation, Log-MMSE suppression, and tonal transient suppression. Algorithm variants stay as modules until a current consumer justifies another crate boundary.
- `pipeline` owns the streaming packet API, buffer lifecycle, and composition of codec and DSP stages. It depends on `codec` and `dsp`; neither depends on it.

The code may use a focused FFT dependency if it offers prepared plans and caller-owned scratch storage. The packet path must not allocate after construction. New dependencies must earn a current role rather than a possible future use.

## Audio flow

The documented reference flow is μ-law decode, high-pass filtering, windowed STFT, noise-power estimation, decision-directed SNR, Log-MMSE gain, tonal transient gain, overlap-add ISTFT, and μ-law encode. The first checkpoint may implement a useful subset, but later checkpoints must complete the documented stages and alternatives. The reference configuration is a 256-sample FFT with a 128-sample hop. The reference offers MCRA and SPP-MMSE noise estimators.

Each stage owns its mutable state and preallocated buffers. Processing one call is sequential because each frame depends on prior state; independent calls can run in parallel. A ring buffer is appropriate for the bounded overlap between packets and FFT frames. Atomics belong only at a demonstrated cross-thread boundary. Related values that must form a coherent snapshot must not be split across unrelated atomics.

## Boundaries and evidence

Configuration and packet boundaries validate their inputs before domain processing. Errors remain typed at the boundary that can explain them; adapters remap errors only when needed and retain relevant causes. The DSP core does not depend on logging or tracing. Optional diagnostics must be cheap when enabled and absent from the hot path when disabled.

The scalar implementation and representative audio checks establish the baseline. Benchmarks will measure per-packet time, allocation count, and aggregate independent-call throughput before a vectorized implementation is considered. Any optimization must preserve the same signal-processing contract and be verified against it.

The Rust build uses the stable channel. Formatting may use nightly rustfmt for the selected style options, as in the two local reference workspaces. CI will check formatting, Clippy with pedantic warnings denied, tests, and build output.

## Current state

This document records the accepted direction before implementation. There is no Rust audio path yet. Startup latency, exact packet output timing, algorithm parameter defaults, and the final crate graph must be reported from the implemented and tested design rather than inferred from this outline.
