use super::Synthesizer;
use crate::{
    analysis::Analyzer,
    fft::{Complex, Fft},
    geometry::{BINS, FFT_SIZE, HOP_SIZE},
    test_support::TestRng,
    window::HannWindow,
};

/// Streams `input` through analysis and synthesis with unmodified spectra, including the zero-padded remainder
/// frame and the synthesis tail, and returns every emitted sample.
fn reconstruct(input: &[f32]) -> Vec<f32> {
    let (window, fft) = (HannWindow::new(), Fft::new());
    let mut analyzer = Analyzer::new();
    let mut synthesizer = Synthesizer::new();
    let mut spectrum = [Complex::ZERO; BINS];
    let mut hop = [0.0; HOP_SIZE];
    let mut output = Vec::new();
    let mut pending = input;
    while !pending.is_empty() {
        let consumed = analyzer.push(pending);
        pending = &pending[consumed..];
        if analyzer.frame_ready() {
            analyzer.analyze(&window, &fft, &mut spectrum);
            synthesizer.synthesize(&window, &fft, &spectrum, &mut hop);
            output.extend_from_slice(&hop);
        }
    }
    if analyzer.analyze_remainder(&window, &fft, &mut spectrum) {
        synthesizer.synthesize(&window, &fft, &spectrum, &mut hop);
        output.extend_from_slice(&hop);
    }
    let mut tail = [0.0; FFT_SIZE - HOP_SIZE];
    if synthesizer.finish(&mut tail) {
        output.extend_from_slice(&tail);
    }
    output
}

#[test]
fn unmodified_spectra_reconstruct_the_input() {
    let mut rng = TestRng::new(11);
    for length in [160, 1000, 1024, 4800] {
        let input: Vec<f32> = (0..length).map(|_| rng.next_signed_f32() * 0.5).collect();
        let output = reconstruct(&input);
        assert!(output.len() > input.len() && output.len() <= input.len() + 128, "length {length}");
        // Sample 0 has zero squared-window weight and is emitted as 0 (reference concern U1).
        assert!(output[0].abs() < f32::EPSILON);
        // Sample 1 is divided by w[1]² ≈ 2.3e-8 and amplifies FFT rounding; it is checked loosely.
        assert!((output[1] - input[1]).abs() < 5e-2, "sample 1: {} vs {}", output[1], input[1]);
        for (index, (restored, original)) in output.iter().zip(&input).enumerate().skip(2) {
            assert!((restored - original).abs() < 2e-3, "length {length}, sample {index}: {restored} vs {original}");
        }
    }
}

#[test]
fn finishing_without_a_frame_emits_nothing() {
    let mut synthesizer = Synthesizer::new();
    let mut tail = [0.0; FFT_SIZE - HOP_SIZE];
    assert!(!synthesizer.finish(&mut tail));
}

#[test]
fn reset_discards_pending_overlap() {
    let (window, fft) = (HannWindow::new(), Fft::new());
    let mut synthesizer = Synthesizer::new();
    let mut spectrum = [Complex::ZERO; BINS];
    spectrum[3] = Complex::new(40.0, -12.0);
    let mut hop = [0.0; HOP_SIZE];
    synthesizer.synthesize(&window, &fft, &spectrum, &mut hop);
    synthesizer.reset();
    let mut tail = [0.0; FFT_SIZE - HOP_SIZE];
    assert!(!synthesizer.finish(&mut tail));

    let mut fresh = Synthesizer::new();
    let mut fresh_hop = [0.0; HOP_SIZE];
    synthesizer.synthesize(&window, &fft, &spectrum, &mut hop);
    fresh.synthesize(&window, &fft, &spectrum, &mut fresh_hop);
    assert!(hop.iter().zip(&fresh_hop).all(|(left, right)| left.to_bits() == right.to_bits()));
}
