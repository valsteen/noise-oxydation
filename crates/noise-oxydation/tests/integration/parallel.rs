//! Independent calls on parallel threads produce the same bytes as sequential processing.

use std::{thread, time::Duration};

use noise_oxydation::CallEnhancer;

use crate::support::{all_configurations, enhancer, noise_packets, run_call};

const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<CallEnhancer>();
};

/// Six concurrent calls, each with a different estimator and interference combination and its own input.
#[test]
fn parallel_calls_with_mixed_configurations_match_sequential_calls() {
    let configurations = all_configurations(Duration::from_secs(1));
    let calls: Vec<_> = (0_u8..6).map(|index| noise_packets(400, 100 + u64::from(index), 60 + 5 * index)).collect();
    let sequential: Vec<Vec<u8>> =
        configurations.iter().zip(&calls).map(|(config, call)| run_call(&mut enhancer(config), call).output).collect();

    let parallel: Vec<Vec<u8>> = thread::scope(|scope| {
        let handles: Vec<_> = configurations
            .iter()
            .zip(&calls)
            .map(|(config, call)| scope.spawn(move || run_call(&mut enhancer(config), call).output))
            .collect();
        handles.into_iter().map(|handle| handle.join().expect("call thread")).collect()
    });

    assert_eq!(parallel, sequential);
    assert!(sequential.windows(2).all(|pair| pair[0] != pair[1]), "calls have distinct inputs and outputs");
}
