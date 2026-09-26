# Noise Oxydation

Noise Oxydation is a Rust library for enhancing mono telephony speech in a streaming pipeline. Its first usable interface will process 20 ms, 8 kHz G.711 μ-law packets; later checkpoints will cover the full documented DSP pipeline.

The implementation is being written independently from the [Go reference project](https://github.com/sghaida/noise-cancelation), using its published behavior and signal-processing documentation at revision [`cfc7520`](https://github.com/sghaida/noise-cancelation/commit/cfc7520a0625da90e4ad4699541a6ffe98e7c637). The Go source is not copied into this repository.

The project is at its design baseline. The packet API, DSP implementation, tests, and CI are still in progress. See [Architecture](ARCHITECTURE.md) for the current implementation contract and [Go reference observations](docs/reference-observations.md) for documented discrepancies and open questions.

Planned license: MIT.
