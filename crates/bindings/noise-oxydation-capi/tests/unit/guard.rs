//! Panics are contained and poison the handle.

use noise_oxydation::{CallConfig, CallEnhancer, CallPhase};

use super::*;
use crate::abi::NOX_STATUS_OK;

#[test]
fn guard_returns_the_value_or_the_fallback() {
    assert_eq!(guard(0, || 7), 7);
    assert_eq!(guard(NOX_STATUS_PANIC, || -> u32 { panic!("contained") }), NOX_STATUS_PANIC);
}

#[test]
fn a_panicking_operation_poisons_the_handle() {
    let enhancer = CallEnhancer::new(&CallConfig::default()).expect("default configuration is valid");
    let mut guarded = GuardedEnhancer::new(enhancer);
    assert_eq!(guarded.read(|enhancer| u32::from(enhancer.phase() == CallPhase::Calibrating)), 1);

    let mut ran = false;
    assert_eq!(
        guarded.run(|_| {
            ran = true;
            panic!("contained")
        }),
        NOX_STATUS_PANIC
    );
    assert!(ran);

    let mut ran_again = false;
    assert_eq!(
        guarded.run(|_| {
            ran_again = true;
            NOX_STATUS_OK
        }),
        NOX_STATUS_PANIC
    );
    assert!(!ran_again, "a poisoned handle does not run operations");
    assert_eq!(guarded.read(|_| NOX_STATUS_OK), NOX_STATUS_PANIC);
}

#[test]
fn a_panicking_read_is_contained() {
    let enhancer = CallEnhancer::new(&CallConfig::default()).expect("default configuration is valid");
    let guarded = GuardedEnhancer::new(enhancer);
    assert_eq!(guarded.read(|_| -> u32 { panic!("contained") }), NOX_STATUS_PANIC);
}
