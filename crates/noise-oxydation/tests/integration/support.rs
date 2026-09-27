//! Test-side signal generation, μ-law decoding and call driving, written independently of the library internals.

use std::{f64::consts::PI, time::Duration};

use noise_oxydation::{
    CallConfig, CallEnhancer, DELAY_PACKETS, InterferenceConfig, McraConfig, MinimumConfig, NoiseEstimatorConfig,
    PACKET_SAMPLES, Packet, PacketOutcome, SppMmseConfig, TonalTransientConfig,
};

/// Deterministic xorshift64* generator.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        u32::try_from(self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32).expect("32 bits")
    }
}

/// Packets of stationary white noise: every sample has a random sign and a uniformly random μ-law magnitude index in
/// `0..=max_magnitude_index`.
pub fn noise_packets(count: usize, seed: u64, max_magnitude_index: u8) -> Vec<Packet> {
    let mut rng = Rng::new(seed);
    let span = u32::from(max_magnitude_index) + 1;
    (0..count)
        .map(|_| {
            let mut packet = [0; PACKET_SAMPLES];
            for byte in &mut packet {
                let draw = rng.next_u32();
                let magnitude = u8::try_from(draw % span).expect("magnitude index fits a byte");
                let sign = if draw >> 31 == 1 { 0x80 } else { 0 };
                *byte = !(sign | magnitude);
            }
            packet
        })
        .collect()
}

/// G.711 μ-law decoding in the 14-bit formulation of the standard, scaled to 16 bits and normalized by 32768:
/// magnitude `((2·mantissa + 33)·2^segment − 33)·4`.
pub fn decode(byte: u8) -> f64 {
    let code = !byte;
    let segment = (code >> 4) & 0x07;
    let mantissa = i32::from(code & 0x0F);
    let magnitude = ((2 * mantissa + 33) * (1 << segment) - 33) * 4;
    let value = f64::from(magnitude) / 32768.0;
    if code & 0x80 == 0 { value } else { -value }
}

/// Position of a μ-law code on the signed level scale; adjacent levels differ by one. Both zero codes map to 0.
pub fn ordinal(byte: u8) -> i32 {
    let code = !byte;
    let index = i32::from(code & 0x7F);
    if code & 0x80 == 0 { index } else { -index }
}

/// Ordinal of the μ-law level nearest to `value`.
pub fn nearest_ordinal(value: f64) -> i32 {
    let magnitude = value.abs();
    let mut best = 0;
    for index in 1..0x80_u8 {
        if (decode(!index) - magnitude).abs() < (decode(!best) - magnitude).abs() {
            best = index;
        }
    }
    let best = i32::from(best);
    if value < 0.0 { -best } else { best }
}

/// μ-law byte of the level nearest to `value`.
pub fn encode(value: f64) -> u8 {
    let level = nearest_ordinal(value);
    let magnitude = u8::try_from(level.unsigned_abs()).expect("μ-law levels fit a byte");
    let sign = if level < 0 { 0x80 } else { 0 };
    !(sign | magnitude)
}

/// Encodes a signal into packets, padding the last packet with silence.
pub fn encode_packets(signal: &[f64]) -> Vec<Packet> {
    signal
        .chunks(PACKET_SAMPLES)
        .map(|chunk| {
            let mut packet = [0xFF; PACKET_SAMPLES];
            for (byte, &value) in packet.iter_mut().zip(chunk) {
                *byte = encode(value);
            }
            packet
        })
        .collect()
}

/// Decodes packets and applies `y[n] = x[n] − x[n−1] + r·y[n−1]` with `r = exp(−2π·80/8000)` in double precision:
/// the pass-through signal the enhancer must reproduce during calibration.
pub fn decoded_high_passed(packets: &[Packet]) -> Vec<f64> {
    let r = (-2.0 * PI * 80.0 / 8000.0).exp();
    let (mut previous_input, mut previous_output) = (0.0, 0.0);
    packets
        .iter()
        .flatten()
        .map(|&byte| {
            let input = decode(byte);
            let output = input - previous_input + r * previous_output;
            previous_input = input;
            previous_output = output;
            output
        })
        .collect()
}

