//! Real-time noise reduction for 8 kHz mono G.711 μ-law telephony packets.
//!
//! Create one [`CallEnhancer`] per call, feed it every 160-byte packet with
//! [`process_packet`](CallEnhancer::process_packet), and forward each emitted packet. Output starts after a fixed
//! two-packet (40 ms) delay; [`drain`](CallEnhancer::drain) returns the withheld packets at the end of the call, and
//! [`reset`](CallEnhancer::reset) reuses the instance for another call.
//!
//! ```
//! use noise_oxydation::{CallConfig, CallEnhancer, DELAY_PACKETS, Packet, PacketOutcome};
//!
//! let mut enhancer = CallEnhancer::new(&CallConfig::default())?;
//! let incoming: Vec<Packet> = vec![[0xFF; 160]; 50];
//! let mut outgoing = Vec::new();
//! let mut output = [0; 160];
//! for packet in &incoming {
//!     if enhancer.process_packet(packet, &mut output)? == PacketOutcome::Emitted {
//!         outgoing.push(output);
//!     }
//! }
//! let mut tail = [[0; 160]; DELAY_PACKETS];
//! let withheld = enhancer.drain(&mut tail)?;
//! outgoing.extend_from_slice(&tail[..withheld]);
//! assert_eq!(outgoing.len(), incoming.len());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Each packet is μ-law decoded, high-pass filtered, analyzed into 256-sample Hann frames every 128 samples, and each
//! frame's noise power is estimated with the configured [`NoiseEstimatorConfig`] (SPP-MMSE by default, or MCRA or
//! the minimum estimator). During the quiet-intro calibration (default 5 s) frames pass through un-enhanced while the
//! estimator learns the noise; afterwards each frame is suppressed with Log-MMSE driven by decision-directed SNR
//! estimates, then attenuated by the tonal transient detector unless [`InterferenceConfig::Disabled`] is chosen,
//! resynthesized by weighted overlap-add, and μ-law encoded.
//!
//! # Features
//!
//! - `log` (default): one debug record from [`CallEnhancer::new`] through the `log` facade. The packet path never logs.
//! - `stage-timing` (off by default): each call accumulates per-stage invocation counts, total and maximum durations,
//!   read with `CallEnhancer::stage_timings`. It reads `Instant::now` once per stage boundary and adds no allocation,
//!   locks, atomics or I/O; the enhanced output is unchanged.
#![forbid(unsafe_code)]

mod analysis;
mod config;
mod convert;
mod decision_directed;
mod enhancer;
mod error;
mod fft;
mod geometry;
mod highpass;
mod log_mmse;
mod mcra;
mod minimum;
mod mulaw;
mod noise_estimator;
mod output_queue;
mod power;
mod spp_mmse;
mod stage_timing;
mod synthesis;
mod tonal;
mod window;

#[cfg(test)]
#[path = "../tests/unit/support.rs"]
mod test_support;

#[cfg(feature = "stage-timing")]
pub use crate::stage_timing::{Stage, StageTiming, StageTimings};
pub use crate::{
    config::{
        CallConfig, DecisionDirectedConfig, HighPassConfig, InterferenceConfig, LogMmseConfig, McraConfig,
        MinimumConfig, NoiseEstimatorConfig, SppMmseConfig, TonalTransientConfig,
    },
    enhancer::{CallEnhancer, CallPhase, DELAY_PACKETS, Packet, PacketOutcome},
    error::{ConfigError, ConfigField, Constraint, StreamError},
    geometry::{PACKET_SAMPLES, SAMPLE_RATE_HZ},
};
