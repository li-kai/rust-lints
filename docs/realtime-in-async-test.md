# `realtime_in_async_test`

**Level:** `warn`

Flags `tokio::time::sleep`, `timeout`, `interval`, and related calls inside async test functions that don't have the tokio clock paused.

## Why

Async tests that call `tokio::time::sleep` or similar functions wait on real wall-clock time by default. A test sleeping for 5 seconds takes 5 seconds to run. This slows CI, makes tests flaky under load, and couples test speed to real time rather than logical time.

Tokio's `start_paused = true` mode solves this: the clock starts frozen and auto-advances whenever the runtime would otherwise block waiting for a timer. A test that sleeps for an hour completes instantly.

## Examples

### Triggers

```rust
#[tokio::test]
async fn test_retry_backoff() {
    //~^ WARNING: real-time wait in async test without paused clock
    tokio::time::sleep(Duration::from_secs(5)).await;
}
```

```rust
#[tokio::test]
async fn test_request_timeout() {
    let _ = tokio::time::timeout( //~ WARNING: real-time wait in async test without paused clock
        Duration::from_secs(5),
        fetch_data(),
    ).await;
}
```

### Does not trigger

```rust
// Clock is paused — time calls are instant.
#[tokio::test(start_paused = true)]
async fn test_retry_backoff() {
    tokio::time::sleep(Duration::from_secs(5)).await; // OK
}

// Manual runtime with start_paused(true) is also recognised.
#[test]
fn test_with_manual_runtime() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .unwrap()
        .block_on(async {
            tokio::time::sleep(Duration::from_secs(60)).await; // OK
        });
}

// Not a test function — production code is not flagged.
async fn wait_for_ready() {
    tokio::time::sleep(Duration::from_secs(1)).await; // OK
}

// tokio::time::advance is the solution, not the problem.
#[tokio::test(start_paused = true)]
async fn test_advance() {
    tokio::time::advance(Duration::from_secs(60)).await; // OK
}
```

## Configuration

```toml
[realtime_in_async_test]
allowed_paths = ["my_crate::time::sleep"]
```

| Field | Type | Default | Description |
|---|---|---|---|
| `allowed_paths` | `[string]` | `[]` | Additional fully-qualified paths to treat as time calls (appended to the defaults) |

## Implementation notes

### Lint pass

`LateLintPass::check_fn` — called for every function definition. Two conditions must both be true to fire:

1. The function is inside a test (detected via `clippy_utils::is_in_test`).
2. The body contains at least one flagged time call but no `.start_paused(true)` call.

### Detection approach

`#[tokio::test(start_paused = true)]` is a proc macro attribute that is consumed before HIR. It cannot be inspected directly. Instead, the lint observes what the macro *generates*: a `.start_paused(true)` method call on the runtime builder. Walking the expanded body for this call gives reliable detection without coupling to proc macro internals.

### Visitor

`TimeCallVisitor` walks the body collecting two signals:

| Signal | How detected |
|---|---|
| First flagged time call | `resolve_callee_def_id` + `find_matching_path` against the configured path list |
| `.start_paused(true)` present | `ExprKind::MethodCall` with method name `"start_paused"` and a single `true` boolean literal argument |

The visitor short-circuits once both signals are known, since the lint outcome is already determined.

### Flagged paths (defaults)

- `tokio::time::sleep`
- `tokio::time::sleep_until`
- `tokio::time::timeout`
- `tokio::time::timeout_at`
- `tokio::time::interval`
- `tokio::time::interval_at`

`tokio::time::advance` is intentionally excluded — it is the correct tool for manually stepping a paused clock.

### Skip conditions

| Condition | Reason |
|---|---|
| Not in a test | Only test code is flagged |
| `.start_paused(true)` found | Clock is paused; time calls are instant |
| No flagged time calls | Nothing to flag |
| `#[allow(realtime_in_async_test)]` | Explicitly suppressed |

### Diagnostic

```
warning: real-time wait in async test without paused clock
  --> src/jobs/retry_test.rs:12:5
   |
12 |     tokio::time::sleep(Duration::from_secs(5)).await;
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: add `start_paused = true` to `#[tokio::test]` so the clock auto-advances and tests run instantly:
           `#[tokio::test(start_paused = true)]`
```
