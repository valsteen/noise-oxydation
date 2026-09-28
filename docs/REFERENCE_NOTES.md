# Pinned reference notes

The comparison source is `github.com/sghaida/noise-cancelation` at commit `cfc7520a0625da90e4ad4699541a6ffe98e7c637`. The relevant pinned paths are `README.md`, `dsp/stft/stft.go`, and `dsp/stft/istft.go`.

## Evidence classification

| Claim | Classification | Notes |
| --- | --- | --- |
| The README's Pipeline API is illustrative, not an in-tree production package. | Documented | Do not treat the example API as a supported implementation surface. |
| For 960,000 samples, the pinned STFT emits 7,499 complete frames. | Static source finding and arithmetic | With a 256-sample frame and 128-sample hop, the count is `floor((960000 - 256) / 128) + 1 = 7499`. |
| STFT flush adds a padded frame, and ISTFT flush adds 128 samples. | Static source finding | The raw Go path therefore yields 960,128 samples. This differs from the 960,000 accepted input samples. |
| The README's zero-allocation wrapper claim conflicts with its benchmark rows. | Documented conflict | STFT reports 2,296 bytes and 3 allocations; ISTFT reports 512 bytes and 1 allocation. |
| At the 40,000-sample intro boundary, 311 complete 256-sample windows with a 128-sample hop lie wholly inside. | Static arithmetic | A separate reference helper rounds to 313. This Rust crate uses the complete-window rule. |
| Speech in the quiet intro can enter the noise estimate and later be suppressed. | Source-based inference | This is not a measured speech result. |
| Rust behavior is numerically equivalent to Go. | Not established | No audio parity measurement is claimed. |

The Rust processor deliberately preserves one valid output sample per accepted input sample after drain. Its 256-sample startup buffering and second-packet first output are frame arithmetic, not measured end-to-end latency. Its allocator test observes initialized push and drain in this crate; it is not a Go benchmark comparison.
