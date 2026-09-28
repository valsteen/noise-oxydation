//! Bounded, call-local enhancement for 8 kHz G.711 μ-law packets.
//!
//! Each processor owns its transform plans, filter history, selected noise
//! estimator, suppression state, calibration, and overlap buffers. Callers
//! provide all packet and output storage. Processing a packet or draining an
//! initialized processor does not allocate or wait on synchronization.

use std::{f32::consts::PI, sync::Arc, time::Duration};

use realfft::{ComplexToReal, RealFftPlanner, RealToComplex, num_complex::Complex};

/// The input sample rate.
pub const SAMPLE_RATE: usize = 8_000;
/// The exact number of μ-law bytes in one input packet.
pub const PACKET_BYTES: usize = 160;
/// The largest output produced by one packet push.
pub const MAX_PACKET_OUTPUT_BYTES: usize = 256;
/// The largest output produced by a final drain.
pub const MAX_DRAIN_OUTPUT_BYTES: usize = 255;

const FRAME_SIZE: usize = 256;
const HOP_SIZE: usize = 128;
const SPECTRUM_BINS: usize = FRAME_SIZE / 2 + 1;
const MINIMUM_NOISE_WINDOW: usize = 50;
const DEFAULT_CALIBRATION_SAMPLES: u128 = 40_000;
const NANOS_PER_SECOND: u128 = 1_000_000_000;
const POWER_FLOOR: f32 = 1e-12;

/// The noise estimator fixed for one processor call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseEstimator {
    /// The README-specified smoothed minimum estimator; this is the default.
    MinimumNoise,
    /// Minimum controlled recursive averaging.
    Mcra,
    /// Speech-presence-probability MMSE estimation.
    SppMmse,
}

/// A packet processor rejection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessorError {
    /// The caller's output buffer is too small; no input or state was consumed.
    OutputTooSmall { required: usize },
    /// The stream has been drained and must be reset before another push.
    StreamDrained,
    /// The requested calibration duration cannot be represented in samples.
    CalibrationDurationTooLong,
    /// The stream exceeded the processor's exact sample counter range.
    StreamTooLong,
}

impl std::fmt::Display for ProcessorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutputTooSmall { required } => {
                write!(formatter, "output buffer needs at least {required} bytes")
            }
            Self::StreamDrained => formatter.write_str("stream is already drained"),
            Self::CalibrationDurationTooLong => {
                formatter.write_str("calibration duration cannot be represented in samples")
            }
            Self::StreamTooLong => formatter.write_str("stream sample counter is exhausted"),
        }
    }
}

impl std::error::Error for ProcessorError {}

/// One independent audio call.
///
/// Construct a processor before the real-time path and reuse it for the
/// duration of one call. Its methods use bounded, preallocated storage and do
/// not share mutable signal state with another processor.
pub struct Processor {
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    fft_input: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    forward_scratch: Vec<Complex<f32>>,
    inverse_scratch: Vec<Complex<f32>>,
    window: [f32; FRAME_SIZE],
    overlap: [f32; FRAME_SIZE],
    overlap_weight: [f32; FRAME_SIZE],
    input: [f32; FRAME_SIZE],
    pending_len: usize,
    power: [f32; SPECTRUM_BINS],
    noise_psd: [f32; SPECTRUM_BINS],
    noise_estimator: NoiseEstimatorState,
    previous_clean_snr: [f32; SPECTRUM_BINS],
    snr_initialized: bool,
    tonal: TonalState,
    calibration_samples: u128,
    frame_start: u128,
    accepted_samples: u128,
    emitted_samples: u128,
    high_pass: HighPass,
    drained: bool,
}

impl Processor {
    /// Creates a processor with the five-second, 40,000-sample quiet intro.
    #[must_use]
    pub fn new() -> Self {
        Self::with_configuration(DEFAULT_CALIBRATION_SAMPLES, NoiseEstimator::MinimumNoise)
    }

    /// Creates a processor with a configurable quiet-intro duration.
    ///
    /// Durations are converted to 8 kHz samples by rounding down to the nearest
    /// sample. Only complete 256-sample analysis windows wholly inside this
    /// interval train the selected estimator.
    ///
    /// # Errors
    ///
    /// Returns `ProcessorError::CalibrationDurationTooLong` when the duration
    /// cannot be represented in the processor's sample counter.
    pub fn with_quiet_intro(duration: Duration) -> Result<Self, ProcessorError> {
        let samples = duration
            .as_nanos()
            .checked_mul(8_000)
            .map(|value| value / NANOS_PER_SECOND)
            .ok_or(ProcessorError::CalibrationDurationTooLong)?;
        Ok(Self::with_configuration(samples, NoiseEstimator::MinimumNoise))
    }

    /// Creates a processor using one estimator and the default quiet intro.
    #[must_use]
    pub fn with_estimator(estimator: NoiseEstimator) -> Self {
        Self::with_configuration(DEFAULT_CALIBRATION_SAMPLES, estimator)
    }

    /// Creates a processor using one estimator and a configurable quiet intro.
    ///
    /// # Errors
    ///
    /// Returns `ProcessorError::CalibrationDurationTooLong` when the duration
    /// cannot be represented in the processor's sample counter.
    pub fn with_estimator_and_quiet_intro(
        estimator: NoiseEstimator,
        duration: Duration,
    ) -> Result<Self, ProcessorError> {
        let samples = duration
            .as_nanos()
            .checked_mul(8_000)
            .map(|value| value / NANOS_PER_SECOND)
            .ok_or(ProcessorError::CalibrationDurationTooLong)?;
        Ok(Self::with_configuration(samples, estimator))
    }

    fn with_configuration(calibration_samples: u128, estimator: NoiseEstimator) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(FRAME_SIZE);
        let inverse = planner.plan_fft_inverse(FRAME_SIZE);
        let mut window = [0.0; FRAME_SIZE];
        for (index, value) in window.iter_mut().enumerate() {
            *value = hann(index);
        }

