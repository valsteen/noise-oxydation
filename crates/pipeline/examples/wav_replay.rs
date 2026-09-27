//! Offline replay of caller-supplied 8 kHz mono 16-bit PCM WAV through the public packet API.
use noise_oxydation_codec::{decode, encode};
use noise_oxydation_pipeline::{
    Config, NoiseEstimator, PACKET_SAMPLES, PacketBatch, Pipeline, ProcessingMode,
};
use std::error::Error;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;

const SPEECH_START: usize = 48_000;
const NOISE_START: usize = 40_800;
const NOISE_END: usize = 47_200;
const SEED: u32 = 0x5eed_1234;
const NOISE_LEVEL: f64 = 0.03;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

fn read_wav(bytes: &[u8]) -> io::Result<Vec<i16>> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(invalid("expected a RIFF/WAVE file"));
    }
    let riff_size = usize::try_from(u32::from_le_bytes(bytes[4..8].try_into().unwrap()))
        .map_err(|_| invalid("WAV is too large"))?;
    let riff_end = riff_size
        .checked_add(8)
        .filter(|&end| end == bytes.len())
        .ok_or_else(|| invalid("RIFF length does not match file length"))?;
    let (mut format, mut data) = (None, None);
    let mut offset = 12;
    while offset < riff_end {
        if offset + 8 > riff_end {
            return Err(invalid("truncated WAV chunk header"));
        }
        let size = usize::try_from(u32::from_le_bytes(
            bytes[offset + 4..offset + 8].try_into().unwrap(),
        ))
        .map_err(|_| invalid("WAV chunk is too large"))?;
        let start = offset + 8;
        let end = start
            .checked_add(size)
            .filter(|&end| end <= riff_end)
            .ok_or_else(|| invalid("truncated WAV chunk"))?;
        match &bytes[offset..offset + 4] {
            b"fmt " => format = Some(&bytes[start..end]),
            b"data" => data = Some(&bytes[start..end]),
            _ => {}
        }
        offset = end + (size & 1);
    }
    if offset != riff_end {
        return Err(invalid("invalid WAV chunk padding"));
    }
    let format = format.ok_or_else(|| invalid("missing WAV fmt chunk"))?;
    let data = data.ok_or_else(|| invalid("missing WAV data chunk"))?;
    if format.len() < 16
        || u16::from_le_bytes(format[0..2].try_into().unwrap()) != 1
        || u16::from_le_bytes(format[2..4].try_into().unwrap()) != 1
        || u32::from_le_bytes(format[4..8].try_into().unwrap()) != 8_000
        || u32::from_le_bytes(format[8..12].try_into().unwrap()) != 16_000
        || u16::from_le_bytes(format[12..14].try_into().unwrap()) != 2
        || u16::from_le_bytes(format[14..16].try_into().unwrap()) != 16
        || data.len() % 2 != 0
    {
        return Err(invalid("expected 8 kHz mono 16-bit little-endian PCM WAV"));
    }
    Ok(data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&sample| i16::from_le_bytes(sample))
        .collect())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // Fixed seed and clamped PCM.
fn prepare(source: &[i16]) -> (Vec<u8>, Vec<f64>) {
    let padded = source.len().div_ceil(PACKET_SAMPLES) * PACKET_SAMPLES;
    let mut input = Vec::with_capacity(SPEECH_START + padded);
    let clean: Vec<f64> = source
        .iter()
        .map(|&sample| f64::from(decode(encode(sample))) / 32_768.0)
        .collect();
    let mut rng = SEED;
    for index in 0..SPEECH_START + padded {
        if index >= SPEECH_START + source.len() {
            input.push(0xff);
            continue;
        }
        rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise = f64::from((rng >> 16) as i16) / 32_768.0 * NOISE_LEVEL;
        let speech = if index < SPEECH_START {
            0.0
        } else {
            f64::from(source[index - SPEECH_START]) / 32_768.0
        };
        let pcm = ((speech + noise) * 32_768.0)
            .round()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
        input.push(encode(pcm));
    }
    (input, clean)
}

fn append(batch: &PacketBatch, output: &mut Vec<u8>) {
    for index in 0..batch.len() {
        let (packet, valid) = batch.packet(index).expect("valid batch index");
        output.extend_from_slice(&packet[..valid]);
    }
}

fn replay(
    input: &[u8],
    estimator: NoiseEstimator,
    mode: ProcessingMode,
) -> Result<(Vec<u8>, usize), Box<dyn Error>> {
    let mut pipeline = Pipeline::new_with_mode(
        Config {
            noise_estimator: estimator,
            ..Config::default()
        },
        mode,
    )?;
    let mut output = Vec::with_capacity(input.len());
    let mut first_output_packet = 0;
    for (index, packet) in input.as_chunks::<PACKET_SAMPLES>().0.iter().enumerate() {
        let batch = pipeline.process_packet(packet)?;
        if first_output_packet == 0 && !batch.is_empty() {
            first_output_packet = index + 1;
        }
        append(&batch, &mut output);
    }
    append(&pipeline.finish()?, &mut output);
    Ok((output, first_output_packet))
}

