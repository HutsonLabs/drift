//! M7-2 Red: `TriggerMerger` tables.

use std::time::{Duration, Instant};

use drift_core::triggers::{Trigger, TriggerAction, TriggerMerger};

use Trigger::{NetworkOffline as Off, NetworkOnline as On, Wake};
use TriggerAction::{PauseReconnect as Pause, RetryNow as Retry};

/// Runs `(ms, trigger)` steps through a fresh merger and returns the actions per step.
fn run(initially_online: bool, steps: &[(u64, Trigger)]) -> Vec<Option<TriggerAction>> {
    let t0 = Instant::now();
    let mut m = TriggerMerger::new(initially_online);
    steps.iter().map(|(ms, trig)| m.on_trigger(*trig, t0 + Duration::from_millis(*ms))).collect()
}

type Case<'a> = (&'a str, bool, &'a [(u64, Trigger)], &'a [Option<TriggerAction>]);

#[test]
fn trigger_tables() {
    #[rustfmt::skip]
    let table: &[Case<'_>] = &[
        ("offline pauses",                     true,  &[(0, Off)],                         &[Some(Pause)]),
        ("duplicate offline ignored",          true,  &[(0, Off), (10, Off)],              &[Some(Pause), None]),
        ("online after offline retries",       true,  &[(0, Off), (5000, On)],             &[Some(Pause), Some(Retry)]),
        ("duplicate online ignored",           true,  &[(0, On)],                          &[None]),
        ("online from offline start",          false, &[(0, On)],                          &[Some(Retry)]),
        ("wake retries when online",           true,  &[(0, Wake)],                        &[Some(Retry)]),
        ("wake while offline waits",           false, &[(0, Wake), (300, On)],             &[None, Some(Retry)]),
        ("retry, outage, online retries",      true,  &[(0, Wake), (0, Off), (400, On)],   &[Some(Retry), Some(Pause), Some(Retry)]),
        ("wake burst debounced",               true,  &[(0, Wake), (500, Wake), (1999, Wake)], &[Some(Retry), None, None]),
        ("wake after debounce retries again",  true,  &[(0, Wake), (2000, Wake)],          &[Some(Retry), Some(Retry)]),
        ("online then wake: one retry",        false, &[(0, On), (300, Wake)],             &[Some(Retry), None]),
        ("flapping network",                   true,  &[(0, Off), (100, On), (200, Off), (300, On)],
                                                      &[Some(Pause), Some(Retry), Some(Pause), Some(Retry)]),
        ("offline, dup online, wake",          true,  &[(0, Off), (100, On), (150, On), (400, Wake)],
                                                      &[Some(Pause), Some(Retry), None, None]),
    ];
    for (name, online, steps, expected) in table {
        assert_eq!(run(*online, steps), expected.to_vec(), "{name}");
    }
}

#[test]
fn reports_online_state() {
    let t = Instant::now();
    let mut m = TriggerMerger::new(true);
    assert!(m.is_online());
    m.on_trigger(Off, t);
    assert!(!m.is_online());
    m.on_trigger(On, t);
    assert!(m.is_online());
}

#[test]
fn custom_debounce_window() {
    let t = Instant::now();
    let mut m = TriggerMerger::with_debounce(true, Duration::from_millis(100));
    assert_eq!(m.on_trigger(Wake, t), Some(Retry));
    assert_eq!(m.on_trigger(Wake, t + Duration::from_millis(99)), None);
    assert_eq!(m.on_trigger(Wake, t + Duration::from_millis(100)), Some(Retry));
}

#[test]
fn clock_going_backwards_does_not_panic() {
    let t = Instant::now() + Duration::from_secs(10);
    let mut m = TriggerMerger::new(true);
    assert_eq!(m.on_trigger(Wake, t), Some(Retry));
    assert_eq!(m.on_trigger(Wake, t - Duration::from_secs(5)), None);
}
