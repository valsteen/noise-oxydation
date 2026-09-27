//! Fixed telephony geometry. These are compile-time constants, not configuration: the packet timing proof and every
//! buffer bound in the crate depend on them (see `ARCHITECTURE.md`, "Fixed Telephony Geometry").

/// Sample rate of G.711 telephony audio, in hertz.
pub const SAMPLE_RATE_HZ: u32 = 8000;

/// Samples (and μ-law bytes) in one 20 ms packet.
pub const PACKET_SAMPLES: usize = 160;

/// Analysis frame length and FFT size, in samples, as a `u16` so that lossless `From` conversions reach every numeric
/// type the DSP code needs.
pub(crate) const FFT_SIZE_U16: u16 = 256;

/// Analysis frame length and FFT size, in samples.
pub(crate) const FFT_SIZE: usize = FFT_SIZE_U16 as usize;

/// Distance between consecutive analysis frames, in samples.
pub(crate) const HOP_SIZE: usize = 128;

/// One-sided spectrum length: `FFT_SIZE / 2 + 1`.
pub(crate) const BINS: usize = FFT_SIZE / 2 + 1;
