//! The C ABI produces exactly the output of the Rust API, and the Go tests compare with the same digests.

use std::time::Duration;

use noise_oxydation::{
    CallConfig, CallEnhancer, DELAY_PACKETS, InterferenceConfig, McraConfig, MinimumConfig, NoiseEstimatorConfig,
    PacketOutcome,
};
use noise_oxydation_capi::{
    NOX_INTERFERENCE_DISABLED, NOX_NOISE_ESTIMATOR_MCRA, NOX_NOISE_ESTIMATOR_MINIMUM, NOX_OUTCOME_EMITTED,
    NOX_STATUS_OK, NoxConfig,
};

use crate::support::{Handle, Packet, default_config, fnv1a, synthetic_packets};

/// Packets of the equality input: 8 s, calibrating for the first second.
const PACKETS: usize = 400;
/// Seed of the equality input.
const SEED: u64 = 20_260_927;
/// FNV-1a digests of the complete output of each case, which `go/noiseox_test.go` repeats.
const DIGESTS: [u64; 3] = [0x75F1_20C1_C543_C7FD, 0x8854_DF40_7A8F_2FB5, 0x9507_EAB0_E56C_8221];

/// The three cases as C and Rust configurations: SPP-MMSE with tonal suppression, MCRA without it, and the minimum
/// estimator with it, each calibrating for one second.
fn cases() -> [(NoxConfig, CallConfig); 3] {
    let calibration = Duration::from_secs(1);
    let mut spp_mmse = default_config();
    spp_mmse.calibration_duration_ns = 1_000_000_000;
    let mut mcra = spp_mmse;
    mcra.noise_estimator = NOX_NOISE_ESTIMATOR_MCRA;
    mcra.interference = NOX_INTERFERENCE_DISABLED;
    let mut minimum = spp_mmse;
    minimum.noise_estimator = NOX_NOISE_ESTIMATOR_MINIMUM;
    [
        (spp_mmse, CallConfig { calibration_duration: calibration, ..CallConfig::default() }),
        (
            mcra,
            CallConfig {
                calibration_duration: calibration,
                noise_estimator: NoiseEstimatorConfig::Mcra(McraConfig::default()),
                interference: InterferenceConfig::Disabled,
                ..CallConfig::default()
            },
        ),
        (
            minimum,
            CallConfig {
                calibration_duration: calibration,
                noise_estimator: NoiseEstimatorConfig::Minimum(MinimumConfig::default()),
                ..CallConfig::default()
            },
        ),
    ]
}

fn through_rust(config: &CallConfig, packets: &[Packet]) -> Vec<u8> {
    let mut enhancer = CallEnhancer::new(config).expect("valid configuration");
    let mut output = Vec::new();
    let mut packet_out = [0; 160];
    for packet in packets {
        if enhancer.process_packet(packet, &mut packet_out).expect("not drained") == PacketOutcome::Emitted {
            output.extend_from_slice(&packet_out);
        }
    }
    let mut tail = [[0; 160]; DELAY_PACKETS];
    let withheld = enhancer.drain(&mut tail).expect("not drained");
    output.extend(tail[..withheld].iter().flatten());
    output
}

fn through_abi(config: &NoxConfig, packets: &[Packet]) -> Vec<u8> {
    let mut handle = Handle::new(config);
    let mut output = Vec::new();
    let mut packet_out = [0; 160];
    for packet in packets {
        let (status, outcome) = handle.process(packet, &mut packet_out);
        assert_eq!(status, NOX_STATUS_OK);
        if outcome == NOX_OUTCOME_EMITTED {
            output.extend_from_slice(&packet_out);
        }
    }
    let mut tail = [[0; 160]; DELAY_PACKETS];
    let (status, withheld) = handle.drain(&mut tail);
    assert_eq!(status, NOX_STATUS_OK);
    output.extend(tail[..withheld].iter().flatten());
    output
}

#[test]
fn abi_output_equals_rust_output() {
    let packets = synthetic_packets(PACKETS, SEED);
    for ((abi_config, rust_config), digest) in cases().iter().zip(DIGESTS) {
        let expected = through_rust(rust_config, &packets);
        let actual = through_abi(abi_config, &packets);
        assert_eq!(actual.len(), PACKETS * 160);
        assert_eq!(actual, expected, "the ABI output differs from the Rust output for {rust_config:?}");
        assert_eq!(fnv1a(&actual), digest, "digest of {rust_config:?}");
    }
}

#[test]
fn reset_reproduces_a_fresh_call() {
    let packets = synthetic_packets(120, SEED);
    let (config, _) = cases()[0];
    let fresh = through_abi(&config, &packets);
    let mut handle = Handle::new(&config);
    let mut scratch = [0; 160];
    for packet in &synthetic_packets(90, 7) {
        assert_eq!(handle.process(packet, &mut scratch).0, NOX_STATUS_OK);
    }
    assert_eq!(handle.reset(), NOX_STATUS_OK);
    let mut output = Vec::new();
    for packet in &packets {
        if handle.process(packet, &mut scratch) == (NOX_STATUS_OK, NOX_OUTCOME_EMITTED) {
            output.extend_from_slice(&scratch);
        }
    }
    let mut tail = [[0; 160]; DELAY_PACKETS];
    let (_, withheld) = handle.drain(&mut tail);
    output.extend(tail[..withheld].iter().flatten());
    assert_eq!(output, fresh);
}
