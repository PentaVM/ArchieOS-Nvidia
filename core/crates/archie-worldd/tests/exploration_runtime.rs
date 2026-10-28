#![cfg(feature = "provider")]
use archie_worldd::exploration_runtime::{Admission, RateLedger};

#[test]
fn configured_rate_is_shared_by_jobs_and_preserves_provider_backoff() {
    let mut rate = RateLedger::new(2, 2);
    assert_eq!(rate.admit(1_000), Admission::Ready);
    assert_eq!(rate.admit(1_001), Admission::Ready);
    rate.finish(100, None, Some(90_000));
    rate.finish(200, Some(4), None);
    assert_eq!(rate.admit(2_000), Admission::WaitUntil(90_000));
    assert_eq!(rate.admit(90_000), Admission::Ready);
    assert_eq!(rate.snapshot("terrain").bytes_received, 300);
    assert_eq!(rate.snapshot("terrain").requests_sent, 3);
    assert_eq!(rate.snapshot("terrain").remaining_requests, Some(4));
}

#[test]
fn restart_preserves_consumption_and_cooldown_but_not_dead_connections() {
    let mut rate = RateLedger::new(1, 1);
    assert_eq!(rate.admit(10), Admission::Ready);
    let saved = serde_json::to_vec(&rate).unwrap();
    let mut restored: RateLedger = serde_json::from_slice(&saved).unwrap();
    restored.recover_after_restart();
    assert_eq!(restored.admit(20), Admission::WaitUntil(60_010));
    assert_eq!(restored.admit(60_010), Admission::Ready);
}

#[test]
fn concurrency_admission_has_no_focus_or_human_input_gate() {
    let mut rate = RateLedger::new(100, 2);
    assert_eq!(rate.admit(10), Admission::Ready);
    assert_eq!(rate.admit(10), Admission::Ready);
    rate.finish(10, None, None);
    // Admission depends on provider load only; there is no global desktop state.
    assert_eq!(rate.admit(11), Admission::Ready);
    assert_eq!(rate.snapshot("terrain").in_flight, 2);
}
