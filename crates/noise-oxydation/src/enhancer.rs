//! The per-call composition root: packet timing, calibration clock, and the stage order of one call.

use crate::{
    analysis::Analyzer,
    config::{CallConfig, ValidConfig},
    error::{ConfigError, StreamError},
    fft::{Complex, Fft},
    geometry::{BINS, FFT_SIZE, HOP_SIZE, PACKET_SAMPLES},
    highpass::HighPass,
    log_mmse::LogMmse,
    mulaw,
    output_queue::OutputQueue,
    spp_mmse::SppMmse,
    synthesis::Synthesizer,
    window::HannWindow,
};

/// One 20 ms packet of 160 G.711 μ-law bytes.
pub type Packet = [u8; PACKET_SAMPLES];

/// Packets withheld by the fixed algorithmic delay; also the most packets [`CallEnhancer::drain`] returns.
pub const DELAY_PACKETS: usize = 2;

/// Result of [`CallEnhancer::process_packet`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PacketOutcome {
    /// One of the first two packets of a call: nothing was written to the output packet.
    Priming,
    /// The output packet now holds the enhanced audio of the input packet received two packets earlier.
    Emitted,
}

/// Lifecycle phase of a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallPhase {
    /// Frames pass through un-enhanced while the noise estimator learns the quiet intro.
    Calibrating,
    /// Frames are enhanced.
    Enhancing,
    /// The call has been drained; only [`CallEnhancer::reset`] is accepted.
    Drained,
}

/// Enhancer for one call: consumes 160-byte μ-law packets and returns enhanced packets after a two-packet delay.
///
/// Each call needs its own instance; instances share nothing, so independent calls run in parallel on separate
/// threads. All storage is sized at construction: [`process_packet`](Self::process_packet), [`drain`](Self::drain)
/// and [`reset`](Self::reset) never allocate, lock, block, perform I/O or log.
#[derive(Debug)]
pub struct CallEnhancer {
    config: ValidConfig,
    window: HannWindow,
    fft: Fft,
    high_pass: HighPass,
    analyzer: Analyzer,
    noise: SppMmse,
    suppressor: LogMmse,
    synthesizer: Synthesizer,
    output: OutputQueue,
    spectrum: [Complex; BINS],
    power: [f32; BINS],
    frame_output: [f32; HOP_SIZE],
    frames: u64,
    packets_in: u64,
    phase: CallPhase,
}

impl CallEnhancer {
    /// Validates `config` and initializes every piece of per-call state, all of it in fixed-size storage.
    ///
    /// With the default `log` feature this emits one debug record describing the validated configuration.
    ///
    /// # Errors
    ///
    /// Returns a [`ConfigError`] naming the first invalid field and the constraint it violates.
    pub fn new(config: &CallConfig) -> Result<Self, ConfigError> {
        let config = ValidConfig::new(config)?;
        #[cfg(feature = "log")]
        log::debug!(
            "call enhancer created: {} calibration samples, configuration {:?}",
            config.calibration_samples,
            config.config
        );
        let settings = &config.config;
        let mut enhancer = Self {
            window: HannWindow::new(),
            fft: Fft::new(),
            high_pass: HighPass::new(settings.high_pass.cutoff_hz),
            analyzer: Analyzer::new(),
            noise: SppMmse::new(settings.spp_mmse),
            suppressor: LogMmse::new(settings.log_mmse, settings.decision_directed),
            synthesizer: Synthesizer::new(),
            output: OutputQueue::new(),
            spectrum: [Complex::ZERO; BINS],
            power: [0.0; BINS],
            frame_output: [0.0; HOP_SIZE],
            frames: 0,
            packets_in: 0,
            phase: CallPhase::Calibrating,
            config,
        };
        enhancer.phase = enhancer.initial_phase();
        Ok(enhancer)
    }

    /// The validated configuration of this enhancer.
    #[must_use]
    pub fn config(&self) -> &CallConfig {
        &self.config.config
    }

    /// The current lifecycle phase.
    #[must_use]
    pub fn phase(&self) -> CallPhase {
        self.phase
    }

