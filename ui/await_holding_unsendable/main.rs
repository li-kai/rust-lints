// Test cases for the `await_holding_unsendable` lint.
//
// std::sync guards and std::cell refs are NOT tested here — Clippy's
// `await_holding_lock` and `await_holding_refcell_ref` cover those.
// This lint complements Clippy by catching types it doesn't know about.
#![allow(
    dead_code,
    unused_variables,
    unknown_lints,
    topological_ordering,
    blocking_in_async
)]

async fn do_work() {}

// ── SHOULD TRIGGER ──────────────────────────────────────────────────

async fn tracing_entered_across_await() {
    let span = tracing::info_span!("op");
    let _entered = span.enter(); //~ ERROR: `Entered` held across `.await`
    do_work().await;
}

async fn tracing_entered_in_async_block() {
    let _fut = async {
        let span = tracing::info_span!("inner");
        let _entered = span.enter(); //~ ERROR: `Entered` held across `.await`
        do_work().await;
    };
}

// ── SHOULD NOT TRIGGER ──────────────────────────────────────────────

// Tracing's async-aware instrument pattern — no Entered guard.
async fn tracing_instrument() {
    use tracing::Instrument as _;
    do_work().instrument(tracing::info_span!("op")).await; // OK
}

// Entered scoped before .await.
async fn tracing_entered_scoped_before_await() {
    let span = tracing::info_span!("op");
    {
        let _entered = span.enter();
        // do sync work
    }
    do_work().await; // OK — entered already dropped
}

// Synchronous function — no async context.
fn sync_fn_tracing() {
    let span = tracing::info_span!("op");
    let _entered = span.enter(); // OK — not async
}

// No flagged types at all.
async fn no_guard() {
    let x = 42;
    do_work().await; // OK
}

// tokio mutex is designed for async — not flagged.
async fn tokio_mutex(mtx: &tokio::sync::Mutex<u32>) {
    let guard = mtx.lock().await;
    do_work().await; // OK
}

fn main() {}
