use super::{TAIL_CONTEXT_SAMPLES, TAIL_SAMPLES, region_statistics, tail_peak_change_db, tail_peak_ratio_db};

#[test]
fn identical_regions_have_no_difference() {
    let bytes: Vec<u8> = (0..=u8::MAX).collect();
    let statistics = region_statistics(&bytes, &bytes, 0..bytes.len());
    assert_eq!(statistics.identical_bytes, 256);
    assert_eq!(statistics.max_abs_difference, 0);
    assert_eq!(statistics.difference_to_reference_db, None);
    assert!((statistics.identical_fraction() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn differences_are_counted_per_region() {
    // 0x80 decodes to +32124 and 0x00 to -32124: flipping the sign doubles the amplitude of the difference.
    let reference = vec![0x80; 10];
    let mut candidate = reference.clone();
    candidate[7..].fill(0x00);
    let head = region_statistics(&reference, &candidate, 0..7);
    assert_eq!((head.identical_bytes, head.max_abs_difference, head.difference_to_reference_db), (7, 0, None));
    let tail = region_statistics(&reference, &candidate, 7..10);
    assert_eq!((tail.samples, tail.identical_bytes, tail.max_abs_difference), (3, 0, 64_248));
    let ratio = tail.difference_to_reference_db.expect("differs");
    assert!((ratio - 10.0 * 4.0_f64.log10()).abs() < 1e-9, "{ratio}");
    let overall = region_statistics(&reference, &candidate, 0..10);
    assert!((overall.identical_fraction() - 0.7).abs() < 1e-12);
}

#[test]
fn the_tail_peak_is_relative_to_the_preceding_second() {
    let mut bytes = vec![0xFF; TAIL_CONTEXT_SAMPLES + TAIL_SAMPLES];
    bytes[100] = 0x80;
    assert_eq!(tail_peak_ratio_db(&bytes), Some(f64::NEG_INFINITY), "a silent tail");
    *bytes.last_mut().expect("non-empty") = 0x80;
    assert_eq!(tail_peak_ratio_db(&bytes), Some(0.0));
    assert_eq!(tail_peak_ratio_db(&bytes[1..]), None, "too short for the context");
    bytes[100] = 0xFF;
    assert_eq!(tail_peak_ratio_db(&bytes), None, "silent context");
}

#[test]
fn the_tail_change_compares_the_same_samples_of_both_files() {
    let mut reference = vec![0xFF; 300];
    reference[250] = 0x80;
    let mut candidate = reference.clone();
    assert_eq!(tail_peak_change_db(&reference, &candidate), Some(0.0));
    candidate[299] = 0x00;
    assert_eq!(tail_peak_change_db(&reference, &candidate), Some(0.0), "the peak is a magnitude");
    candidate[250] = 0xFF;
    candidate[299] = 0xFF;
    assert_eq!(tail_peak_change_db(&reference, &candidate), Some(f64::NEG_INFINITY), "a silenced tail");
    assert_eq!(tail_peak_change_db(&candidate, &reference), None, "silent reference tail");
    assert_eq!(tail_peak_change_db(&reference[..100], &candidate[..100]), None, "shorter than the tail");
    assert_eq!(tail_peak_change_db(&reference, &candidate[1..]), None, "different lengths");
}