    /// Consumes one input packet and, after the first two packets of a call, writes one enhanced output packet.
    ///
    /// Output packet *n* carries the enhanced samples of input packet *n − 2*. `output` is left untouched when the
    /// outcome is [`PacketOutcome::Priming`].
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::Drained`] after [`drain`](Self::drain) until [`reset`](Self::reset).
    pub fn process_packet(&mut self, input: &Packet, output: &mut Packet) -> Result<PacketOutcome, StreamError> {
        if self.phase == CallPhase::Drained {
            return Err(StreamError::Drained);
        }
        let mut samples = [0.0; PACKET_SAMPLES];
        for (sample, &byte) in samples.iter_mut().zip(input) {
            *sample = mulaw::decode_sample(byte);
        }
        self.high_pass.process(&mut samples);
        self.packets_in += 1;
        self.output.receive_packet();

        let mut pending = &samples[..];
        while !pending.is_empty() {
            let consumed = self.analyzer.push(pending);
            pending = &pending[consumed..];
            if self.analyzer.frame_ready() {
                self.analyzer.analyze(&self.window, &self.fft, &mut self.spectrum);
                self.process_frame();
            }
        }

        if self.packets_in <= DELAY_PACKETS as u64 {
            return Ok(PacketOutcome::Priming);
        }
        let emitted = self.output.pop_packet(output);
        debug_assert!(emitted, "two packets of delay always finalize one packet");
        Ok(PacketOutcome::Emitted)
    }

    /// Ends the call and writes the withheld packets to the front of `output`, returning how many were written.
    ///
    /// The buffered tail is processed with one zero-padded analysis frame plus the synthesis tail. Exactly
    /// `min(packets received, 2)` packets are returned, so the call's total output length equals its input length;
    /// samples beyond the end of the input are never emitted. A call without packets processes no frame and
    /// returns 0.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::Drained`] if the call was already drained.
    pub fn drain(&mut self, output: &mut [Packet; DELAY_PACKETS]) -> Result<usize, StreamError> {
        if self.phase == CallPhase::Drained {
            return Err(StreamError::Drained);
        }
        if self.analyzer.analyze_remainder(&self.window, &self.fft, &mut self.spectrum) {
            self.process_frame();
        }
        let mut tail = [0.0; FFT_SIZE - HOP_SIZE];
        if self.synthesizer.finish(&mut tail) {
            self.output.finalize(&tail);
        }
        self.phase = CallPhase::Drained;

        let mut written = 0;
        for packet in output.iter_mut() {
            if !self.output.pop_packet(packet) {
                break;
            }
            written += 1;
        }
        debug_assert!(self.output.is_empty(), "drain returns every withheld sample");
        Ok(written)
    }

    /// Starts a new call on this instance: discards buffered input and withheld output, clears all temporal state,
    /// and restores the initial phase. Storage is reused; nothing is allocated.
    pub fn reset(&mut self) {
        self.high_pass.reset();
        self.analyzer.reset();
        self.noise.reset();
        self.suppressor.reset();
        self.synthesizer.reset();
        self.output.reset();
        self.frames = 0;
        self.packets_in = 0;
        self.phase = self.initial_phase();
    }

    fn initial_phase(&self) -> CallPhase {
        if self.is_calibration_frame(0) { CallPhase::Calibrating } else { CallPhase::Enhancing }
    }

    /// Frame `t` covers input samples `[128t, 128t + 256)`; it is a calibration frame when it ends within the
    /// calibration duration.
    fn is_calibration_frame(&self, frame: u64) -> bool {
        frame.saturating_mul(HOP_SIZE as u64).saturating_add(FFT_SIZE as u64) <= self.config.calibration_samples
    }

    /// Runs the stages of one analyzed frame held in `self.spectrum` and queues the samples it finalizes.
    fn process_frame(&mut self) {
        for (power, bin) in self.power.iter_mut().zip(&self.spectrum) {
            *power = bin.norm_sqr();
        }
        let calibration = self.is_calibration_frame(self.frames);
        let noise = self.noise.estimate(&self.power, calibration);
        if !calibration {
            self.suppressor.apply(&mut self.spectrum, &self.power, noise);
            if self.phase == CallPhase::Calibrating {
                self.phase = CallPhase::Enhancing;
            }
        }
        self.synthesizer.synthesize(&self.window, &self.fft, &self.spectrum, &mut self.frame_output);
        self.frames += 1;
        self.output.finalize(&self.frame_output);
    }
}
