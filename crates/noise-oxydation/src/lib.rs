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
//! frame's noise power is estimated with SPP-MMSE. During the quiet-intro calibration (default 5 s) frames pass
//! through un-enhanced while the estimator learns the noise; afterwards each frame is suppressed with Log-MMSE driven
//! by decision-directed SNR estimates, resynthesized by weighted overlap-add, and μ-law encoded.
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
mod mulaw;
mod output_queue;
mod spp_mmse;
mod synthesis;
mod window;

#[cfg(test)]
#[path = "../tests/unit/support.rs"]
mod test_support;

pub use crate::{
    config::{CallConfig, DecisionDirectedConfig, HighPassConfig, LogMmseConfig, SppMmseConfig},
    enhancer::{CallEnhancer, CallPhase, DELAY_PACKETS, Packet, PacketOutcome},
    error::{ConfigError, ConfigField, Constraint, StreamError},
    geometry::{PACKET_SAMPLES, SAMPLE_RATE_HZ},
};