        Self {
            fft_input: forward.make_input_vec(),
            spectrum: forward.make_output_vec(),
            forward_scratch: forward.make_scratch_vec(),
            inverse_scratch: inverse.make_scratch_vec(),
            forward,
            inverse,
            window,
            overlap: [0.0; FRAME_SIZE],
            overlap_weight: [0.0; FRAME_SIZE],
            input: [0.0; FRAME_SIZE],
            pending_len: 0,
            power: [0.0; SPECTRUM_BINS],
            noise_psd: [POWER_FLOOR; SPECTRUM_BINS],
            noise_estimator: NoiseEstimatorState::new(estimator),
            previous_clean_snr: [0.0; SPECTRUM_BINS],
            snr_initialized: false,
            tonal: TonalState::new(),
            calibration_samples,
            frame_start: 0,
            accepted_samples: 0,
            emitted_samples: 0,
            high_pass: HighPass::new(),
            drained: false,
        }
    }

    /// Pushes exactly one 160-byte packet and writes any ready output.
    ///
    /// A push can produce zero through 256 ordered μ-law bytes. The first
    /// complete 256-sample window is ready on the second packet push.
    ///
    /// # Errors
    ///
    /// Returns `ProcessorError::OutputTooSmall` with the exact required
    /// capacity, without consuming input or changing state. Returns
    /// `ProcessorError::StreamDrained` after a final drain.
    pub fn push_packet(&mut self, packet: &[u8; PACKET_BYTES], output: &mut [u8]) -> Result<usize, ProcessorError> {
        if self.drained {
            return Err(ProcessorError::StreamDrained);
        }

        let new_total = self.accepted_samples.checked_add(PACKET_BYTES as u128).ok_or(ProcessorError::StreamTooLong)?;
        let ready = self.pending_len + PACKET_BYTES;
        let frame_count = frames_ready(ready);
        let required = frame_count * HOP_SIZE;
        if output.len() < required {
            return Err(ProcessorError::OutputTooSmall { required });
        }

        self.accepted_samples = new_total;
        let mut output_len = 0;
        for &byte in packet {
            self.input[self.pending_len] = self.high_pass.apply(f32::from(decode_mulaw(byte)));
            self.pending_len += 1;
            if self.pending_len == FRAME_SIZE {
                self.process_frame();
                self.write_frame(output, output_len, HOP_SIZE);
                output_len += HOP_SIZE;
                self.advance_frame();
            }
        }
        self.emitted_samples += output_len as u128;
        Ok(output_len)
    }

    /// Emits all remaining valid samples and closes the stream.
    ///
    /// The returned size is zero through 255 bytes. Repeated drains return
    /// empty output.
    ///
    /// # Errors
    ///
    /// Returns `ProcessorError::OutputTooSmall` with the exact required
    /// capacity, without changing state.
    ///
    /// # Panics
    ///
    /// Panics only if internal sample accounting exceeds the bounded frame tail.
    pub fn drain(&mut self, output: &mut [u8]) -> Result<usize, ProcessorError> {
        if self.drained {
            return Ok(0);
        }
        let required = usize::try_from(self.accepted_samples - self.emitted_samples)
            .expect("the streaming remainder is bounded by one frame");
        if output.len() < required {
            return Err(ProcessorError::OutputTooSmall { required });
        }

        let mut output_len = 0;
        while self.emitted_samples < self.accepted_samples {
            self.input[self.pending_len..].fill(0.0);
            self.pending_len = FRAME_SIZE;
            self.process_frame();
            let remaining = usize::try_from(self.accepted_samples - self.emitted_samples)
                .expect("the streaming remainder is bounded by one frame");
            let frame_len = remaining.min(HOP_SIZE);
            self.write_frame(output, output_len, frame_len);
            output_len += frame_len;
            self.emitted_samples += frame_len as u128;
            self.advance_frame();
        }
        self.drained = true;
        Ok(output_len)
    }

    /// Discards pending samples and resets filter, transform, estimator, suppression, and stream state.
    pub fn reset(&mut self) {
        self.fft_input.fill(0.0);
        self.spectrum.fill(Complex::new(0.0, 0.0));
        self.forward_scratch.fill(Complex::new(0.0, 0.0));
        self.inverse_scratch.fill(Complex::new(0.0, 0.0));
        self.overlap.fill(0.0);
        self.overlap_weight.fill(0.0);
        self.input.fill(0.0);
        self.pending_len = 0;
        self.power.fill(0.0);
        self.noise_psd.fill(POWER_FLOOR);
        self.noise_estimator.reset();
        self.previous_clean_snr.fill(0.0);
        self.snr_initialized = false;
        self.tonal.reset();
        self.frame_start = 0;
        self.accepted_samples = 0;
        self.emitted_samples = 0;
        self.high_pass.reset();
        self.drained = false;
    }

    fn process_frame(&mut self) {
        for (output, (&sample, &window)) in self.fft_input.iter_mut().zip(self.input.iter().zip(&self.window)) {
            *output = sample * window;
        }
        self.forward
            .process_with_scratch(&mut self.fft_input, &mut self.spectrum, &mut self.forward_scratch)
            .expect("FFT buffers were created from the matching plan");

        let frame_end = self.frame_start + FRAME_SIZE as u128;
        let complete_input_window = frame_end <= self.accepted_samples;
        let calibration_window = frame_end <= self.calibration_samples && complete_input_window;
        let suppress = frame_end > self.calibration_samples;
        for (power, frequency) in self.power.iter_mut().zip(&self.spectrum) {
            *power = frequency.norm_sqr();
        }
        self.noise_estimator.process(&self.power, calibration_window, suppress, &mut self.noise_psd);
        if suppress {
            self.apply_log_mmse();
        }
        if calibration_window || suppress {
            self.tonal.process(&self.power, &mut self.spectrum, suppress);
        }

        self.inverse
            .process_with_scratch(&mut self.spectrum, &mut self.fft_input, &mut self.inverse_scratch)
            .expect("inverse FFT buffers were created from the matching plan");

        // The fixed 256-sample frame size is exactly representable as f32.
        #[allow(clippy::cast_precision_loss)]
        let inverse_scale = 1.0 / FRAME_SIZE as f32;
        for index in 0..FRAME_SIZE {
            let window = self.window[index];
            self.overlap[index] += self.fft_input[index] * inverse_scale * window;
            self.overlap_weight[index] += window * window;
        }
    }

    fn apply_log_mmse(&mut self) {
        for index in 0..SPECTRUM_BINS {
            let observed_power = self.power[index].max(0.0);
            let effective_noise = (self.noise_psd[index].max(POWER_FLOOR) * 1.25).max(POWER_FLOOR);
            let gamma = f64::from(observed_power) / f64::from(effective_noise);
            let instantaneous_xi = (gamma - 1.0).max(0.0);
            let xi = if self.snr_initialized {
                0.98 * f64::from(self.previous_clean_snr[index]) + 0.02 * instantaneous_xi
            } else {
                instantaneous_xi
            };
            let v = (gamma * xi / (1.0 + xi)).max(f64::from(POWER_FLOOR));
            let gain = (xi / (1.0 + xi) * (0.5 * exp_integral_e1(v)).exp()).clamp(0.05, 1.0);
            #[allow(clippy::cast_possible_truncation)]
            let gain = gain as f32;
            let clean_power = gain * gain * observed_power;
            self.spectrum[index] *= gain;
            self.previous_clean_snr[index] = clean_power / effective_noise;
        }
        self.snr_initialized = true;
    }

    fn write_frame(&mut self, output: &mut [u8], offset: usize, count: usize) {
        for index in 0..count {
            let weight = self.overlap_weight[index];
            let sample = if weight > f32::EPSILON { self.overlap[index] / weight } else { 0.0 };
            output[offset + index] = encode_mulaw(quantize(sample));
        }
    }

    fn advance_frame(&mut self) {
        self.input.copy_within(HOP_SIZE..FRAME_SIZE, 0);
        self.pending_len = FRAME_SIZE - HOP_SIZE;
        self.overlap.copy_within(HOP_SIZE..FRAME_SIZE, 0);
        self.overlap[HOP_SIZE..].fill(0.0);
        self.overlap_weight.copy_within(HOP_SIZE..FRAME_SIZE, 0);
        self.overlap_weight[HOP_SIZE..].fill(0.0);
        self.frame_start += HOP_SIZE as u128;
    }
}

impl Default for Processor {
    fn default() -> Self {
        Self::new()
    }
}

