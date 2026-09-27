//! Independent calls on parallel threads produce the same bytes as sequential processing.

use std::{thread, time::Duration};

use noise_oxydation::CallEnhancer;

use crate::support::{enhancer_with_calibration, noise_packets, run_call};

const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<CallEnhancer>();
};

#[test]
fn parallel_calls_match_sequential_calls() {
    let calls: Vec<_> = (0_u8..6).map(|index| noise_packets(400, 100 + u64::from(index), 60 + 5 * index)).collect();
    let calibration = Duration::from_secs(1);
    let sequential: Vec<Vec<u8>> =
        calls.iter().map(|call| run_call(&mut enhancer_with_calibration(calibration), call).output).collect();

    let parallel: Vec<Vec<u8>> = thread::scope(|scope| {
        let handles: Vec<_> = calls
            .iter()
            .map(|call| scope.spawn(move || run_call(&mut enhancer_with_calibration(calibration), call).output))
            .collect();
        handles.into_iter().map(|handle| handle.join().expect("call thread")).collect()
    });

    assert_eq!(parallel, sequential);
    assert!(sequential.windows(2).all(|pair| pair[0] != pair[1]), "calls have distinct inputs and outputs");
}