#[allow(clippy::cast_precision_loss)] // Offline WAV slices are far below 2^53 samples.
fn rms(samples: &[f64]) -> f64 {
    (samples.iter().map(|sample| sample * sample).sum::<f64>() / samples.len() as f64).sqrt()
}

#[allow(clippy::cast_precision_loss)] // Offline WAV slices are far below 2^53 samples.
fn speech_metrics(clean: &[f64], output: &[f64]) -> io::Result<(f64, f64)> {
    let count = clean.len() as f64;
    let clean_mean = clean.iter().sum::<f64>() / count;
    let output_mean = output.iter().sum::<f64>() / count;
    let mut dot = 0.0;
    let mut clean_energy = 0.0;
    let mut centered_dot = 0.0;
    let mut clean_variance = 0.0;
    let mut output_variance = 0.0;
    for (&reference, &observed) in clean.iter().zip(output) {
        dot += observed * reference;
        clean_energy += reference * reference;
        centered_dot += (observed - output_mean) * (reference - clean_mean);
        clean_variance += (reference - clean_mean).powi(2);
        output_variance += (observed - output_mean).powi(2);
    }
    if clean_energy == 0.0 || clean_variance == 0.0 || output_variance == 0.0 {
        return Err(invalid("speech window has zero energy or variance"));
    }
    Ok((
        centered_dot / (clean_variance * output_variance).sqrt(),
        dot / clean_energy,
    ))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "usage: wav_replay <8k-mono.wav> [prepared.mulaw] [outputs_dir] [conservative|experimental]",
        )
    })?;
    let dump = args.next();
    let outputs_dir = args.next();
    let mode = match args.next().as_deref().and_then(std::ffi::OsStr::to_str) {
        None | Some("conservative") => ProcessingMode::Conservative,
        Some("experimental") => ProcessingMode::ExperimentalLowDelay,
        _ => {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                "mode must be conservative or experimental",
            )
            .into());
        }
    };
    if args.next().is_some() {
        return Err(io::Error::new(ErrorKind::InvalidInput, "too many arguments").into());
    }
    let source = read_wav(&fs::read(&path)?)?;
    if source.len() <= 1_600 {
        return Err(invalid("WAV needs more than 1600 samples for speech metrics").into());
    }
    let (input, clean) = prepare(&source);
    if let Some(path) = dump {
        fs::write(path, &input)?;
    }
    if let Some(ref dir) = outputs_dir {
        fs::create_dir_all(dir)?;
    }
    let speech_end = SPEECH_START + source.len() - 800;
    println!(
        "source: {} samples, mono 8000 Hz 16-bit PCM; seed=0x{SEED:08x}, noise_level={NOISE_LEVEL}; prepared_input={} samples",
        source.len(),
        input.len()
    );
    println!("noise window=[{NOISE_START},{NOISE_END}); speech window=[48800,{speech_end})");
    let input_noise: Vec<f64> = input[NOISE_START..NOISE_END]
        .iter()
        .map(|&code| f64::from(decode(code)) / 32_768.0)
        .collect();
    for estimator in [
        NoiseEstimator::SppMmse,
        NoiseEstimator::Mcra,
        NoiseEstimator::Minimum,
    ] {
        let (output, first_output_packet) = replay(&input, estimator, mode)?;
        if output.len() != input.len() {
            return Err(invalid("pipeline returned a different valid sample count").into());
        }
        if let Some(ref dir) = outputs_dir {
            fs::write(Path::new(dir).join(format!("{estimator:?}.mulaw")), &output)?;
        }
        let output_noise: Vec<f64> = output[NOISE_START..NOISE_END]
            .iter()
            .map(|&code| f64::from(decode(code)) / 32_768.0)
            .collect();
        let speech: Vec<f64> = output[48_800..speech_end]
            .iter()
            .map(|&code| f64::from(decode(code)) / 32_768.0)
            .collect();
        let (correlation, projection_gain) =
            speech_metrics(&clean[800..source.len() - 800], &speech)?;
        println!(
            "{mode:?} {estimator:?}: first_output_input_packet={first_output_packet} valid_samples={} noise_rms_ratio={:.4} speech_correlation={correlation:.4} speech_projection_gain={projection_gain:.4}",
            output.len(),
            rms(&output_noise) / rms(&input_noise)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{prepare, read_wav};

    #[test]
    fn wav_boundary_and_preparation_are_deterministic() {
        let mut wav = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x80\x3e\0\0\x02\0\x10\0data\x04\0\0\0\x01\0\xff\xff".to_vec();
        wav[4..8].copy_from_slice(&40_u32.to_le_bytes());
        let source = read_wav(&wav).unwrap();
        assert_eq!(source, [1, -1]);
        assert_eq!(prepare(&source), prepare(&source));
        wav[22] = 2;
        assert!(read_wav(&wav).is_err());
    }
}
