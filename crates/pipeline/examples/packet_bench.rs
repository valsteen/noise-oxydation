use noise_oxydation_pipeline::{Config, NoiseEstimator, PACKET_SAMPLES, Pipeline};
use std::hint::black_box;
use std::time::Instant;

const PACKETS: usize = 2_000;
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

fn main() {
    let packet = [0x80; PACKET_SAMPLES];
    println!(
        "{} {} {} build; {PACKETS} identical input packets; 250 learning, 50 transition, 1700 suppression",
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
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
        for _ in 0..PACKETS {
            black_box(
                pipeline
                    .process_packet(&packet)
                    .expect("active warmup call"),
            );
        }
        black_box(pipeline.finish().expect("first warmup finish"));
        pipeline.reset();

        let mut learning = Vec::with_capacity(LEARNING_PACKETS);
        let mut suppression = Vec::with_capacity(PACKETS - SUPPRESSION_START);
        for index in 0..PACKETS {
            let start = Instant::now();
            let batch = pipeline.process_packet(&packet).expect("active call");
            let nanos = start.elapsed().as_nanos();
            black_box(batch);
            if index < LEARNING_PACKETS {
                learning.push(nanos);
            } else if index >= SUPPRESSION_START {
                suppression.push(nanos);
            }
        }
        let start = Instant::now();
        black_box(pipeline.finish().expect("first finish"));
        let finish_us = start.elapsed().as_secs_f64() * 1_000_000.0;
        println!("{noise_estimator:?}:");
        print_phase("learning", &mut learning);
        print_phase("suppression", &mut suppression);
        println!("  finish={finish_us:.2} µs");
    }
}
