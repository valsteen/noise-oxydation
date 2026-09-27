# Processing modes and comparison

A call can select either processing mode when it is created. `Conservative` is the default and retains the released packet and audio behavior. `ExperimentalLowDelay` returns complete audio earlier, while its suppression quality and processing cost can differ. Both modes are built into the same Rust library and Go binary; no compile-time flag or second native archive is needed.

| Property | Conservative | Experimental lower delay |
| --- | --- | --- |
| Analysis | 256 samples, periodic Hann | 256 samples, asymmetric analysis window |
| Hop | 128 samples / 16 ms | 80 samples / 10 ms |
| Synthesis | Full 256-sample Hann, normalized overlap | Last 160 samples of each frame, asymmetric window |
| First complete 160-sample output packet | Input packet 3 | Input packet 2 |
| Minimum/MCRA history | 50 frames / 800 ms | 80 frames / 800 ms |
| Quiet intro | Five seconds by default, original decoded audio bypasses suppression | Same duration and bypass, with an independent frame schedule |

The first experimental output packet becomes reconstructible after input sample 240 (30 ms of captured audio); the 160-byte packet API returns it when input packet two arrives at 40 ms. The conservative mode first has a complete output packet after sample 384 (48 ms) and returns it with input packet three at 60 ms. This removes **one 20 ms packet wait** at startup. It does not reduce the caller's 20 ms capture interval, network time, or Go/OS scheduling delay. Independent calls still run concurrently when the caller schedules them separately; each call processes its own frames in order.

The experimental analyzer uses 176 samples of zero pre-padding at call start. Its analysis and synthesis windows sum to unity through overlap-add when spectral gains are disabled; a numerical test covers impulses, a sine wave, random noise, and startup. Once Log-MMSE and tonal gains change the spectrum, that identity result no longer guarantees perceptual quality. The 80-sample hop runs 100 spectral frames per second instead of 62.5. The minimum/MCRA history grows to 80 frames to preserve the 800 ms horizon; this corrected a synthetic speech-energy regression seen with 50 experimental frames. Other estimator and tonal smoothing coefficients remain frame-based and therefore have different time constants in the experimental path. The calibration boundary also falls on a different frame schedule. These are reasons to evaluate real calls before selecting the experimental mode for production.

## Select a mode

Rust callers continue to use `Pipeline::new(Config::default())` for the conservative path. Select the experiment per call with `Pipeline::new_with_mode(config, ProcessingMode::ExperimentalLowDelay)`. Existing C callers can keep `no_create`; `no_create_with_mode` adds a mode argument (`0` conservative, `1` experimental). Invalid values return `NO_MODE`. In Go, set `Config.Mode` to `noise.ExperimentalLowDelay`; the zero value and `DefaultConfig()` select `noise.Conservative`. The mode cannot change during a call or on `Reset`; create another call to compare alternatives. All modes preserve whole-packet ordering, valid sample counts, one-time `Finish`, and full `Reset`.

The [Go replay example](../go/README.md) accepts `-mode conservative` or `-mode experimental`. The Rust `wav_replay` and `packet_bench` examples accept the same two words as an optional final argument. Use the same prepared input and estimator for each run, and write outputs to separate directories.

## Reproduce the current comparison