enum NoiseEstimatorState {
    MinimumNoise(Box<MinimumNoiseState>),
    Mcra(Box<McraState>),
    SppMmse(Box<SppMmseState>),
}

impl NoiseEstimatorState {
    fn new(estimator: NoiseEstimator) -> Self {
        match estimator {
            NoiseEstimator::MinimumNoise => Self::MinimumNoise(Box::new(MinimumNoiseState::new())),
            NoiseEstimator::Mcra => Self::Mcra(Box::new(McraState::new())),
            NoiseEstimator::SppMmse => Self::SppMmse(Box::new(SppMmseState::new())),
        }
    }

    fn process(
        &mut self,
        power: &[f32; SPECTRUM_BINS],
        calibration: bool,
        active: bool,
        noise: &mut [f32; SPECTRUM_BINS],
    ) {
        match self {
            Self::MinimumNoise(state) => state.process(power, calibration || active, noise),
            Self::Mcra(state) => state.process(power, calibration, active, noise),
            Self::SppMmse(state) => state.process(power, calibration, active, noise),
        }
    }

    fn reset(&mut self) {
        match self {
            Self::MinimumNoise(state) => state.reset(),
            Self::Mcra(state) => state.reset(),
            Self::SppMmse(state) => state.reset(),
        }
    }
}

struct MinimumNoiseState {
    history: Vec<[f32; SPECTRUM_BINS]>,
    smoothed: [f32; SPECTRUM_BINS],
    cursor: usize,
    initialized: bool,
}

impl MinimumNoiseState {
    fn new() -> Self {
        Self {
            history: vec![[f32::INFINITY; SPECTRUM_BINS]; MINIMUM_NOISE_WINDOW],
            smoothed: [0.0; SPECTRUM_BINS],
            cursor: 0,
            initialized: false,
        }
    }

    fn process(&mut self, power: &[f32; SPECTRUM_BINS], update: bool, noise: &mut [f32; SPECTRUM_BINS]) {
        if !update {
            return;
        }
        for index in 0..SPECTRUM_BINS {
            let observed = power[index].max(0.0);
            self.smoothed[index] =
                if self.initialized { 0.8 * self.smoothed[index] + 0.2 * observed } else { observed };
            self.history[self.cursor][index] = self.smoothed[index];
            noise[index] = self.history.iter().map(|row| row[index]).fold(f32::INFINITY, f32::min).max(POWER_FLOOR);
        }
        self.cursor = (self.cursor + 1) % MINIMUM_NOISE_WINDOW;
        self.initialized = true;
    }

    fn reset(&mut self) {
        self.history.fill([f32::INFINITY; SPECTRUM_BINS]);
        self.smoothed.fill(0.0);
        self.cursor = 0;
        self.initialized = false;
    }
}

struct McraState {
    baseline_sum: [f64; SPECTRUM_BINS],
    noise: [f32; SPECTRUM_BINS],
    smoothed: [f32; SPECTRUM_BINS],
    current_minimum: [f32; SPECTRUM_BINS],
    previous_minimum: [f32; SPECTRUM_BINS],
    speech_probability: [f32; SPECTRUM_BINS],
    baseline_frames: usize,
    frames_in_window: usize,
    initialized: bool,
}

