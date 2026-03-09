// Test cases for the `unbounded_channel` lint.

use std::sync::mpsc;

// ── SHOULD TRIGGER ──────────────────────────────────────────────────

fn trigger_std_mpsc() {
    let (_tx, _rx) = mpsc::channel::<String>(); //~ WARNING: unbounded channel
}

// Note: tokio/crossbeam/flume triggers cannot be tested in UI tests without
// adding those crates as dev-dependencies. The path-matching logic is identical
// to global_side_effect which is already well-tested. We rely on the std case
// to validate the end-to-end flow.

// ── SHOULD NOT TRIGGER ──────────────────────────────────────────────

fn ok_std_sync_channel() {
    // Bounded: explicit capacity
    let (_tx, _rx) = mpsc::sync_channel::<String>(100);
}

fn main() {
    // Inside main — suppressed (composition root)
    let (_tx, _rx) = mpsc::channel::<String>();

    ok_std_sync_channel();
    trigger_std_mpsc();
}

#[test]
fn test_suppressed() {
    // Inside test — suppressed
    let (_tx, _rx) = mpsc::channel::<String>();
}

#[allow(unbounded_channel)]
fn allowed_explicitly() {
    // Suppressed via allow attribute
    let (_tx, _rx) = mpsc::channel::<String>();
}
