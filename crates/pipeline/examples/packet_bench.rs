use noise_oxydation_pipeline::{Config, NoiseEstimator, PACKET_SAMPLES, Pipeline};
use std::error::Error;
use std::fs;
use std::hint::black_box;
use std::io::{self, ErrorKind};
use std::time::Instant;

const INPUT_BYTES: usize = 317_120;
const LEARNING_PACKETS: usize = 250;
const SUPPRESSION_START: usize = 300;

#[allow(clippy::cast_precision_loss)] // One packet cannot take 2^53 nanoseconds in this run.
fn percentile(sorted: &[u128], percent: usize) -> f64 {
    let index = (sorted.len() * percent).div_ceil(100) - 1;
    sorted[index] as f64 / 1_000.0
}

fn print_phase(name: &str, samples: &mut [u128]) {
    samples.sort_unstable();
    println!(
        "  {name}: {} packets, p50={:.2} µs, p95={:.2} µs",
        samples.len(),
        percentile(samples, 50),
        percentile(samples, 95)
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "usage: packet_bench <verified OSR prepared.mulaw>",
        )
    })?;
    if args.next().is_some() {
        return Err(io::Error::new(ErrorKind::InvalidInput, "too many arguments").into());
    }
    let input = fs::read(path)?;
    if input.len() != INPUT_BYTES {
        return Err(io::Error::new(ErrorKind::InvalidData, "expected 317120 μ-law bytes").into());
    }
    let packets = input.as_chunks::<PACKET_SAMPLES>().0;
    println!(
        "{} {} {} build; {} OSR input packets; 250 learning, 50 transition, {} suppression",
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        packets.len(),
        packets.len() - SUPPRESSION_START
    );
    for noise_estimator in [
        NoiseEstimator::SppMmse,
        NoiseEstimator::Mcra,
        NoiseEstimator::Minimum,
    ] {
        let mut pipeline = Pipeline::new(Config {
            noise_estimator,
            ..Config::default()
        })
        .expect("valid configuration");
        for packet in packets {
            black_box(pipeline.process_packet(packet).expect("active warmup call"));
        }
        black_box(pipeline.finish().expect("first warmup finish"));
        pipeline.reset();

        let mut learning = Vec::with_capacity(LEARNING_PACKETS);
        let mut suppression = Vec::with_capacity(packets.len() - SUPPRESSION_START);
        let mut valid_samples = 0;
        for (index, packet) in packets.iter().enumerate() {
            let start = Instant::now();
            let batch = pipeline.process_packet(packet).expect("active call");
            let nanos = start.elapsed().as_nanos();
            for i in 0..batch.len() {
                valid_samples += batch.packet(i).expect("valid batch index").1;
            }
            black_box(batch);
            if index < LEARNING_PACKETS {
                learning.push(nanos);
            } else if index >= SUPPRESSION_START {
                suppression.push(nanos);
            }
        }
        let start = Instant::now();
        let tail = pipeline.finish().expect("first finish");
        let finish_us = start.elapsed().as_secs_f64() * 1_000_000.0;
        for i in 0..tail.len() {
            valid_samples += tail.packet(i).expect("valid batch index").1;
        }
        black_box(tail);
        println!("{noise_estimator:?}:");
        print_phase("learning", &mut learning);
        print_phase("suppression", &mut suppression);
        println!("  finish={finish_us:.2} µs; valid_samples={valid_samples}");
        assert_eq!(valid_samples, input.len());
    }
    Ok(())
}
