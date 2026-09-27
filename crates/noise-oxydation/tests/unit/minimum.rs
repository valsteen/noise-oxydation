use super::MinimumEstimator;
use crate::{
    config::MinimumConfig,
    geometry::BINS,
    test_support::{TestRng, assert_same_bits},
};

fn uniform(value: f32) -> [f32; BINS] {
    [value; BINS]
}

/// Random non-negative power spectrum spanning several orders of magnitude.
fn random_power(rng: &mut TestRng) -> [f32; BINS] {
    let mut power = [0.0; BINS];
    for value in &mut power {
        *value = 10_f32.powf(3.0 * rng.next_signed_f32());
    }
    power
}

/// Straightforward model of the documented estimator: the smoothed power history of every frame since tracking
/// started, preceded by `window` copies of the initial value, and the minimum of the last `window` entries.
struct BruteForce {
    smoothing: f32,
    window: usize,
    history: Vec<[f32; BINS]>,
    smoothed: [f32; BINS],
}

impl BruteForce {
    fn new(smoothing: f32, window: usize, initial: &[f32; BINS]) -> Self {
        Self { smoothing, window, history: vec![*initial; window], smoothed: *initial }
    }

    fn step(&mut self, power: &[f32; BINS]) -> [f32; BINS] {
        for (smoothed, &observed) in self.smoothed.iter_mut().zip(power) {
            *smoothed = self.smoothing * *smoothed + (1.0 - self.smoothing) * observed.max(1e-12);
        }
        self.history.push(self.smoothed);
        let recent = &self.history[self.history.len() - self.window..];
        let mut minimum = [f32::INFINITY; BINS];
        for (bin, value) in minimum.iter_mut().enumerate() {
            *value = recent.iter().map(|frame| frame[bin]).fold(f32::INFINITY, f32::min).max(1e-12);
        }
        minimum
    }
}

#[test]
fn the_estimate_is_the_exact_minimum_over_the_last_w_smoothed_frames() {
    for window in [1_u16, 2, 5, 50] {
        let config = MinimumConfig { window_frames: window, ..MinimumConfig::default() };
        let mut rng = TestRng::new(u64::from(window));
        let mut estimator = MinimumEstimator::new(config);
        let calibration = [random_power(&mut rng), random_power(&mut rng)];
        let mut initial = [0.0; BINS];
        for frame in &calibration {
            let mean = estimator.estimate(frame, true);
            initial = *mean;
        }
        let mut model = BruteForce::new(config.smoothing, usize::from(window), &initial);
        for frame in 0..300 {
            let power = random_power(&mut rng);
            let expected = model.step(&power);
            let actual = estimator.estimate(&power, false);
            assert_same_bits(actual, &expected, &format!("window {window}, frame {frame}"));
        }
    }
}

#[test]
fn calibration_exposes_the_running_mean_and_initializes_tracking() {
    let mut estimator = MinimumEstimator::new(MinimumConfig::default());
    assert_same_bits(estimator.estimate(&uniform(2.0), true), &uniform(2.0), "first calibration frame");
    assert_same_bits(estimator.estimate(&uniform(4.0), true), &uniform(3.0), "calibration mean");
    // Smoothing starts from the mean 3: a quiet frame gives S = 0.8·3 + 0.2·0.5 = 2.5, below the mean in every
    // history slot, so it becomes the minimum ...
    let quiet = 0.8_f32 * 3.0 + 0.2 * 0.5;
    assert_same_bits(estimator.estimate(&uniform(0.5), false), &uniform(quiet), "quiet frame");
    // ... which a louder frame (S = 0.8·2.5 + 0.2·50 = 12) does not raise.
    assert_same_bits(estimator.estimate(&uniform(50.0), false), &uniform(quiet), "loud frame");
    // Frames 3–50 replace the 48 slots that still hold the mean 3. The quiet frame (frame 1) stays in the 50-frame
    // window through frame 50, and frame 51 pushes it out.
    for _ in 0..47 {
        estimator.estimate(&uniform(50.0), false);
    }
    assert_same_bits(estimator.estimate(&uniform(50.0), false), &uniform(quiet), "quiet frame still in the window");
    let later = estimator.estimate(&uniform(50.0), false)[0];
    assert!(later > 3.0, "quiet frame left the window: {later}");
}

#[test]
fn without_calibration_the_first_frame_initializes_the_estimate() {
    let mut estimator = MinimumEstimator::new(MinimumConfig::default());
    let mut power = uniform(6.0);
    power[7] = 0.0;
    power[8] = f32::INFINITY;
    let mut expected = uniform(6.0);
    expected[7] = 1e-12;
    expected[8] = 1e-12;
    assert_same_bits(estimator.estimate(&power, false), &expected, "first frame");
}

#[test]
fn silence_drives_the_estimate_down_to_the_floor_but_not_below() {
    // Zero power is floored before smoothing, so S = floor + (1 − floor)·0.8ⁿ approaches the floor from above.
    let config = MinimumConfig { floor: 1e-3, ..MinimumConfig::default() };
    let mut estimator = MinimumEstimator::new(config);
    estimator.estimate(&uniform(1.0), true);
    for frame in 1..=200 {
        let noise = estimator.estimate(&uniform(0.0), false);
        assert!(noise.iter().all(|&value| value >= 1e-3), "frame {frame}: {noise:?}");
    }
    let noise = estimator.estimate(&uniform(0.0), false);
    assert!(noise.iter().all(|&value| value <= 1.000_01e-3), "{noise:?}");
}

#[test]
fn reset_matches_a_fresh_estimator() {
    let mut rng = TestRng::new(9);
    let frames: Vec<_> = (0..80).map(|_| random_power(&mut rng)).collect();
    let mut reused = MinimumEstimator::new(MinimumConfig::default());
    for frame in &frames {
        reused.estimate(frame, false);
    }
    reused.reset();
    let mut fresh = MinimumEstimator::new(MinimumConfig::default());
    for (index, frame) in frames.iter().enumerate() {
        let calibration = index < 10;
        let expected = *fresh.estimate(frame, calibration);
        assert_same_bits(reused.estimate(frame, calibration), &expected, &format!("frame {index}"));
    }
}
