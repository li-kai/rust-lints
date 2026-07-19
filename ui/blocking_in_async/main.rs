// Test cases for the `blocking_in_async` lint.
#![allow(dead_code, unknown_lints, topological_ordering)]

use std::time::Duration;

// ── SHOULD TRIGGER ──────────────────────────────────────────────────

async fn trigger_fs_read() {
    let _ = std::fs::read_to_string("foo.txt"); //~ WARNING: blocking call
}

async fn trigger_thread_sleep() {
    std::thread::sleep(Duration::from_secs(1)); //~ WARNING: blocking call
}

async fn trigger_net_connect() {
    let _ = std::net::TcpStream::connect("127.0.0.1:8080"); //~ WARNING: blocking call
}

async fn trigger_in_async_block() {
    let _fut = async {
        let _ = std::fs::read_to_string("bar.txt"); //~ WARNING: blocking call
    };
}

// Should trigger: parking_lot's `Mutex::lock` blocks the executor on contention.
// parking_lot's `Mutex` is `lock_api::Mutex` re-exported via the public
// `parking_lot::lock_api` module, so `def_path_str` yields
// `parking_lot::lock_api::Mutex::lock`; the configured default path includes the
// `lock_api` segment to match it.
async fn trigger_parking_lot_lock(m: &parking_lot::Mutex<u32>) {
    let _g = m.lock(); //~ WARNING: blocking call
}

// Should trigger: the closure passed to `Iterator::for_each` is driven
// synchronously on the executor thread, so the blocking read inside it still
// starves the executor. `is_in_async_context` treats a sync closure invoked in
// place (a method-call argument here) as transparent and keeps walking out to
// the enclosing async fn.
async fn trigger_sync_closure_invoked_inline() {
    [0u8].iter().for_each(|_| {
        let _ = std::fs::read_to_string("foo.txt"); //~ WARNING: blocking call
    });
}

// Should trigger: an immediately-invoked closure (IIFE) runs on the spot, so
// the blocking call executes on the executor thread.
async fn trigger_iife() {
    (|| {
        let _ = std::fs::read_to_string("foo.txt"); //~ WARNING: blocking call
    })();
}

// ── SHOULD NOT TRIGGER ──────────────────────────────────────────────

// Synchronous function — no async context.
fn ok_sync_fs_read() {
    let _ = std::fs::read_to_string("foo.txt");
}

// Inside spawn_blocking — intentional escape hatch.
async fn ok_spawn_blocking() {
    tokio::task::spawn_blocking(|| {
        let _ = std::fs::read_to_string("foo.txt");
        std::thread::sleep(Duration::from_secs(1));
    });
}

// A closure handed to `std::thread::Builder::spawn` runs on the freshly
// spawned OS thread, never on the executor — spawn-method receivers are
// escape hatches just like `spawn_blocking`.
async fn ok_thread_builder_spawn() {
    let _ = std::thread::Builder::new().name("worker".into()).spawn(|| {
        let _ = std::fs::read_to_string("foo.txt");
    });
}

// Inside a regular (non-async) closure — not in async context.
fn ok_closure() {
    let _f = || {
        let _ = std::fs::read_to_string("foo.txt");
    };
}

// A SYNC closure that is merely stored in a `let` (never invoked here) within
// an async fn — its body runs wherever the closure is later called (possibly
// another thread), NOT necessarily on the executor, so this should NOT trigger.
// `is_in_async_context` treats a stored sync closure as opaque (it is neither an
// IIFE nor a method-call argument) and stops walking.
async fn ok_sync_closure_in_async() {
    let _f = || {
        let _ = std::fs::read_to_string("foo.txt");
    };
}

// #[allow] suppresses the lint.
#[allow(blocking_in_async)]
async fn ok_allowed() {
    let _ = std::fs::read_to_string("foo.txt");
}

fn main() {
    // Synchronous main — not async context, should not trigger.
    let _ = std::fs::read_to_string("foo.txt");
}

#[test]
fn test_suppressed() {
    // Inside test — suppressed.
}
