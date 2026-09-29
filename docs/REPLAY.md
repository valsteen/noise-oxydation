# Offline speech replay

The replay example lets a maintainer inspect one attributed speech utterance through the public Rust packet processor. It reads a converted clean WAV, constructs an aligned synthetic-noise input, pushes ordered μ-law packets through `Processor`, drains once, and writes the noisy speech and enhanced result under a new directory in the system temporary root. It performs no runtime work inside the installed library beyond the normal push and drain calls.

## Source and attribution

Speech comes only from OpenSLR Mini LibriSpeech SLR31 `dev-clean-2`, utterance `3576-138058-0005`. OpenSLR lists SLR31 under [CC BY 4.0](https://openslr.org/31/). Attribute the source as: “LibriSpeech, dev-clean-2, utterance 3576-138058-0005, distributed by OpenSLR under CC BY 4.0.” The replay uses a derived sample-rate/channel/encoding conversion; it does not apply this repository's MIT license to the recording. The archive, source FLAC, converted WAV, and rendered WAVs remain outside the Git checkout and are not committed or redistributed here.

The official source archive is `https://openslr.org/resources/31/dev-clean-2.tar.gz`, with MD5 `6d7ab67ac6a1d2c993d050e16d61080d`. That archive checksum was verified during authorized source preparation before this continuation; this continuation did not download or reread the archive. The selected member is `LibriSpeech/dev-clean-2/3576/138058/3576-138058-0005.flac`, SHA-256 `8e00599659c75a55eadc254069a92bcfedb75b7e92e91034fbc79f9dbf33f8c4`.

Convert the selected FLAC with FFmpeg 8.0.1 using this command from the directory containing the source member:

```sh
ffmpeg -i 3576-138058-0005.flac -ar 8000 -ac 1 -c:a pcm_s16le clean.wav
```

The converted WAV is PCM16 mono at 8 kHz, contains 133,240 samples, and has SHA-256 `ea3f2a5f62e7e8c5751609927f47101de09ff61a9be691b2792540bbaa0b70b1`. The earlier authorized source preparation verified the official archive MD5 and selected FLAC hash; the staged FLAC and converted WAV hashes and format were rechecked for this replay. Keep both source files outside the checkout.

## Mix, alignment, and replay

The input WAV parser accepts one complete RIFF/WAVE file with PCM format, one channel, 8,000 Hz, 16 bits per sample, and exactly 133,240 samples. It rejects mismatched RIFF/chunk sizes, truncated chunks, duplicate format or data chunks, incomplete samples, and an unexpected sample count.

The mixer initializes xorshift32 with the fixed seed `0x4e4f4953`. Each step updates the `u32` state in order with left 13, right 17, and left 5 shifts: `state ^= state << 13; state ^= state >> 17; state ^= state << 5;`. Each generated value is `state / 2^32 - 0.5`, producing a centered uniform sample in `[-0.5, 0.5)`. One continuous noise sequence covers the 40,000-sample (five-second) calibration prefix and the following speech. Its RMS over the speech segment is scaled to the clean utterance RMS. If the combined peak would clip PCM16, one shared gain is applied to the clean reference, noise, and mixture. This is a nominal 0 dB pre-quantization noise-to-clean target; measured input metrics are computed after PCM16 and μ-law quantization.

The exact clean sample array begins at noisy-stream offset 40,000. The example checks that the stream length equals `40,000 + 133,240` and verifies that the calibration prefix is not silent before scoring. It makes 1,083 ordered pushes of fixed 160-byte μ-law packets, padding only the last packet with 40 μ-law-silence samples. It drains once, removes those known padding samples, and requires 173,240 valid output samples. The scored clean and noisy/enhanced ranges are both 133,240 samples: `clean[0..133240)` maps to `noisy/enhanced[40000..173240)`.

Run the example with a clean WAV kept outside the checkout:

```sh
cargo +1.98.1 run --release --locked --package noise-oxydation-core --example replay -- /path/to/clean.wav
```

Progress diagnostics can be suppressed with `--quiet` after the WAV path. The quality summary remains visible. The executable prints the temporary output paths; each run creates a unique directory and opens each WAV with create-new semantics so existing outputs are never overwritten. For listening on macOS, use `afplay` with each printed WAV path. On a system with FFplay, use `ffplay -nodisp -autoexit` with each path.

## Metrics and observed run

For reference samples `x` and observed samples `y`, speech SNR is `10 log10(sum(x²) / sum((y - x)²))`. SI-SDR first computes `alpha = dot(x, y) / sum(x²)`, then evaluates `10 log10(sum((alpha x)²) / sum((y - alpha x)²))`. Both metrics use the same post-shared-gain clean reference and same aligned speech interval. The noisy baseline is the input after μ-law encode/decode; the enhanced result is the output after the processor's μ-law encoding and decode. These calculations measure the rendered sample streams, not perceived quality.

The authorized run used the verified `/private/tmp/noise-oxydation-librispeech/clean.wav`. It reported 133,240 clean samples, 40,000 prefix samples, noise RMS scale `5647.75436793`, shared gain `1.0`, and 40 final padding samples. On that run, noisy speech measured SNR `-0.10 dB` and SI-SDR `-0.05 dB`; enhanced speech measured SNR `4.93 dB` and SI-SDR `3.36 dB`. Both rendered files were confirmed as PCM16 mono 8 kHz WAVs with 133,240 samples. Playback worked. The user reported a saturation-like voice artifact around “Bengal light” and chose to preserve post-calibration minimum-noise adaptation while deferring its correction to a later audio-quality task.

Rerunning prints fresh noisy and enhanced output paths beneath the system temporary root. The specific temporary paths and checksums for the delivered run are retained in the private attempt result rather than treated as permanent project data.

## Performance and vectorization

The `performance-analysis` feature adds opt-in `--profile` support to the example and no dependencies. It times each initialized packet push and the single drain while using reusable caller-owned input/output storage. Processor construction, source reads, μ-law conversion, metrics, allocation reporting, WAV writing, and console output are outside the timed calls. A separate pass counts allocator calls made during initialized push and drain; those counts are not inferred from latency. There is no tracing callback or logging in the processor.

The measured command was `cargo +1.98.1 run --release --locked --package noise-oxydation-core --example replay --features performance-analysis -- /private/tmp/noise-oxydation-librispeech/clean.wav --quiet --profile`. It ran on macOS 15.5 (Darwin 24.5.0), Apple M1 Ultra, `aarch64`, with `rustc 1.98.1 (48a229cea 2026-09-01)` in the release profile. The first pass recorded 1,083 pushes: p50 `14,625 ns`, p95 `30,333 ns`, p99 `39,334 ns`, and maximum `116,584 ns` (0.58% of the 20 ms packet cadence); one drain took `22,916 ns`. A second verification pass on the same host recorded p50 `14,250 ns`, p95 `29,208 ns`, p99 `32,708 ns`, and maximum `261,625 ns` (1.31% of cadence); its drain took `22,833 ns`. The separate allocation pass observed zero allocator calls across push and zero during drain. Timer and allocator instrumentation perturb execution differently; the allocation pass is separate, and neither host result nor its cadence ratio is a cross-machine guarantee.

The plausible custom hot loops are the 129-bin per-frame estimator and suppression passes, plus small packet and codec loops; the FFT itself is delegated to `realfft`. The release scalar profile is the baseline. No portable explicit SIMD candidate was selected or benchmarked, so no SIMD output-equivalence comparison was performed and no end-to-end SIMD gain is claimed. No minitrace comparison was run; this offline example already reports per-call measurements and no tracing consumer or measured benefit was established. Neither SIMD nor minitrace was added. The replay does not measure end-to-end scheduling or hard real-time behavior.

One utterance with seeded synthetic uniform noise does not establish general speech quality, robustness to natural or nonstationary noise, performance on other hosts, or numerical parity with the pinned Go reference. Those questions require separate representative evidence.


## Go wrapper measurement

The Go measurement command accepts an external raw μ-law input. For the recorded run, the verified 133,240-sample PCM16 mono 8 kHz `clean.wav` was converted with FFmpeg 8.0.1 to a raw μ-law stream using `-ar 8000 -ac 1 -c:a pcm_mulaw -f mulaw`. The resulting input contained 133,240 bytes and had SHA-256 `db6f1f2ea35fddeaa791aaaebf652c8baf00dcffe7a998d7b2f85b99b34c2e48`. Its source WAV SHA-256 is `ea3f2a5f62e7e8c5751609927f47101de09ff61a9be691b2792540bbaa0b70b1`; both files remained outside the checkout.

Run it from `bindings/go` after building the Rust static archive:

```sh
cargo +1.98.1 build --release --locked --package noise-oxydation-ffi
cd bindings/go
CGO_ENABLED=1 go run ./cmd/measure /path/to/clean.ulaw
```

The command prepends 40,000 μ-law silence bytes for the default five-second quiet calibration, pads its final packet with μ-law silence, and reuses fixed packet and output arrays. It performs one Go heap-allocation pass and a separate timed pass so reporting and percentile sorting do not enter packet timings. Allocations per packet count Go heap allocations during Push and Drain; they do not measure the Rust allocator.

The run on 2026-09-29 used macOS 15.5 (`darwin/arm64`), Go 1.27.1, and the repository's Rust 1.98.1 static archive. It processed 1,083 packets, including the calibration prefix and final padding. The measured pass recorded 1.0009 Go allocations and 8.01 allocated bytes per packet. Push latency was p50 `0.016 ms`, p95 `0.039 ms`, p99 `0.056 ms`, and maximum `0.105 ms`; one drain took `0.033 ms`. Push p99 and maximum were 0.3% and 0.5% of the 20 ms packet cadence. These are single-host call timings for this input, not scheduling guarantees, cross-host performance, or audio-quality evidence.
