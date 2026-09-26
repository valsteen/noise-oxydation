#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)] // Bounded synthetic test signals.

use noise_oxydation_codec::{decode, encode};
use noise_oxydation_pipeline::{Config, PACKET_SAMPLES, PacketBatch, Pipeline, PipelineError};
use std::error::Error;
use std::time::Duration;

fn take(batch: &PacketBatch, output: &mut Vec<u8>) {
    for index in 0..batch.len() {
        let (packet, valid) = batch.packet(index).unwrap();
        output.extend_from_slice(&packet[..valid]);
    }
}

#[test]
fn packet_timing_flush_and_reset() {
    let mut pipeline = Pipeline::new(Config::default()).unwrap();
    assert!(pipeline.finish().unwrap().is_empty());
    assert!(matches!(
        pipeline.finish(),
        Err(PipelineError::AlreadyFinished)
    ));
    pipeline.reset();

    for count in [1, 2, 3, 8] {
        let mut output = Vec::new();
        for index in 0..count {
            let packet = [encode((i16::try_from(index).unwrap() + 1) * 512); PACKET_SAMPLES];
            let batch = pipeline.process_packet(&packet).unwrap();
            if count == 3 {
                assert_eq!(batch.len(), usize::from(index == 2));
            }
            if count == 8 && index == 3 {
                assert_eq!(batch.len(), 2);
            }
            for i in 0..batch.len() {
                assert_eq!(batch.packet(i).unwrap().1, PACKET_SAMPLES);
            }
            take(&batch, &mut output);
        }
        let tail = pipeline.finish().unwrap();
        if count == 1 {
            assert_eq!(tail.packet(0).unwrap().1, PACKET_SAMPLES);
        }
        take(&tail, &mut output);
        assert_eq!(output.len(), count * PACKET_SAMPLES);
        assert!(matches!(
            pipeline.process_packet(&[0xff; PACKET_SAMPLES]),
            Err(PipelineError::AlreadyFinished)
        ));
        pipeline.reset();
    }

    let clean = pipeline.process_packet(&[0xff; PACKET_SAMPLES]).unwrap();
    assert!(clean.is_empty());
    let tail = pipeline.finish().unwrap();
    assert!(
        tail.packet(0).unwrap().0[..PACKET_SAMPLES]
            .iter()
            .all(|&code| decode(code) == 0)
    );
}

#[test]
fn constructor_errors_keep_the_dsp_cause() {
    assert!(matches!(
        Pipeline::new(Config {
            learning_duration: Duration::ZERO
        }),
        Err(PipelineError::InvalidLearningDuration)
    ));
    let Err(error) = Pipeline::new(Config {
        learning_duration: Duration::from_millis(20),
    }) else {
        panic!("20 ms has less than one FFT window");
    };
    assert!(matches!(error, PipelineError::DspInitialization(_)));
    assert!(error.source().is_some());
}

#[test]
fn default_and_alternate_learning_bypass_then_suppress_noise() {
    let input = [encode(1_000); PACKET_SAMPLES];
    let mut default = Pipeline::new(Config::default()).unwrap();
    let mut baseline = Vec::new();
    for _ in 0..10 {
        take(&default.process_packet(&input).unwrap(), &mut baseline);
    }
    take(&default.finish().unwrap(), &mut baseline);
    assert_eq!(baseline.len(), 10 * PACKET_SAMPLES);
    assert!(
        baseline
            .iter()
            .all(|&code| decode(code) == decode(input[0]))
    );

    for seconds in [1, 5] {
        check_suppression(seconds);
    }
}

fn check_suppression(seconds: usize) {
    let mut pipeline = Pipeline::new(Config {
        learning_duration: Duration::from_secs(seconds as u64),
    })
    .unwrap();
    let mut input_pcm = Vec::new();
    let mut output = Vec::new();
    let mut rng = 0x1234_5678_u32;
    for packet_index in 0..(seconds + 3) * 50 {
        let mut packet = [0xff; PACKET_SAMPLES];
        for (sample_index, code) in packet.iter_mut().enumerate() {
            rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (f32::from((rng >> 16) as i16) / 32_768.0) * 0.13;
            let time = (packet_index * PACKET_SAMPLES + sample_index) as f32 / 8_000.0;
            let speech = if packet_index >= (seconds + 1) * 50 {
                0.35 * (2.0 * std::f32::consts::PI * 300.0 * time).sin()
            } else {
                0.0
            };
            *code = encode(((noise + speech) * 32_768.0) as i16);
            input_pcm.push(f32::from(decode(*code)) / 32_768.0);
        }
        take(&pipeline.process_packet(&packet).unwrap(), &mut output);
    }
    take(&pipeline.finish().unwrap(), &mut output);
    assert_eq!(output.len(), input_pcm.len());
    let processed: Vec<f32> = output
        .iter()
        .map(|&code| f32::from(decode(code)) / 32_768.0)
        .collect();
    assert!(processed.iter().all(|sample| sample.is_finite()));
    assert_eq!(
        &processed[..seconds * 8_000 - 1_000],
        &input_pcm[..seconds * 8_000 - 1_000]
    );
    let rms = |samples: &[f32]| {
        (samples.iter().map(|value| value * value).sum::<f32>() / samples.len() as f32).sqrt()
    };
    let noise_start = seconds * 8_000 + 1_000;
    let speech_start = (seconds + 1) * 8_000 + 1_000;
    let noise_in = rms(&input_pcm[noise_start..noise_start + 5_000]);
    let noise_out = rms(&processed[noise_start..noise_start + 5_000]);
    let speech_out = rms(&processed[speech_start..speech_start + 6_000]);
    assert!(
        noise_out < noise_in * 0.8,
        "noise input {noise_in}, output {noise_out}"
    );
    assert!(speech_out > 0.16, "speech output RMS {speech_out}");
}
