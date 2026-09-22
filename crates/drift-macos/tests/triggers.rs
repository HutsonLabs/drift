//! M7-2: NWPathMonitor / wake adapters feeding the pure TriggerMerger.
#![allow(missing_docs, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use drift_core::{Trigger, TriggerAction, TriggerMerger};
use drift_macos::network::{PathMonitor, PathStatus, TriggerFeed, WakeObserver};
use drift_testkit::ManualClock;

#[test]
fn raw_path_status_values() {
    assert_eq!(PathStatus::from_raw(0), PathStatus::Invalid);
    assert_eq!(PathStatus::from_raw(1), PathStatus::Satisfied);
    assert_eq!(PathStatus::from_raw(2), PathStatus::Unsatisfied);
    assert_eq!(PathStatus::from_raw(3), PathStatus::Satisfiable);
    assert_eq!(PathStatus::from_raw(42), PathStatus::Invalid);
    assert_eq!(PathStatus::from_raw(-1), PathStatus::Invalid);
}

#[test]
fn path_status_to_trigger_table() {
    assert_eq!(PathStatus::Satisfied.trigger(), Some(Trigger::NetworkOnline));
    assert_eq!(PathStatus::Satisfiable.trigger(), Some(Trigger::NetworkOnline));
    assert_eq!(PathStatus::Unsatisfied.trigger(), Some(Trigger::NetworkOffline));
    assert_eq!(PathStatus::Invalid.trigger(), None);
}

fn feed(clock: &Arc<ManualClock>) -> (TriggerFeed, Arc<Mutex<Vec<TriggerAction>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = seen.clone();
    let f = TriggerFeed::new(true, clock.clone(), move |a| s.lock().unwrap().push(a));
    (f, seen)
}

#[test]
fn feed_table_offline_online_wake_and_duplicates() {
    let clock = Arc::new(ManualClock::new());
    let (f, seen) = feed(&clock);
    assert!(f.is_online());

    // The monitor's initial "satisfied" while online is a duplicate.
    f.path_status(PathStatus::Satisfied);
    assert_eq!(*seen.lock().unwrap(), vec![]);

    // Wi-Fi off: pause; repeated unsatisfied updates are ignored.
    f.path_status(PathStatus::Unsatisfied);
    f.path_status(PathStatus::Unsatisfied);
    assert_eq!(*seen.lock().unwrap(), vec![TriggerAction::PauseReconnect]);
    assert!(!f.is_online());

    // Wake while offline: nothing (the retry happens when the network returns).
    f.trigger(Trigger::Wake);
    assert_eq!(seen.lock().unwrap().len(), 1);

    // Network back: retry now.
    clock.advance(Duration::from_secs(10));
    f.path_status(PathStatus::Satisfied);
    assert_eq!(seen.lock().unwrap().last(), Some(&TriggerAction::RetryNow));
    assert_eq!(seen.lock().unwrap().len(), 2);

    // A wake right after the online retry is debounced …
    clock.advance(Duration::from_millis(300));
    f.trigger(Trigger::Wake);
    assert_eq!(seen.lock().unwrap().len(), 2);

    // … but a wake after the debounce window retries again.
    clock.advance(TriggerMerger::DEFAULT_DEBOUNCE);
    f.trigger(Trigger::Wake);
    assert_eq!(
        *seen.lock().unwrap(),
        vec![TriggerAction::PauseReconnect, TriggerAction::RetryNow, TriggerAction::RetryNow]
    );

    // Invalid status is ignored.
    f.path_status(PathStatus::Invalid);
    assert_eq!(seen.lock().unwrap().len(), 3);
}

#[test]
fn path_monitor_reports_the_initial_status() {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let monitor = PathMonitor::start(move |status| {
        let _ = tx.lock().unwrap().send(status);
    });
    let status = rx.recv_timeout(Duration::from_secs(5)).expect("nw_path_monitor delivers an initial update");
    assert_ne!(status, PathStatus::Invalid, "the initial update is determined");
    drop(monitor.expect("monitor created"));
}

#[test]
fn wake_observer_receives_the_workspace_wake_notification_until_dropped() {
    use objc2_app_kit::{NSWorkspace, NSWorkspaceDidWakeNotification};

    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let observer = WakeObserver::start(move || {
        c.fetch_add(1, Ordering::SeqCst);
    });
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: posting a notification name constant with no object/userInfo.
    unsafe { center.postNotificationName_object(NSWorkspaceDidWakeNotification, None) };
    assert_eq!(count.load(Ordering::SeqCst), 1);
    drop(observer);
    // SAFETY: as above.
    unsafe { center.postNotificationName_object(NSWorkspaceDidWakeNotification, None) };
    assert_eq!(count.load(Ordering::SeqCst), 1, "no calls after drop");
}
