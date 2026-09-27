use std::path::PathBuf;

use hound::{SampleFormat, WavSpec, WavWriter};

use super::{read_mulaw_packets, read_wav, write_bytes, write_wav};
use crate::error::EvalError;

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("noise-oxydation-eval-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("temporary directory");
    directory.join(name)
}

#[test]
fn a_written_wav_reads_back_as_normalized_samples() {
    let path = scratch("round-trip.wav");
    let samples = [0, 1, -1, i16::MAX, i16::MIN, 12_345];
    write_wav(&path, 8000, &samples).expect("write");
    let read = read_wav(&path, 8000).expect("read");
    let expected: Vec<f64> = samples.iter().map(|&sample| f64::from(sample) / 32768.0).collect();
    assert_eq!(read, expected);
}

#[test]
fn wav_files_with_another_rate_or_layout_are_rejected() {
    let path = scratch("rate.wav");
    write_wav(&path, 16_000, &[0; 10]).expect("write");
    assert!(matches!(
        read_wav(&path, 8000),
        Err(EvalError::UnsupportedWav { sample_rate: 16_000, expected_sample_rate: 8000, .. })
    ));

    let stereo = scratch("stereo.wav");
    let spec = WavSpec { channels: 2, sample_rate: 8000, bits_per_sample: 16, sample_format: SampleFormat::Int };
    let mut writer = WavWriter::create(&stereo, spec).expect("create");
    for _ in 0..4 {
        writer.write_sample(0_i16).expect("sample");
    }
    writer.finalize().expect("finalize");
    assert!(matches!(read_wav(&stereo, 8000), Err(EvalError::UnsupportedWav { channels: 2, .. })));

    assert!(matches!(read_wav(&scratch("missing.wav"), 8000), Err(EvalError::Wav { .. })));
}

#[test]
fn mulaw_inputs_must_hold_whole_packets() {
    let path = scratch("packets.ul");
    write_bytes(&path, &[0xFF; 320]).expect("write");
    assert_eq!(read_mulaw_packets(&path).expect("two packets").len(), 320);
    for length in [0, 159, 161] {
        write_bytes(&path, &vec![0xFF; length]).expect("write");
        assert!(
            matches!(read_mulaw_packets(&path), Err(EvalError::NotWholePackets { bytes, .. }) if bytes == length),
            "{length} bytes"
        );
    }
}
