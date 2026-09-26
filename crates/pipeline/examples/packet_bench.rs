use noise_oxydation_pipeline::{Config, NoiseEstimator, PACKET_SAMPLES, Pipeline};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let packet = [0x80; PACKET_SAMPLES];
    let packets = 2_000;
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
        let start = Instant::now();
        for _ in 0..packets {
            black_box(
                pipeline
                    .process_packet(black_box(&packet))
                    .expect("active call"),
            );
        }
        black_box(pipeline.finish().expect("first finish"));
        let elapsed = start.elapsed();
        println!(
            "{noise_estimator:?}: {packets} packets in {elapsed:?}; {:.1} µs/input packet",
            elapsed.as_secs_f64() * 1_000_000.0 / f64::from(packets)
        );
    }
}
