#![allow(
    dead_code,
    unknown_lints,
    unused_variables,
    unused_must_use,
    clippy::allow_attributes_without_reason,
    topological_ordering
)]
// Tests for the `realtime_in_async_test` lint.

use std::time::Duration;

// ══════════════════════════════════════════════════════════════════════
// Should trigger: tokio::time::sleep in a test without start_paused.
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn trigger_sleep() {
    tokio::time::sleep(Duration::from_secs(5)).await; //~ WARNING: real-time wait
}

#[tokio::test]
async fn trigger_timeout() {
    let _ = tokio::time::timeout(
        //~ WARNING: real-time wait
        Duration::from_secs(5),
        async { 42 },
    )
    .await;
}

#[tokio::test]
async fn trigger_interval() {
    let mut interval = tokio::time::interval(Duration::from_secs(1)); //~ WARNING: real-time wait
    interval.tick().await;
}

#[tokio::test]
async fn trigger_sleep_until() {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    tokio::time::sleep_until(deadline).await; //~ WARNING: real-time wait
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: start_paused = true makes time instant.
// ══════════════════════════════════════════════════════════════════════

#[tokio::test(start_paused = true)]
async fn ok_paused_sleep() {
    tokio::time::sleep(Duration::from_secs(60)).await; // OK: paused clock
}

#[tokio::test(start_paused = true)]
async fn ok_paused_timeout() {
    let _ = tokio::time::timeout(Duration::from_secs(5), async { 42 }).await; // OK: paused clock
}

#[tokio::test(start_paused = true)]
async fn ok_paused_interval() {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.tick().await; // OK: paused clock
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: no time operations at all.
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ok_no_time_ops() {
    assert_eq!(2 + 2, 4); // OK: no time calls
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: synchronous test (not tokio::test).
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ok_sync_test() {
    std::thread::sleep(Duration::from_millis(10)); // OK: not async test
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: non-test async function.
// ══════════════════════════════════════════════════════════════════════

async fn ok_non_test() {
    tokio::time::sleep(Duration::from_secs(1)).await; // OK: not a test
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: suppressed with #[allow].
// ══════════════════════════════════════════════════════════════════════

#[allow(realtime_in_async_test)]
#[tokio::test]
async fn ok_allowed() {
    tokio::time::sleep(Duration::from_secs(5)).await; // OK: explicitly allowed
}

// ══════════════════════════════════════════════════════════════════════
// Edge case: manual runtime with start_paused — should NOT trigger.
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ok_manual_runtime_paused() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::sleep(Duration::from_secs(60)).await; // OK: start_paused(true)
        });
}

// ══════════════════════════════════════════════════════════════════════
// Edge case: tokio::time::advance is fine (it's the solution, not the problem).
// ══════════════════════════════════════════════════════════════════════

#[tokio::test(start_paused = true)]
async fn ok_advance() {
    tokio::time::advance(Duration::from_secs(60)).await; // OK: this is the right pattern
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: std::time::Instant::now() in a paused-clock test.
// ══════════════════════════════════════════════════════════════════════

#[tokio::test(start_paused = true)]
async fn trigger_std_instant_in_paused() {
    let start = std::time::Instant::now(); //~ WARNING: `std::time::Instant::now()` does not respect
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(start.elapsed() >= Duration::from_secs(30));
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: tokio::time::Instant::now() is correct in paused-clock tests.
// ══════════════════════════════════════════════════════════════════════

#[tokio::test(start_paused = true)]
async fn ok_tokio_instant_in_paused() {
    let start = tokio::time::Instant::now(); // OK: respects paused clock
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(tokio::time::Instant::now() >= start);
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: std::time::Instant::now() in a plain sync test.
// ══════════════════════════════════════════════════════════════════════

#[test]
fn ok_std_instant_in_sync_test() {
    let start = std::time::Instant::now(); // OK: no paused clock
    std::thread::sleep(Duration::from_millis(10));
    assert!(start.elapsed() >= Duration::from_millis(10));
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: std::time::Instant::now() in a non-paused async test
// (the tokio::time::sleep is flagged separately, not the Instant).
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ok_std_instant_without_paused() {
    let start = std::time::Instant::now(); // OK: no paused clock to conflict with
    tokio::time::sleep(Duration::from_secs(1)).await; //~ WARNING: real-time wait
}

fn main() {}