The [Open Speech Repository](https://www.voiptroubleshooter.com/open_speech/american.html) supplies both 8 kHz mono 16-bit PCM voices used here. They remain outside Git. The [female clip](https://www.voiptroubleshooter.com/open_speech/american/OSR_us_000_0010_8k.wav) is `OSR_us_000_0010_8k.wav`, SHA-256 `a4bf9becd046d7aedb6d05b6e12347a6294a44f74d263089c636fb0a2b1e6561`. The [male clip](https://www.voiptroubleshooter.com/open_speech/american/OSR_us_000_0030_8k.wav) is `OSR_us_000_0030_8k.wav`, SHA-256 `d3e1cfba98a44ddfc6e99508e663794c8dc41bdb2baced224f779c453143b622`. Credit the source as **Open Speech Repository** when reusing them.

For each source, `wav_replay` adds the same fixed-seed broadband noise and five-second quiet intro. It writes the prepared μ-law input if a second argument is supplied. The female prepared input hashes to `ac50c854bbc0500ee478376a1bc74e339d53257170974685403a1674430a082f`; the male input hashes to `07dad2aa6a175869b6a9564e4932c0902b283ead3443705f2675b4f29ab3f7ae`. Run each mode with a separate output directory:

```sh
cargo run --release --locked -p noise-oxydation-pipeline --features performance-analysis --example wav_replay -- /tmp/OSR_us_000_0010_8k.wav /tmp/female-prepared.mulaw /tmp/female-conservative conservative
cargo run --release --locked -p noise-oxydation-pipeline --features performance-analysis --example wav_replay -- /tmp/OSR_us_000_0010_8k.wav /tmp/female-prepared.mulaw /tmp/female-experimental experimental
```

The input hash, estimator, sample alignment, and metric windows are identical between modes. Noise RMS ratio uses samples `[40800,47200)` after calibration; smaller means less residual noise. Speech correlation and projection gain use recorded speech `[48800,48000+N-800)` against the μ-law-quantized clean reference. Correlation measures shape similarity, while projection gain reports retained speech amplitude; neither replaces listening. The [original replay evidence](real-audio-evidence.md) defines the preparation and formulas in detail.

| Voice | Estimator | Mode | Noise RMS ratio | Speech correlation | Projection gain |
| --- | --- | --- | ---: | ---: | ---: |
| Female | SPP-MMSE | Conservative | 0.1040 | 0.8902 | 0.5910 |
| Female | SPP-MMSE | Experimental | 0.0998 | 0.9136 | 0.6501 |
| Female | MCRA | Conservative | 0.1070 | 0.9634 | 0.8383 |
| Female | MCRA | Experimental | 0.1033 | 0.9663 | 0.8567 |
| Female | Minimum | Conservative | 0.1563 | 0.9586 | 0.8303 |
| Female | Minimum | Experimental | **0.2153** | 0.9654 | 0.8761 |
| Male | SPP-MMSE | Conservative | 0.1040 | 0.8790 | 0.6087 |
| Male | SPP-MMSE | Experimental | 0.0998 | 0.9025 | 0.6584 |
| Male | MCRA | Conservative | 0.1070 | 0.9640 | 0.8848 |
| Male | MCRA | Experimental | 0.1033 | 0.9674 | 0.8952 |
| Male | Minimum | Conservative | 0.1563 | 0.9658 | 0.9018 |
| Male | Minimum | Experimental | **0.2153** | 0.9689 | 0.9299 |

The noise numbers repeat across voices because both preparations use the identical fixed-seed noise before speech. The experimental minimum estimator leaves more noise in this scenario despite retaining more projected speech energy. These two voices and one synthetic noise type cannot establish overall speech quality, musical-noise behavior, transient handling, or performance with a different call intro. Compare the emitted audio by listening, especially near speech onset and the five-second transition, before deciding whether that tradeoff is acceptable.

The conservative female outputs in the experimental branch are byte-for-byte identical to the preceding integration branch: SPP-MMSE SHA-256 `d1c85b74f385e53a66c565096d182f33a445f214721c5a7274f9437983c5ff17`, MCRA `d1d930735a10fe340c9b559cdbe7384adf3c9fac5272f4686449677ed23e2a5f`, and minimum `537425a9daf0fff4efecba51fecfa3a78ebdedf5a2cadb5c131aec10aefcc150`. The Go experimental replay matched the Rust SPP-MMSE output hash on the same prepared input.

## Processing cost and live-call limits

On VincentacStudio (macOS arm64, Rust 1.98.1, release build), one warmed offline `packet_bench` run over the 1,982-packet female stream measured these suppression-phase distributions. The benchmark excludes disk I/O, packet arrival wait, cgo, and scheduling. It is a local sample, not a capacity or worst-case guarantee.

| Estimator | Conservative p50 / p95 | Experimental p50 / p95 |
| --- | ---: | ---: |
| SPP-MMSE | 21.67 / 43.67 µs | 41.92 / 45.71 µs |
| MCRA | 22.88 / 46.96 µs | 48.54 / 52.75 µs |
| Minimum | 23.79 / 48.25 µs | 49.29 / 56.54 µs |

Run `packet_bench <prepared.mulaw> conservative` and again with `experimental` to compare on a target host. The Go `cmd/bench` example accepts `-mode` for the Go/cgo packet path. Both modes have allocation-free Rust `process_packet` and `finish` paths after construction. Neither benchmark measures live capture-to-playback delay. A paced, concurrent replay in the actual consumer is still needed to measure scheduling jitter, backpressure, and audible call quality under load. Until that evidence exists, use the conservative default for production.

One warmed Go/cgo run over the same prepared call measured experimental whole-call `Process` p95 of 47.708 µs for SPP-MMSE, 55.916 µs for MCRA, and 56.750 µs for minimum. Conservative p95 was 44.000, 46.750, and 48.916 µs respectively. This Go measurement includes the per-call mutex and cgo call but still excludes packet arrival and playback scheduling. The experimental run observed zero Go mallocs across each measured call; one conservative minimum run observed a single Go runtime allocation, so these counts are observations rather than allocation guarantees for Go.

The asymmetric-window design follows the reconstruction construction described by [Wang et al., 2021](https://arxiv.org/html/2106.11794v1) and earlier low-delay enhancement analysis by [Wood et al., 2017](https://spandh.dcs.shef.ac.uk/chat2017/papers/CHAT_2017_wood.pdf). Their reported perceptual results concern different enhancement systems and do not validate this implementation.
