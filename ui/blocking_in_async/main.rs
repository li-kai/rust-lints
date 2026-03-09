// Test cases for the `blocking_in_async` lint.

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

// Inside a regular (non-async) closure — not in async context.
fn ok_closure() {
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
