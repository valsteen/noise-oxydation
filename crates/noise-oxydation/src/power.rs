//! Power-spectrum helpers shared by the noise estimators and the tonal transient detector.

use crate::{convert::narrow, geometry::BINS};

/// Replaces values below `floor`, NaN and infinities with `floor`.
pub(crate) fn sanitize(value: f32, floor: f32) -> f32 {
    if value.is_finite() && value >= floor { value } else { floor }
}

/// Running mean of the sanitized power per bin over the quiet-intro calibration frames, accumulated in `f64`.
#[derive(Debug, Clone)]
pub(crate) struct CalibrationMean {
    sum: [f64; BINS],
    frames: u32,
}

impl CalibrationMean {
    pub(crate) fn new() -> Self {
        Self { sum: [0.0; BINS], frames: 0 }
    }

    /// Adds one calibration frame and writes the floored running mean to `mean`.
    pub(crate) fn accumulate(&mut self, power: &[f32; BINS], floor: f32, mean: &mut [f32; BINS]) {
        self.frames = self.frames.saturating_add(1);
        let frames = f64::from(self.frames);
        for ((sum, mean), &observed) in self.sum.iter_mut().zip(mean).zip(power) {
            *sum += f64::from(sanitize(observed, floor));
            *mean = sanitize(narrow(*sum / frames), floor);
        }
    }

    /// Whether no calibration frame has been accumulated.
    pub(crate) fn is_empty(&self) -> bool {
        self.frames == 0
    }

    pub(crate) fn reset(&mut self) {
        self.sum.fill(0.0);
        self.frames = 0;
    }
}
