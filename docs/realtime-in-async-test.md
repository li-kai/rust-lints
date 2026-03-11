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

