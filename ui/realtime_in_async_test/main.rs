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

// Should trigger: tokio::time::sleep in a test without start_paused.

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

// Should NOT trigger: start_paused = true makes time instant.

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

// Should NOT trigger: no time operations at all.

#[tokio::test]
async fn ok_no_time_ops() {
    assert_eq!(2 + 2, 4); // OK: no time calls
}

// Should NOT trigger: synchronous test (not tokio::test).

#[test]
fn ok_sync_test() {
    std::thread::sleep(Duration::from_millis(10)); // OK: not async test
}

// Should NOT trigger: non-test async function.

async fn ok_non_test() {
    tokio::time::sleep(Duration::from_secs(1)).await; // OK: not a test
}

// Should NOT trigger: plain async helper (not a test) using tokio time APIs.

/// Shared teardown: await handle with a timeout for clean exit.
async fn shutdown_forwarder(handle: tokio::task::JoinHandle<std::io::Result<()>>) {
    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("forwarder should exit within 2s")
        .expect("task should not panic")
        .expect("forwarder should return Ok");
}

// Should NOT trigger: suppressed with #[allow].

#[allow(realtime_in_async_test)]
#[tokio::test]
async fn ok_allowed() {
    tokio::time::sleep(Duration::from_secs(5)).await; // OK: explicitly allowed
}

// Edge case: manual runtime with start_paused — should NOT trigger.

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

// Edge case: tokio::time::advance is fine (it's the solution, not the problem).

#[tokio::test(start_paused = true)]
async fn ok_advance() {
    tokio::time::advance(Duration::from_secs(60)).await; // OK: this is the right pattern
}

// Should trigger: std::time::Instant::now() in a paused-clock test.

#[tokio::test(start_paused = true)]
async fn trigger_std_instant_in_paused() {
    let start = std::time::Instant::now(); //~ WARNING: `std::time::Instant::now()` does not respect
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(start.elapsed() >= Duration::from_secs(30));
}

// Should NOT trigger: tokio::time::Instant::now() is correct in paused-clock tests.

#[tokio::test(start_paused = true)]
async fn ok_tokio_instant_in_paused() {
    let start = tokio::time::Instant::now(); // OK: respects paused clock
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(tokio::time::Instant::now() >= start);
}

// Should NOT trigger: std::time::Instant::now() in a plain sync test.

#[test]
fn ok_std_instant_in_sync_test() {
    let start = std::time::Instant::now(); // OK: no paused clock
    std::thread::sleep(Duration::from_millis(10));
    assert!(start.elapsed() >= Duration::from_millis(10));
}

// Should NOT trigger: std::time::Instant::now() in a non-paused async test
// (the tokio::time::sleep is flagged separately, not the Instant).

#[tokio::test]
async fn ok_std_instant_without_paused() {
    let start = std::time::Instant::now(); // OK: no paused clock to conflict with
    tokio::time::sleep(Duration::from_secs(1)).await; //~ WARNING: real-time wait
}

// Should trigger: test calls helper that transitively uses tokio time.

#[tokio::test]
async fn trigger_via_helper() {
    let handle = tokio::spawn(async { Ok(()) });
    shutdown_forwarder(handle).await; //~ WARNING: real-time wait
}

// Should NOT trigger: test with start_paused calls the same helper.

#[tokio::test(start_paused = true)]
async fn ok_paused_via_helper() {
    let handle = tokio::spawn(async { Ok(()) });
    shutdown_forwarder(handle).await; // OK: paused clock
}

// Should trigger: unrelated `.start_paused(true)` must not suppress the lint.

struct FakeBuilder;

impl FakeBuilder {
    fn start_paused(self, _yes: bool) -> Self {
        self
    }
}

#[tokio::test]
async fn trigger_fake_start_paused_method() {
    let _fake = FakeBuilder.start_paused(true);
    tokio::time::sleep(Duration::from_secs(1)).await; //~ WARNING: real-time wait
}

// Should NOT trigger: plain async helper inside a test module
// (only triggers when called from a test without start_paused).

#[cfg(test)]
mod tests {
    use std::time::Duration;

    async fn shutdown_forwarder_in_mod(handle: tokio::task::JoinHandle<std::io::Result<()>>) {
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("forwarder should exit within 2s")
            .expect("task should not panic")
            .expect("forwarder should return Ok");
    }

    #[tokio::test]
    async fn trigger_via_mod_helper() {
        let handle = tokio::spawn(async { Ok(()) });
        shutdown_forwarder_in_mod(handle).await; //~ WARNING: real-time wait
    }
}

// Should NOT trigger: helper only uses std::time::Instant::now(), which
// is not an error without a paused clock (regression test for transitive
// Instant::now() false positive).

fn measure_elapsed() -> std::time::Duration {
    let start = std::time::Instant::now();
    std::thread::sleep(Duration::from_millis(1));
    start.elapsed()
}

#[tokio::test]
async fn ok_helper_only_uses_std_instant() {
    let _elapsed = measure_elapsed(); // OK: Instant::now() without paused clock is fine
}

// Should NOT ICE: enum tuple variant constructor is a local "callee"
// but has no body. The transitive checker must not call
// `hir_body_owned_by` on it (regression test for ICE on CtorOf).

use std::path::PathBuf;

enum SandboxError {
    HostPathNotAbsolute(PathBuf),
}

async fn make_sandbox_error() -> SandboxError {
    SandboxError::HostPathNotAbsolute(PathBuf::from("/tmp"))
}

#[tokio::test]
async fn ok_enum_ctor_no_ice() {
    // Calls a local helper that constructs an enum tuple variant.
    // The variant constructor resolves as a local callee — must not ICE.
    let _err = make_sandbox_error().await;
}

// Should NOT ICE: trait method declaration has no body.
// The transitive checker must not call `hir_body_owned_by` on it
// (regression test for ICE on bodyless AssocFn).

trait VmLike {
    fn state(&self) -> u32;
}

struct FakeVm;

impl VmLike for FakeVm {
    fn state(&self) -> u32 {
        42
    }
}

fn check_vm_state(vm: &dyn VmLike) -> u32 {
    vm.state()
}

#[tokio::test]
async fn ok_trait_method_no_ice() {
    let vm = FakeVm;
    let _s = check_vm_state(&vm);
}

fn main() {}