impl McraState {
    fn new() -> Self {
        Self {
            baseline_sum: [0.0; SPECTRUM_BINS],
            noise: [POWER_FLOOR; SPECTRUM_BINS],
            smoothed: [0.0; SPECTRUM_BINS],
            current_minimum: [f32::INFINITY; SPECTRUM_BINS],
            previous_minimum: [f32::INFINITY; SPECTRUM_BINS],
            speech_probability: [0.0; SPECTRUM_BINS],
            baseline_frames: 0,
            frames_in_window: 0,
            initialized: false,
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn process(
        &mut self,
        power: &[f32; SPECTRUM_BINS],
        calibration: bool,
        active: bool,
        output: &mut [f32; SPECTRUM_BINS],
    ) {
        if !calibration && !active {
            output.copy_from_slice(&self.noise);
            return;
        }
        if calibration {
            self.baseline_frames += 1;
            let count = self.baseline_frames as f64;
            for index in 0..SPECTRUM_BINS {
                let value = f64::from(power[index].max(POWER_FLOOR));
                self.baseline_sum[index] += value;
                let mean = (self.baseline_sum[index] / count).max(f64::from(POWER_FLOOR));
                #[allow(clippy::cast_possible_truncation)]
                let mean = mean as f32;
                self.noise[index] = mean;
                self.smoothed[index] = mean;
                self.current_minimum[index] = mean;
                self.previous_minimum[index] = mean;
                self.speech_probability[index] = 0.0;
                output[index] = mean;
            }
            self.initialized = true;
            return;
        }

        if !self.initialized {
            for index in 0..SPECTRUM_BINS {
                let value = power[index].max(POWER_FLOOR);
                self.noise[index] = value;
                self.smoothed[index] = value;
                self.current_minimum[index] = value;
                self.previous_minimum[index] = value;
                self.speech_probability[index] = 0.0;
                output[index] = value;
            }
            self.initialized = true;
            return;
        }

        for index in 0..SPECTRUM_BINS {
            let observed = power[index].max(POWER_FLOOR);
            self.smoothed[index] = 0.8 * self.smoothed[index] + 0.2 * observed;
            self.current_minimum[index] = self.current_minimum[index].min(self.smoothed[index]);
            let minimum = self.current_minimum[index].min(self.previous_minimum[index]).max(POWER_FLOOR);
            let indicator = if self.smoothed[index] / minimum > 5.0 { 1.0 } else { 0.0 };
            self.speech_probability[index] = 0.2 * self.speech_probability[index] + 0.8 * indicator;
            let adaptive_alpha = 0.95 + 0.05 * self.speech_probability[index];
            self.noise[index] =
                (adaptive_alpha * self.noise[index] + (1.0 - adaptive_alpha) * observed).max(POWER_FLOOR);
            output[index] = self.noise[index];
        }
        self.frames_in_window += 1;
        if self.frames_in_window == MINIMUM_NOISE_WINDOW {
            self.previous_minimum.copy_from_slice(&self.current_minimum);
            self.current_minimum.fill(f32::INFINITY);
            self.frames_in_window = 0;
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

struct SppMmseState {
    baseline_sum: [f64; SPECTRUM_BINS],
    noise: [f32; SPECTRUM_BINS],
    speech_probability: [f32; SPECTRUM_BINS],
    smoothed_probability: [f32; SPECTRUM_BINS],
    baseline_frames: usize,
    initialized: bool,
}

impl SppMmseState {
    fn new() -> Self {
        Self {
            baseline_sum: [0.0; SPECTRUM_BINS],
            noise: [POWER_FLOOR; SPECTRUM_BINS],
            speech_probability: [0.0; SPECTRUM_BINS],
            smoothed_probability: [0.0; SPECTRUM_BINS],
            baseline_frames: 0,
            initialized: false,
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn process(
        &mut self,
        power: &[f32; SPECTRUM_BINS],
        calibration: bool,
        active: bool,
        output: &mut [f32; SPECTRUM_BINS],
    ) {
        if !calibration && !active {
            output.copy_from_slice(&self.noise);
            return;
        }
        if calibration {
            self.baseline_frames += 1;
            let count = self.baseline_frames as f64;
            for index in 0..SPECTRUM_BINS {
                let observed = f64::from(power[index].max(POWER_FLOOR));
                self.baseline_sum[index] += observed;
                let mean = (self.baseline_sum[index] / count).min(f64::from(f32::MAX));
                #[allow(clippy::cast_possible_truncation)]
                let mean = mean as f32;
                self.noise[index] = mean.max(POWER_FLOOR);
                self.speech_probability[index] = 0.0;
                self.smoothed_probability[index] = 0.0;
                output[index] = self.noise[index];
            }
            self.initialized = true;
            return;
        }

        if !self.initialized {
            for index in 0..SPECTRUM_BINS {
                let value = power[index].max(POWER_FLOOR);
                self.noise[index] = value;
                self.speech_probability[index] = 0.0;
                self.smoothed_probability[index] = 0.0;
                output[index] = value;
            }
            self.initialized = true;
            return;
        }

        for index in 0..SPECTRUM_BINS {
            let observed = power[index].max(POWER_FLOOR);
            let previous_noise = self.noise[index].max(POWER_FLOOR);
            let gamma = observed / previous_noise;
            let fixed_prior_snr = 31.622_776_f32;
            let probability =
                (1.0 + (1.0 + fixed_prior_snr) * (-gamma * fixed_prior_snr / (1.0 + fixed_prior_snr)).exp()).recip();
            let probability = probability.clamp(0.0, 1.0);
            self.smoothed_probability[index] = 0.9 * self.smoothed_probability[index] + 0.1 * probability;
            let probability = if self.smoothed_probability[index] > 0.99 && probability > 0.99 {
                probability.min(0.99)
            } else {
                probability
            };
            self.speech_probability[index] = probability;
            let conditional_noise = (1.0 - probability) * observed + probability * previous_noise;
            self.noise[index] = (0.8 * previous_noise + 0.2 * conditional_noise).max(POWER_FLOOR);
            output[index] = self.noise[index];
        }
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

struct TonalState {
    previous_power: [f32; SPECTRUM_BINS],
    gain: [f32; SPECTRUM_BINS],
    initialized: bool,
}

impl TonalState {
    fn new() -> Self {
        Self { previous_power: [POWER_FLOOR; SPECTRUM_BINS], gain: [1.0; SPECTRUM_BINS], initialized: false }
    }

    fn process(&mut self, power: &[f32; SPECTRUM_BINS], spectrum: &mut [Complex<f32>], apply_gain: bool) {
        if !self.initialized {
            for (previous, observed) in self.previous_power.iter_mut().zip(power) {
                *previous = observed.max(POWER_FLOOR);
            }
            self.initialized = true;
            return;
        }

        let mut target_gain = [1.0_f32; SPECTRUM_BINS];
        for index in 64..SPECTRUM_BINS - 2 {
            let center = power[index].max(POWER_FLOOR);
            let mut is_peak = true;
            for (neighbor_index, neighbor_power) in power.iter().enumerate().take(index + 3).skip(index - 2) {
                if neighbor_index != index {
                    let neighbor = neighbor_power.max(POWER_FLOOR);
                    is_peak &= neighbor <= center;
                }
            }
            if !is_peak {
                continue;
            }

            let mut background_power = 0.0_f32;
            let mut background_count = 0.0_f32;
            for (candidate, candidate_power) in
                power.iter().enumerate().take((index + 6).min(SPECTRUM_BINS - 1) + 1).skip(index.saturating_sub(6))
            {
                if index.abs_diff(candidate) <= 2 {
                    continue;
                }
                background_power += candidate_power.max(POWER_FLOOR);
                background_count += 1.0;
            }
            let background = background_power / background_count;
            let prominence_db = 10.0 * (center / background.max(POWER_FLOOR)).log10();
            let tonal_score = normalize_score(prominence_db, 5.0, 14.0);
            if tonal_score <= 0.0 {
                continue;
            }

            let flux_db =
                (10.0 * (center.max(POWER_FLOOR) / self.previous_power[index].max(POWER_FLOOR)).log10()).max(0.0);
            let flux_score = normalize_score(flux_db, 3.0, 18.0);
            let start = index.saturating_sub(6);
            let end = (index + 6).min(SPECTRUM_BINS - 1);
            let mut previous_peak = index;
            let mut previous_peak_power = 0.0_f32;
            for candidate in start..=end {
                let candidate_power = self.previous_power[candidate].max(POWER_FLOOR);
                if candidate_power > previous_peak_power {
                    previous_peak = candidate;
                    previous_peak_power = candidate_power;
                }
            }
            let movement_score = if previous_peak_power < center * 0.1 {
                0.0
            } else {
                normalize_score(
                    f32::from(
                        u8::try_from(index.abs_diff(previous_peak)).expect("movement search spans at most six bins"),
                    ),
                    1.0,
                    4.0,
                )
            };
            let harmonic = Self::harmonic_support(power, index, center);
            let persistent_score = 0.35 * tonal_score;
            let temporal_score = flux_score.max(movement_score).max(persistent_score);
            let score = (tonal_score * temporal_score * (1.0 - harmonic)).clamp(0.0, 1.0);
            let center_gain = (1.0 - 0.5 * score).max(0.5);
            for (candidate, target) in target_gain.iter_mut().enumerate().take(index + 3).skip(index - 2) {
                let distance = index.abs_diff(candidate);
                let weight = f32::from(u8::try_from(3 - distance).expect("spread radius is two bins")) / 3.0;
                let neighbor_gain = 1.0 - (1.0 - center_gain) * weight;
                *target = target.min(neighbor_gain);
            }
        }

        for index in 0..SPECTRUM_BINS {
            let previous = self.gain[index];
            let target = target_gain[index];
            self.gain[index] =
                if target < previous { 0.3 * previous + 0.7 * target } else { 0.85 * previous + 0.15 * target }
                    .clamp(0.5, 1.0);
            if apply_gain {
                spectrum[index] *= self.gain[index];
            }
            self.previous_power[index] = power[index].max(POWER_FLOOR);
        }
    }

    fn harmonic_support(power: &[f32; SPECTRUM_BINS], index: usize, center: f32) -> f32 {
        if center <= POWER_FLOOR {
            return 0.0;
        }
        let targets = [
            index / 2 + usize::from(2 * (index % 2) >= 2),
            index / 3 + usize::from(2 * (index % 3) >= 3),
            index / 4 + usize::from(2 * (index % 4) >= 4),
            index * 2,
            index * 3,
            index * 4,
        ];
        let mut seen = [usize::MAX; 6];
        let mut seen_count = 0;
        let mut support = 0.0_f32;
        for target in targets {
            if target == 0 || target >= SPECTRUM_BINS || seen[..seen_count].contains(&target) {
                continue;
            }
            seen[seen_count] = target;
            seen_count += 1;
            let start = target.saturating_sub(1);
            let end = (target + 1).min(SPECTRUM_BINS - 1);
            let related = power[start..=end].iter().copied().fold(POWER_FLOOR, f32::max);
            support = support.max((related / (0.15 * center)).clamp(0.0, 1.0));
        }
        support
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

fn normalize_score(value: f32, start: f32, full: f32) -> f32 {
    if value <= start {
        0.0
    } else if value >= full {
        1.0
    } else {
        (value - start) / (full - start)
    }
}

fn exp_integral_e1(value: f64) -> f64 {
    const EULER_GAMMA: f64 = 0.577_215_664_901_532_9;
    const EPSILON: f64 = 1e-14;
    const FP_MIN: f64 = 1e-300;
    const MAX_ITERATIONS: usize = 100;

    if value <= 0.0 {
        return f64::INFINITY;
    }
    if value <= 1.0 {
        let mut result = -value.ln() - EULER_GAMMA;
        let mut factor = 1.0;
        for index in 1..=MAX_ITERATIONS {
            #[allow(clippy::cast_precision_loss)]
            let iteration = index as f64;
            factor *= -value / iteration;
            let delta = -factor / iteration;
            result += delta;
            if delta.abs() < result.abs() * EPSILON {
                break;
            }
        }
        return result;
    }

    let mut continued_denominator = value + 1.0;
    let mut continued_numerator = 1.0 / FP_MIN;
    let mut fraction_denominator = 1.0 / continued_denominator;
    let mut fraction = fraction_denominator;
    for index in 1..=MAX_ITERATIONS {
        #[allow(clippy::cast_precision_loss)]
        let iteration = index as f64;
        let numerator = -(iteration * iteration);
        continued_denominator += 2.0;
        fraction_denominator = numerator * fraction_denominator + continued_denominator;
        if fraction_denominator.abs() < FP_MIN {
            fraction_denominator = FP_MIN;
        }
        continued_numerator = continued_denominator + numerator / continued_numerator;
        if continued_numerator.abs() < FP_MIN {
            continued_numerator = FP_MIN;
        }
        fraction_denominator = 1.0 / fraction_denominator;
        let delta = continued_numerator * fraction_denominator;
        fraction *= delta;
        if (delta - 1.0).abs() < EPSILON {
            break;
        }
    }
    fraction * (-value).exp()
}

#[derive(Debug, Clone, Copy)]
struct HighPass {
    previous_input: f32,
    previous_output: f32,
    coefficient: f32,
}

impl HighPass {
    fn new() -> Self {
        let cutoff_hz = 80.0_f32;
        let coefficient = (-2.0 * PI * cutoff_hz / 8_000.0).exp();
        Self { previous_input: 0.0, previous_output: 0.0, coefficient }
    }

    fn apply(&mut self, input: f32) -> f32 {
        let output = self.coefficient * (self.previous_output + input - self.previous_input);
        self.previous_input = input;
        self.previous_output = output;
        output
    }

    fn reset(&mut self) {
        self.previous_input = 0.0;
        self.previous_output = 0.0;
    }
}

fn frames_ready(pending_samples: usize) -> usize {
    if pending_samples < FRAME_SIZE { 0 } else { (pending_samples - FRAME_SIZE) / HOP_SIZE + 1 }
}

fn hann(index: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let phase = 2.0 * PI * index as f32 / 255.0;
    0.5 - 0.5 * phase.cos()
}

fn decode_mulaw(byte: u8) -> i16 {
    let value = !byte;
    let sign = value & 0x80 != 0;
    let exponent = i32::from((value >> 4) & 0x07);
    let mantissa = i32::from(value & 0x0f);
    let magnitude = ((mantissa << 3) + 0x84) << exponent;
    let sample = magnitude - 0x84;
    let signed_sample = if sign { -sample } else { sample };
    i16::try_from(signed_sample).expect("μ-law codewords decode within the i16 range")
}

fn encode_mulaw(sample: i16) -> u8 {
    let sample = i32::from(sample);
    let sign = if sample < 0 { 0x80_u8 } else { 0 };
    let magnitude = sample.abs().min(32_635) + 0x84;
    let mut exponent = 0_u8;
    while exponent < 7 && magnitude > (0xff_i32 << exponent) {
        exponent += 1;
    }
    let mantissa =
        u8::try_from((magnitude >> (u32::from(exponent) + 3)) & 0x0f).expect("masked μ-law mantissa fits in u8");
    !(sign | (exponent << 4) | mantissa)
}

#[allow(clippy::cast_possible_truncation)]
fn quantize(sample: f32) -> i16 {
    sample.round().clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: tests::CountingAllocator = tests::CountingAllocator;

#[cfg(test)]
mod tests {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
        sync::{
            Arc, Barrier,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    };

    use super::{
        HOP_SIZE, MAX_DRAIN_OUTPUT_BYTES, MAX_PACKET_OUTPUT_BYTES, MINIMUM_NOISE_WINDOW, NoiseEstimator,
        NoiseEstimatorState, PACKET_BYTES, Processor, ProcessorError, decode_mulaw, encode_mulaw, frames_ready,
        quantize,
    };

    thread_local! {
        static TRACK_THIS_THREAD: Cell<bool> = const { Cell::new(false) };
    }

    pub struct CountingAllocator;

    static ALLOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);

    // Test-only counting; production packet processing uses no allocator wrapper.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if TRACK_THIS_THREAD.try_with(Cell::get).unwrap_or(false) {
                ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) }
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            if TRACK_THIS_THREAD.try_with(Cell::get).unwrap_or(false) {
                ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
            }
            unsafe { System.realloc(pointer, layout, new_size) }
        }
    }

    fn collect(processor: &mut Processor, packets: &[[u8; PACKET_BYTES]]) -> Vec<u8> {
        let mut collected = Vec::new();
        let mut output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
        for packet in packets {
            let written = processor.push_packet(packet, &mut output).unwrap();
            collected.extend_from_slice(&output[..written]);
        }
        let mut tail = [0_u8; MAX_DRAIN_OUTPUT_BYTES];
        let written = processor.drain(&mut tail).unwrap();
        collected.extend_from_slice(&tail[..written]);
        collected
    }

    #[test]
    fn codec_preserves_decoded_values_for_all_codewords() {
        for byte in u8::MIN..=u8::MAX {
            let decoded = decode_mulaw(byte);
            let encoded = encode_mulaw(decoded);
            let decoded_again = decode_mulaw(encoded);
            assert!((i32::from(decoded) - i32::from(decoded_again)).abs() <= 8);
        }
    }

    #[test]
    fn default_intro_is_forty_thousand_samples_and_contains_311_training_windows() {
        let processor = Processor::new();
        assert_eq!(processor.calibration_samples, 40_000);
        let full_windows = (processor.calibration_samples - 256) / 128 + 1;
        assert_eq!(full_windows, 311);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn default_calibration_excludes_a_window_crossing_the_intro_boundary() {
        let mut processor = Processor::new();
        let packet = [0xff; PACKET_BYTES];
        let mut output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
        for _ in 0..250 {
            processor.push_packet(&packet, &mut output).unwrap();
        }
        assert_eq!(processor.accepted_samples, 40_000);
        assert!(!processor.snr_initialized);
        match &processor.noise_estimator {
            NoiseEstimatorState::MinimumNoise(state) => assert_eq!(state.cursor, 311 % MINIMUM_NOISE_WINDOW),
            NoiseEstimatorState::Mcra(state) => assert_eq!(state.baseline_frames, 311),
            NoiseEstimatorState::SppMmse(state) => assert_eq!(state.baseline_frames, 311),
        }

        processor.push_packet(&packet, &mut output).unwrap();
        assert!(processor.snr_initialized);
        match &processor.noise_estimator {
            NoiseEstimatorState::MinimumNoise(state) => assert_eq!(state.cursor, 312 % MINIMUM_NOISE_WINDOW),
            NoiseEstimatorState::Mcra(state) => assert_eq!(state.baseline_frames, 311),
            NoiseEstimatorState::SppMmse(state) => assert_eq!(state.baseline_frames, 311),
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn configured_calibration_trains_only_on_a_complete_window() {
        let mut packets = vec![[0_u8; PACKET_BYTES]; 4];
        for (packet_index, packet) in packets.iter_mut().enumerate() {
            for (offset, byte) in packet.iter_mut().enumerate() {
                let sample_index = packet_index * PACKET_BYTES + offset;
                let amplitude = if sample_index < 256 { 4_000.0 } else { 400.0 };
                let phase = 2.0 * std::f32::consts::PI * 1_000.0 * sample_index as f32 / 8_000.0;
                *byte = encode_mulaw(quantize(amplitude * phase.sin()));
            }
        }

        let first_output_window = |duration| {
            let mut processor = Processor::with_quiet_intro(duration).unwrap();
            let mut output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
            let mut first_window = [0_u8; HOP_SIZE];
            let mut captured = false;
            for packet in &packets {
                let written = processor.push_packet(packet, &mut output).unwrap();
                if written > 0 && !captured {
                    first_window.copy_from_slice(&output[..HOP_SIZE]);
                    captured = true;
                }
            }
            assert!(captured);
            first_window
        };
        let partial_window = first_output_window(Duration::from_millis(31));
        let complete_window = first_output_window(Duration::from_millis(32));
        let energy = |samples: &[u8]| {
            samples
                .iter()
                .map(|byte| {
                    let sample = i64::from(decode_mulaw(*byte));
                    sample * sample
                })
                .sum::<i64>()
        };
        assert!(energy(&complete_window) > energy(&partial_window) * 2);
    }

    #[test]
    fn every_estimator_uses_floored_quiet_intro_and_the_first_crossing_frame() {
        let estimators = [NoiseEstimator::MinimumNoise, NoiseEstimator::Mcra, NoiseEstimator::SppMmse];
        let packet = [0x00; PACKET_BYTES];
        for estimator in estimators {
            let mut short = Processor::with_estimator_and_quiet_intro(estimator, Duration::from_millis(31)).unwrap();
            let mut exact = Processor::with_estimator_and_quiet_intro(estimator, Duration::from_millis(32)).unwrap();
            assert_eq!(short.calibration_samples, 248);
            assert_eq!(exact.calibration_samples, 256);
            let mut output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
            for _ in 0..2 {
                short.push_packet(&packet, &mut output).unwrap();
                exact.push_packet(&packet, &mut output).unwrap();
            }
            assert!(short.snr_initialized);
            assert!(!exact.snr_initialized);
            assert!(short.tonal.initialized);
            assert!(exact.tonal.initialized);
            match (&short.noise_estimator, &exact.noise_estimator) {
                (NoiseEstimatorState::MinimumNoise(_), NoiseEstimatorState::MinimumNoise(_)) => {}
                (NoiseEstimatorState::Mcra(short_state), NoiseEstimatorState::Mcra(exact_state)) => {
                    assert_eq!(short_state.baseline_frames, 0);
                    assert_eq!(exact_state.baseline_frames, 1);
                }
                (NoiseEstimatorState::SppMmse(short_state), NoiseEstimatorState::SppMmse(exact_state)) => {
                    assert_eq!(short_state.baseline_frames, 0);
                    assert_eq!(exact_state.baseline_frames, 1);
                }
                _ => panic!("the selected estimator changed during the call"),
            }
            short.push_packet(&packet, &mut output).unwrap();
            exact.push_packet(&packet, &mut output).unwrap();
            assert!(short.snr_initialized);
            assert!(exact.snr_initialized);
        }
        let floored =
            Processor::with_estimator_and_quiet_intro(NoiseEstimator::Mcra, Duration::new(0, 31_999_999)).unwrap();
        assert_eq!(floored.calibration_samples, 255);
    }

    #[test]
    fn a_padded_drain_frame_inside_the_intro_does_not_train_any_estimator() {
        for estimator in [NoiseEstimator::MinimumNoise, NoiseEstimator::Mcra, NoiseEstimator::SppMmse] {
            let mut processor = Processor::with_estimator_and_quiet_intro(estimator, Duration::from_secs(1)).unwrap();
            let packet = [0x00; PACKET_BYTES];
            let mut output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
            assert_eq!(processor.push_packet(&packet, &mut output).unwrap(), 0);
            let mut tail = [0_u8; MAX_DRAIN_OUTPUT_BYTES];
            assert_eq!(processor.drain(&mut tail).unwrap(), PACKET_BYTES);
            match &processor.noise_estimator {
                NoiseEstimatorState::MinimumNoise(state) => assert!(!state.initialized),
                NoiseEstimatorState::Mcra(state) => {
                    assert_eq!(state.baseline_frames, 0);
                    assert!(!state.initialized);
                }
                NoiseEstimatorState::SppMmse(state) => {
                    assert_eq!(state.baseline_frames, 0);
                    assert!(!state.initialized);
                }
            }
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn selectable_estimators_produce_finite_bounded_state_and_keep_their_choice() {
        let estimators = [NoiseEstimator::MinimumNoise, NoiseEstimator::Mcra, NoiseEstimator::SppMmse];
        let packets = (0..24)
            .map(|packet_index| {
                std::array::from_fn(|offset| {
                    let sample_index = packet_index * PACKET_BYTES + offset;
                    let phase = 2.0 * std::f32::consts::PI * 1_375.0 * sample_index as f32 / 8_000.0;
                    encode_mulaw(quantize(3_000.0 * phase.sin()))
                })
            })
            .collect::<Vec<_>>();
        for estimator in estimators {
            let mut processor =
                Processor::with_estimator_and_quiet_intro(estimator, Duration::from_millis(32)).unwrap();
            let output = collect(&mut processor, &packets);
            assert_eq!(output.len(), packets.len() * PACKET_BYTES);
            assert!(processor.noise_psd.iter().all(|value| value.is_finite() && *value >= 0.0 && *value < 1e20));
            assert!(processor.previous_clean_snr.iter().all(|value| value.is_finite() && *value >= 0.0));
            match (&processor.noise_estimator, estimator) {
                (NoiseEstimatorState::MinimumNoise(_), NoiseEstimator::MinimumNoise)
                | (NoiseEstimatorState::Mcra(_), NoiseEstimator::Mcra)
                | (NoiseEstimatorState::SppMmse(_), NoiseEstimator::SppMmse) => {}
                _ => panic!("the processor did not retain its selected estimator"),
            }
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn tonal_detector_reduces_a_narrow_peak_from_original_frame_power() {
        let packets = (0..16)
            .map(|packet_index| {
                std::array::from_fn(|offset| {
                    let sample_index = packet_index * PACKET_BYTES + offset;
                    let phase = 2.0 * std::f32::consts::PI * 3_000.0 * sample_index as f32 / 8_000.0;
                    encode_mulaw(quantize(4_000.0 * phase.sin()))
                })
            })
            .collect::<Vec<_>>();
        let mut processor = Processor::with_quiet_intro(Duration::from_millis(32)).unwrap();
        let output = collect(&mut processor, &packets);
        assert_eq!(output.len(), packets.len() * PACKET_BYTES);
        assert!((0.5..1.0).contains(&processor.tonal.gain[96]));
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn quiet_intro_seeds_tonal_history_without_enhancing_it_before_the_first_crossing() {
        let packets = (0..5)
            .map(|packet_index| {
                std::array::from_fn(|offset| {
                    let sample_index = packet_index * PACKET_BYTES + offset;
                    let amplitude = if sample_index < 512 { 500.0 } else { 8_000.0 };
                    let phase = 2.0 * std::f32::consts::PI * 3_000.0 * sample_index as f32 / 8_000.0;
                    encode_mulaw(quantize(amplitude * phase.sin()))
                })
            })
            .collect::<Vec<_>>();
        let input = packets.iter().flat_map(|packet| packet.iter().copied()).collect::<Vec<_>>();
        let energy = |samples: &[u8]| {
            samples
                .iter()
                .map(|byte| {
                    let sample = i64::from(decode_mulaw(*byte));
                    sample * sample
                })
                .sum::<i64>()
        };

        for estimator in [NoiseEstimator::MinimumNoise, NoiseEstimator::Mcra, NoiseEstimator::SppMmse] {
            let mut processor =
                Processor::with_estimator_and_quiet_intro(estimator, Duration::from_millis(64)).unwrap();
            let mut output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
            let mut calibration_output = Vec::new();
            for packet in &packets[..3] {
                let written = processor.push_packet(packet, &mut output).unwrap();
                calibration_output.extend_from_slice(&output[..written]);
            }
            assert_eq!(calibration_output.len(), HOP_SIZE * 2);
            assert!(energy(&calibration_output) * 10 > energy(&input[..HOP_SIZE * 2]) * 9);
            assert!(processor.tonal.gain[96] < 1.0);

            let written = processor.push_packet(&packets[3], &mut output).unwrap();
            assert_eq!(written, HOP_SIZE * 2);
            assert!(energy(&output[..HOP_SIZE]) * 10 > energy(&input[HOP_SIZE * 2..HOP_SIZE * 3]) * 9);
            assert!(energy(&output[HOP_SIZE..HOP_SIZE * 2]) * 4 < energy(&input[HOP_SIZE * 3..HOP_SIZE * 4]) * 3);
            assert!(processor.tonal.gain[96] < 0.7);

            let written = processor.push_packet(&packets[4], &mut output).unwrap();
            assert!(written >= HOP_SIZE);
            assert!(energy(&output[..HOP_SIZE]) * 4 < energy(&input[512..640]) * 3);
        }
    }

    #[test]
    fn output_requirements_are_bounded_and_capacity_rejection_is_retryable() {
        assert_eq!(frames_ready(415) * 128, MAX_PACKET_OUTPUT_BYTES);
        let packet = [0xff; PACKET_BYTES];
        let mut rejected = Processor::new();
        let mut output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
        assert_eq!(rejected.push_packet(&packet, &mut output).unwrap(), 0);

        let error = rejected.push_packet(&packet, &mut output[..127]).unwrap_err();
        assert_eq!(error, ProcessorError::OutputTooSmall { required: 128 });
        let retried = rejected.push_packet(&packet, &mut output).unwrap();
        let retried_output = output[..retried].to_vec();

        let mut uninterrupted = Processor::new();
        uninterrupted.push_packet(&packet, &mut output).unwrap();
        let expected = uninterrupted.push_packet(&packet, &mut output).unwrap();
        assert_eq!(retried, expected);
        assert_eq!(retried_output.as_slice(), &output[..expected]);
    }

    #[test]
    fn stream_cardinality_is_exact_for_each_packet_phase() {
        let packet = [0x7f; PACKET_BYTES];
        let expected_push_sizes = [0, 128, 128, 256, 128, 128, 128, 256];
        for packet_count in 1..=8 {
            let mut processor = Processor::with_quiet_intro(Duration::ZERO).unwrap();
            let mut output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
            let mut push_sizes = Vec::with_capacity(packet_count);
            for _ in 0..packet_count {
                push_sizes.push(processor.push_packet(&packet, &mut output).unwrap());
            }
            assert_eq!(push_sizes.as_slice(), &expected_push_sizes[..packet_count]);

            let mut tail = [0_u8; MAX_DRAIN_OUTPUT_BYTES];
            let drained = processor.drain(&mut tail).unwrap();
            let pushed = push_sizes.iter().sum::<usize>();
            assert_eq!(drained, packet_count * PACKET_BYTES - pushed);
            assert_eq!(pushed + drained, PACKET_BYTES * packet_count);
        }
    }

    #[test]
    fn push_and_drain_output_remain_in_sample_order() {
        let packets = [[0xff; PACKET_BYTES], [0xff; PACKET_BYTES], [0xff; PACKET_BYTES], [0x00; PACKET_BYTES]];
        let mut processor = Processor::with_quiet_intro(Duration::ZERO).unwrap();
        let mut output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
        let mut ordered = Vec::with_capacity(packets.len() * PACKET_BYTES);
        let mut push_sizes = Vec::with_capacity(packets.len());
        for packet in &packets {
            let written = processor.push_packet(packet, &mut output).unwrap();
            push_sizes.push(written);
            ordered.extend_from_slice(&output[..written]);
        }

        let mut tail = [0_u8; MAX_DRAIN_OUTPUT_BYTES];
        let drained = processor.drain(&mut tail).unwrap();
        ordered.extend_from_slice(&tail[..drained]);
        assert_eq!(push_sizes.as_slice(), &[0, 128, 128, 256]);
        assert_eq!(drained, 128);
        assert_eq!(ordered.len(), packets.len() * PACKET_BYTES);
        assert!(ordered[..480].iter().all(|byte| *byte == 0xff));
        assert_ne!(ordered[480], 0xff);
        assert!(ordered[512..].iter().any(|byte| *byte != 0xff));
        assert_eq!(processor.drain(&mut tail).unwrap(), 0);
        assert_eq!(processor.push_packet(&packets[0], &mut output), Err(ProcessorError::StreamDrained));
    }

    #[test]
    fn drain_capacity_failure_does_not_close_or_consume_the_tail() {
        let packet = [0xff; PACKET_BYTES];
        let mut processor = Processor::with_quiet_intro(Duration::ZERO).unwrap();
        let mut output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
        processor.push_packet(&packet, &mut output).unwrap();
        let error = processor.drain(&mut [0_u8; 159]).unwrap_err();
        assert_eq!(error, ProcessorError::OutputTooSmall { required: 160 });
        let mut tail = [0_u8; MAX_DRAIN_OUTPUT_BYTES];
        assert_eq!(processor.drain(&mut tail).unwrap(), 160);
        assert_eq!(processor.drain(&mut tail).unwrap(), 0);
        assert_eq!(processor.push_packet(&packet, &mut output), Err(ProcessorError::StreamDrained));
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn reset_discards_pending_samples_and_restarts_each_selected_stream() {
        let tone_packets = |frequency_hz: f32, later_amplitude: f32, packet_count: usize| {
            (0..packet_count)
                .map(|packet_index| {
                    std::array::from_fn(|offset| {
                        let sample_index = packet_index * PACKET_BYTES + offset;
                        let amplitude = if sample_index < 256 { 8_000.0 } else { later_amplitude };
                        let phase = 2.0 * std::f32::consts::PI * frequency_hz * sample_index as f32 / 8_000.0;
                        encode_mulaw(quantize(amplitude * phase.sin()))
                    })
                })
                .collect::<Vec<_>>()
        };
        let calibration = Duration::from_millis(32);
        for estimator in [NoiseEstimator::MinimumNoise, NoiseEstimator::Mcra, NoiseEstimator::SppMmse] {
            let mut processor = Processor::with_estimator_and_quiet_intro(estimator, calibration).unwrap();
            let mut output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
            for packet in tone_packets(1_000.0, 8_000.0, 4) {
                processor.push_packet(&packet, &mut output).unwrap();
            }
            assert!(processor.snr_initialized);
            assert!(processor.high_pass.previous_output != 0.0);
            assert!(processor.overlap.iter().any(|sample| *sample != 0.0));
            assert!(processor.accepted_samples > processor.emitted_samples);

            let new_stream = tone_packets(1_500.0, 100.0, 16);
            processor.reset();
            assert_eq!(processor.calibration_samples, calibration.as_nanos() * 8_000 / 1_000_000_000);
            let after_reset = collect(&mut processor, &new_stream);

            let mut fresh = Processor::with_estimator_and_quiet_intro(estimator, calibration).unwrap();
            let fresh_output = collect(&mut fresh, &new_stream);
            assert_eq!(after_reset, fresh_output);
            assert_eq!(after_reset.len(), new_stream.len() * PACKET_BYTES);
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn default_log_mmse_reduces_quiet_noise_and_keeps_an_above_floor_tone() {
        let sample_count = 52_000;
        let mut packets = Vec::with_capacity(sample_count / PACKET_BYTES);
        for packet_index in 0..sample_count / PACKET_BYTES {
            let mut packet = [0_u8; PACKET_BYTES];
            for (offset, byte) in packet.iter_mut().enumerate() {
                let sample_index = packet_index * PACKET_BYTES + offset;
                let background_amplitude = if sample_index < 40_000 { 2_000.0 } else { 150.0 };
                let background =
                    background_amplitude * (2.0 * std::f32::consts::PI * 1_000.0 * sample_index as f32 / 8_000.0).sin();
                let tone = if sample_index >= 48_000 {
                    6_000.0 * (2.0 * std::f32::consts::PI * 1_500.0 * (sample_index - 48_000) as f32 / 8_000.0).sin()
                } else {
                    0.0
                };
                *byte = encode_mulaw(quantize(background + tone));
            }
            packets.push(packet);
        }

        let mut processor = Processor::new();
        let enhanced = collect(&mut processor, &packets);
        assert_eq!(enhanced.len(), sample_count);
        let noise =
            enhanced[42_000..47_000].iter().map(|byte| f32::from(decode_mulaw(*byte)).powi(2)).sum::<f32>() / 5_000.0;
        let tone =
            enhanced[49_000..51_000].iter().map(|byte| f32::from(decode_mulaw(*byte)).powi(2)).sum::<f32>() / 2_000.0;
        assert!(noise.sqrt() < 80.0);
        assert!(tone.sqrt() > 2_000.0);
    }

    #[test]
    fn initialized_push_and_drain_do_not_allocate() {
        let packet = [0xff; PACKET_BYTES];
        for estimator in [NoiseEstimator::MinimumNoise, NoiseEstimator::Mcra, NoiseEstimator::SppMmse] {
            let mut processor = Processor::with_estimator_and_quiet_intro(estimator, Duration::ZERO).unwrap();
            let mut output = [0_u8; MAX_PACKET_OUTPUT_BYTES];
            let mut tail = [0_u8; MAX_DRAIN_OUTPUT_BYTES];
            ALLOCATION_COUNT.store(0, Ordering::Relaxed);
            TRACK_THIS_THREAD.with(|enabled| enabled.set(true));
            for _ in 0..4 {
                processor.push_packet(&packet, &mut output).unwrap();
            }
            processor.drain(&mut tail).unwrap();
            TRACK_THIS_THREAD.with(|enabled| enabled.set(false));
            assert_eq!(ALLOCATION_COUNT.load(Ordering::Relaxed), 0);
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn make_noise_packets() -> Vec<[u8; PACKET_BYTES]> {
        let mut packets = vec![[0_u8; PACKET_BYTES]; 32];
        for (index, byte) in packets.iter_mut().flatten().enumerate() {
            let amplitude = if index < 256 { 2_000.0 } else { 100.0 };
            let sample = amplitude * (2.0 * std::f32::consts::PI * 1_000.0 * index as f32 / 8_000.0).sin();
            *byte = encode_mulaw(quantize(sample));
        }
        packets
    }

    #[test]
    fn independent_processors_keep_calibration_state_local_under_concurrency() {
        let packets = Arc::new(make_noise_packets());
        let barrier = Arc::new(Barrier::new(2));
        let left_barrier = Arc::clone(&barrier);
        let right_barrier = Arc::clone(&barrier);
        let left_packets = Arc::clone(&packets);
        let right_packets = Arc::clone(&packets);
        let left = thread::spawn(move || {
            let mut processor = Processor::with_quiet_intro(Duration::ZERO).unwrap();
            let mut output = Vec::new();
            let mut scratch = [0_u8; MAX_PACKET_OUTPUT_BYTES];
            left_barrier.wait();
            for packet in left_packets.iter() {
                let written = processor.push_packet(packet, &mut scratch).unwrap();
                output.extend_from_slice(&scratch[..written]);
            }
            let mut tail = [0_u8; MAX_DRAIN_OUTPUT_BYTES];
            let written = processor.drain(&mut tail).unwrap();
            output.extend_from_slice(&tail[..written]);
            output
        });
        let right = thread::spawn(move || {
            let mut processor = Processor::with_quiet_intro(Duration::from_millis(32)).unwrap();
            let mut output = Vec::new();
            let mut scratch = [0_u8; MAX_PACKET_OUTPUT_BYTES];
            right_barrier.wait();
            for packet in right_packets.iter() {
                let written = processor.push_packet(packet, &mut scratch).unwrap();
                output.extend_from_slice(&scratch[..written]);
            }
            let mut tail = [0_u8; MAX_DRAIN_OUTPUT_BYTES];
            let written = processor.drain(&mut tail).unwrap();
            output.extend_from_slice(&tail[..written]);
            output
        });
        let left_output = left.join().unwrap();
        let right_output = right.join().unwrap();
        assert_eq!(left_output.len(), right_output.len());
        assert_ne!(left_output, right_output);
    }
}
