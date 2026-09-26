# Repository guidance

- This Rust 2024 workspace has three current owners: `codec` converts G.711 μ-law, `dsp` owns fixed-frame signal processing, and `pipeline` owns packet and call lifecycle. Keep dependency direction from pipeline to the other crates.
- The public path is 8 kHz mono and consumes 160-byte packets. Preserve ordered valid samples, the whole-packet process result, one-time finish with its final valid count, and full reset.
- Keep processing and finish free of allocation and blocking synchronization. Recheck `crates/pipeline/tests/no_alloc.rs` when changing those paths.
- The quiet intro is a five-second default configured at runtime. Keep docs and tests aligned with the observed 128-sample frame cutoff and baseline bypass.
- SPP-MMSE is the default of three public noise estimators; MCRA and minimum estimation use a fixed 50-frame history. MCRA protects sustained narrowband speech after window turnover while the simple minimum remains a lower-envelope estimator. Tonal gain follows Log-MMSE for every selection. Keep each call's estimator and tonal buffers private to its DSP instance, and document chosen formulas separately from unverified Go behavior.
- Run the locked workspace build, tests, Clippy with pedantic warnings denied, and nightly rustfmt check before returning a candidate.