/// Everything a call produced: one outcome per input packet, and the concatenated output packets including drain.
pub struct CallRun {
    pub outcomes: Vec<PacketOutcome>,
    pub drained: usize,
    pub output: Vec<u8>,
}

/// Streams `packets` through `enhancer` and drains it.
pub fn run_call(enhancer: &mut CallEnhancer, packets: &[Packet]) -> CallRun {
    let mut outcomes = Vec::with_capacity(packets.len());
    let mut output = Vec::with_capacity(packets.len() * PACKET_SAMPLES);
    let mut packet_out = [0; PACKET_SAMPLES];
    for packet in packets {
        let outcome = enhancer.process_packet(packet, &mut packet_out).expect("call is not drained");
        if outcome == PacketOutcome::Emitted {
            output.extend_from_slice(&packet_out);
        }
        outcomes.push(outcome);
    }
    let mut tail = [[0; PACKET_SAMPLES]; DELAY_PACKETS];
    let drained = enhancer.drain(&mut tail).expect("first drain succeeds");
    output.extend(tail[..drained].iter().flatten());
    CallRun { outcomes, drained, output }
}

pub fn enhancer_with_calibration(duration: Duration) -> CallEnhancer {
    CallEnhancer::new(&CallConfig { calibration_duration: duration, ..CallConfig::default() }).expect("valid config")
}

/// The three noise estimators with default parameters.
pub fn estimators() -> [NoiseEstimatorConfig; 3] {
    [
        NoiseEstimatorConfig::SppMmse(SppMmseConfig::default()),
        NoiseEstimatorConfig::Mcra(McraConfig::default()),
        NoiseEstimatorConfig::Minimum(MinimumConfig::default()),
    ]
}

/// Interference suppression enabled with default parameters, and disabled.
pub fn interference_settings() -> [InterferenceConfig; 2] {
    [InterferenceConfig::TonalTransient(TonalTransientConfig::default()), InterferenceConfig::Disabled]
}

/// All six estimator and interference combinations with the given calibration duration.
pub fn all_configurations(calibration: Duration) -> Vec<CallConfig> {
    estimators()
        .into_iter()
        .flat_map(|noise_estimator| {
            interference_settings().into_iter().map(move |interference| CallConfig {
                calibration_duration: calibration,
                noise_estimator,
                interference,
                ..CallConfig::default()
            })
        })
        .collect()
}

/// Short description of a configuration's alternatives for assertion messages.
pub fn describe(config: &CallConfig) -> String {
    let estimator = match config.noise_estimator {
        NoiseEstimatorConfig::SppMmse(_) => "SPP-MMSE",
        NoiseEstimatorConfig::Mcra(_) => "MCRA",
        NoiseEstimatorConfig::Minimum(_) => "minimum",
    };
    let interference = match config.interference {
        InterferenceConfig::TonalTransient(_) => "tonal suppression",
        InterferenceConfig::Disabled => "no interference suppression",
    };
    format!("{estimator} with {interference}")
}

pub fn enhancer(config: &CallConfig) -> CallEnhancer {
    CallEnhancer::new(config).expect("valid config")
}

/// Mean energy of the decoded output bytes over `range`.
pub fn output_energy(output: &[u8], range: std::ops::Range<usize>) -> f64 {
    mean_square(output[range].iter().map(|&byte| decode(byte)))
}

/// Mean energy of `signal` over `range`.
pub fn signal_energy(signal: &[f64], range: std::ops::Range<usize>) -> f64 {
    mean_square(signal[range].iter().copied())
}

fn mean_square(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0.0), |(sum, count), value| (sum + value * value, count + 1.0));
    sum / count
}
