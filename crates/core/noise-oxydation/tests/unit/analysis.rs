use super::Analyzer;
use crate::{
    fft::{Complex, Fft},
    geometry::BINS,
    window::HannWindow,
};

/// Pushes `length` samples in chunks of `chunk` and counts the frames analyzed along the way.
fn frames_while_streaming(length: usize, chunk: usize) -> (Analyzer, usize) {
    let (window, fft) = (HannWindow::new(), Fft::new());
    let mut analyzer = Analyzer::new();
    let mut spectrum = [Complex::ZERO; BINS];
    let samples = vec![0.25_f32; length];
    let mut frames = 0;
    for packet in samples.chunks(chunk) {
        let mut pending = packet;
        while !pending.is_empty() {
            let consumed = analyzer.push(pending);
            pending = &pending[consumed..];
            if analyzer.frame_ready() {
                analyzer.analyze(&window, &fft, &mut spectrum);
                frames += 1;
            }
        }
    }
    (analyzer, frames)
}

#[test]
fn streaming_yields_one_frame_per_hop_after_the_first_full_frame() {
    for length in [0, 100, 255, 256, 383, 384, 1000, 16_000] {
        let expected = if length < 256 { 0 } else { (length - 256) / 128 + 1 };
        for chunk in [1, 160, 1000] {
            assert_eq!(frames_while_streaming(length, chunk).1, expected, "length {length}, chunk {chunk}");
        }
    }
}

#[test]
fn the_remainder_is_analyzed_exactly_once_when_samples_are_buffered() {
    let (window, fft) = (HannWindow::new(), Fft::new());
    let mut spectrum = [Complex::ZERO; BINS];
    for length in [1, 160, 256, 1000] {
        let (mut analyzer, _) = frames_while_streaming(length, 160);
        assert!(analyzer.analyze_remainder(&window, &fft, &mut spectrum), "length {length}");
        assert!(!analyzer.analyze_remainder(&window, &fft, &mut spectrum), "length {length}");
    }
    let (mut empty, _) = frames_while_streaming(0, 160);
    assert!(!empty.analyze_remainder(&window, &fft, &mut spectrum));
}
