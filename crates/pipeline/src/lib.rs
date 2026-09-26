//! One pipeline instance owns one 8 kHz mono G.711 μ-law call.

use noise_oxydation_codec::{decode, encode};
pub use noise_oxydation_dsp::NoiseEstimator;
use noise_oxydation_dsp::{DspError, Enhancer, HOP};
use std::error::Error;
use std::fmt;
use std::time::Duration;

pub const PACKET_SAMPLES: usize = 160;
/// Processing or finishing can emit at most two packets per call.
pub const MAX_BATCH_PACKETS: usize = 2;

#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Quiet call intro used to estimate stationary noise. Default: five seconds.
    pub learning_duration: Duration,
    /// Noise estimator. Default: SPP-MMSE. Tonal gain follows Log-MMSE for all variants.
    pub noise_estimator: NoiseEstimator,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            learning_duration: Duration::from_secs(5),
            noise_estimator: NoiseEstimator::default(),
        }
    }
}

#[derive(Debug)]
pub enum PipelineError {
    InvalidLearningDuration,
    DspInitialization(DspError),
    AlreadyFinished,
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLearningDuration => write!(
                f,
                "learning duration must be positive and representable at 8 kHz"
            ),
            Self::DspInitialization(error) => write!(f, "DSP initialization failed: {error}"),
            Self::AlreadyFinished => write!(
                f,
                "call already finished; reset before processing another call"
            ),
        }
    }
}

impl Error for PipelineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::DspInitialization(error) => Some(error),
            _ => None,
        }
    }
}

/// Stack-owned output. Processing only returns whole packets. After `finish`,
/// the last packet reports `final_valid_samples`; all earlier packets contain
/// 160 valid samples. Fixed-size input packets make the current final count 160.
#[derive(Clone, Copy)]
pub struct PacketBatch {
    packets: [[u8; PACKET_SAMPLES]; MAX_BATCH_PACKETS],
    count: usize,
    final_valid_samples: usize,
}

impl PacketBatch {
    fn empty() -> Self {
        Self {
            packets: [[0xff; PACKET_SAMPLES]; MAX_BATCH_PACKETS],
            count: 0,
            final_valid_samples: 0,
        }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.count
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[must_use]
    pub fn packet(&self, index: usize) -> Option<(&[u8; PACKET_SAMPLES], usize)> {
        (index < self.count).then(|| {
            let valid = if index + 1 == self.count {
                self.final_valid_samples
            } else {
                PACKET_SAMPLES
            };
            (&self.packets[index], valid)
        })
    }

    fn push_whole(&mut self, packet: [u8; PACKET_SAMPLES]) {
        debug_assert!(self.count < MAX_BATCH_PACKETS);
        self.packets[self.count] = packet;
        self.count += 1;
        self.final_valid_samples = PACKET_SAMPLES;
    }
}

pub struct Pipeline {
    dsp: Enhancer,
    pending: [u8; PACKET_SAMPLES],
    pending_len: usize,
    input_samples: u64,
    output_samples: u64,
    finished: bool,
}

impl Pipeline {
    /// Construct independent state for one call.
    ///
    /// # Errors
    /// Returns [`PipelineError::InvalidLearningDuration`] for zero or overflow,
    /// or [`PipelineError::DspInitialization`] if the interval is shorter than one FFT frame.
    pub fn new(config: Config) -> Result<Self, PipelineError> {
        let nanos = config.learning_duration.as_nanos();
        let samples = nanos
            .checked_mul(8_000)
            .and_then(|value| value.checked_add(999_999_999))
            .map(|value| value / 1_000_000_000)
            .and_then(|value| u64::try_from(value).ok())
            .filter(|&value| value != 0)
            .ok_or(PipelineError::InvalidLearningDuration)?;
        let dsp = Enhancer::with_estimator(samples, config.noise_estimator)
            .map_err(PipelineError::DspInitialization)?;
        Ok(Self {
            dsp,
            pending: [0xff; PACKET_SAMPLES],
            pending_len: 0,
            input_samples: 0,
            output_samples: 0,
            finished: false,
        })
    }

    /// Consume exactly one 20 ms packet; return zero or more ordered whole packets.
    ///
    /// # Errors
    /// Returns [`PipelineError::AlreadyFinished`] after `finish` until `reset`.
    pub fn process_packet(
        &mut self,
        input: &[u8; PACKET_SAMPLES],
    ) -> Result<PacketBatch, PipelineError> {
        if self.finished {
            return Err(PipelineError::AlreadyFinished);
        }
        let mut batch = PacketBatch::empty();
        for &code in input {
            let sample = f32::from(decode(code)) / 32_768.0;
            if let Some(output) = self.dsp.push(sample) {
                self.append_output(&output, HOP, &mut batch);
            }
        }
        self.input_samples += PACKET_SAMPLES as u64;
        Ok(batch)
    }

    /// Emit the remaining valid samples once. Any partial packet is padded with
    /// μ-law silence and accompanied by its valid count.
    ///
    /// # Errors
    /// Returns [`PipelineError::AlreadyFinished`] on a second call.
    #[allow(clippy::cast_possible_truncation)] // The minimum limits this value to HOP (128).
    pub fn finish(&mut self) -> Result<PacketBatch, PipelineError> {
        if self.finished {
            return Err(PipelineError::AlreadyFinished);
        }
        self.finished = true;
        let mut batch = PacketBatch::empty();
        while self.output_samples < self.input_samples {
            if let Some(output) = self.dsp.push(0.0) {
                let valid = (self.input_samples - self.output_samples).min(HOP as u64) as usize;
                self.append_output(&output, valid, &mut batch);
            }
        }
        if self.pending_len != 0 {
            batch.packets[batch.count] = self.pending;
            batch.count += 1;
            batch.final_valid_samples = self.pending_len;
        }
        Ok(batch)
    }

    /// Reuse the instance for a new call without retaining previous audio.
    pub fn reset(&mut self) {
        self.dsp.reset();
        self.pending.fill(0xff);
        self.pending_len = 0;
        self.input_samples = 0;
        self.output_samples = 0;
        self.finished = false;
    }

    #[allow(clippy::cast_possible_truncation)] // Clamp and rounding bound PCM to i16.
    fn append_output(&mut self, samples: &[f32; HOP], valid: usize, batch: &mut PacketBatch) {
        for &sample in &samples[..valid] {
            let pcm = (sample.clamp(-1.0, 32_767.0 / 32_768.0) * 32_768.0).round() as i16;
            self.pending[self.pending_len] = encode(pcm);
            self.pending_len += 1;
            if self.pending_len == PACKET_SAMPLES {
                batch.push_whole(self.pending);
                self.pending.fill(0xff);
                self.pending_len = 0;
            }
        }
        self.output_samples += valid as u64;
    }
}
