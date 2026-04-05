# `must_not_suspend`

**Level:** `deny`

Flags values of specific types that are held alive across `.await` points. Supersedes Clippy's `await_holding_lock` and `await_holding_refcell_ref` with a single configurable lint.

## Why

- **Deadlocks** — a `MutexGuard` held across `.await` keeps the lock while the task is suspended. If another task on the same executor needs the lock to make progress, the executor deadlocks. Unlike blocking code, the holder isn't actively running toward the unlock.
- **Broken span nesting** — a `tracing::span::Entered` guard held across `.await` keeps the span active while the task is suspended. When another task runs on the same thread, its events are attributed to the wrong span.
- **RefCell panics** — a `Ref` or `RefMut` held across `.await` keeps the borrow active across an unbounded suspension. Any attempt to borrow the same `RefCell` will panic.
- **Cancellation unsafety** — if the future is dropped at the `.await` point, the guard's `Drop` runs in an arbitrary context. For mutex guards this unlocks from a potentially unexpected state; for span guards this corrupts the thread-local span stack.

The fix is to scope the guard so it is dropped before the `.await`:

```rust
// Before (deadlock risk):
async fn bad(mtx: &Mutex<Data>) {
    let guard = mtx.lock();
    send(guard.clone()).await;  // guard held across await
}

// After:
async fn good(mtx: &Mutex<Data>) {
    let data = {
        let guard = mtx.lock();
        guard.clone()
    };
    send(data).await;  // guard already dropped
}
```

## What it checks

The lint fires when a value of a flagged type is live across an `.await` expression. A value is "live across `.await`" when it is created before the `.await` and used or dropped after it, or when its scope encloses the `.await`.

## Default types

### `std::sync` — standard library locks

| Type | Risk |
|---|---|
| `std::sync::MutexGuard` | Deadlock — blocks executor thread on contention |
| `std::sync::RwLockReadGuard` | Deadlock — blocks executor thread on contention |
| `std::sync::RwLockWriteGuard` | Deadlock — blocks executor thread on contention |

### `std::cell` — runtime borrow checking

| Type | Risk |
|---|---|
| `std::cell::Ref` | Panic — concurrent `borrow_mut()` panics while `Ref` is alive |
| `std::cell::RefMut` | Panic — concurrent `borrow()` panics while `RefMut` is alive |

### `parking_lot` — alternative lock guards

| Type | Risk |
|---|---|
| `parking_lot::MutexGuard` | Deadlock |
| `parking_lot::FairMutexGuard` | Deadlock |
| `parking_lot::RwLockReadGuard` | Deadlock |
| `parking_lot::RwLockWriteGuard` | Deadlock |
| `parking_lot::RwLockUpgradableReadGuard` | Deadlock |
| `parking_lot::MappedMutexGuard` | Deadlock |
| `parking_lot::MappedFairMutexGuard` | Deadlock |
| `parking_lot::MappedRwLockReadGuard` | Deadlock |
| `parking_lot::MappedRwLockWriteGuard` | Deadlock |
| `parking_lot::ArcMutexGuard` | Deadlock |
| `parking_lot::ArcRwLockReadGuard` | Deadlock |
| `parking_lot::ArcRwLockWriteGuard` | Deadlock |
| `parking_lot::ArcRwLockUpgradableReadGuard` | Deadlock |

### `tracing` — span guards

| Type | Risk |
|---|---|
| `tracing::span::Entered` | Corrupted span nesting — events on other tasks attributed to wrong span |
| `tracing::span::EnteredSpan` | Corrupted span nesting — events on other tasks attributed to wrong span |

### `crossbeam` — epoch-based reclamation

| Type | Risk |
|---|---|
| `crossbeam_epoch::Guard` | Delays memory reclamation while the task is suspended, causing unbounded memory growth |

### `rusqlite` — database transactions

| Type | Risk |
|---|---|
| `rusqlite::Transaction` | Holds exclusive connection lock — blocks all other queries while suspended |
| `rusqlite::Savepoint` | Holds exclusive connection lock — blocks all other queries while suspended |

### Connection pools — checked-out connections

| Type | Risk |
|---|---|
| `r2d2::PooledConnection` | Pool starvation — connection checked out across `.await` is unavailable to other tasks while suspended; with small pools this deadlocks the application |
| `diesel::r2d2::PooledConnection` | Same as `r2d2::PooledConnection` (re-export) |

## Examples

### Fires

```rust
async fn process(mtx: &std::sync::Mutex<Vec<Job>>) {
    let guard = mtx.lock().unwrap();
    //~^ ERROR: `MutexGuard` held across `.await` — this can deadlock the executor
    do_work(guard.last()).await;
}
```

```rust
async fn process(mtx: &std::sync::Mutex<Vec<Job>>) {
    let guard = mtx.lock().unwrap();
    //~^ ERROR: `MutexGuard` held across `.await` — this can deadlock the executor
    let job = guard.last().cloned();
    drop(guard); // explicit drop does not help — guard's scope encloses the .await
    do_work(job).await;
}
```

```rust
async fn traced() {
    let span = tracing::info_span!("op");
    let _entered = span.enter();
    //~^ ERROR: `Entered` held across `.await` — span nesting will be corrupted
    do_work().await;
}
```

```rust
async fn borrow(cell: &RefCell<Config>) {
    let cfg = cell.borrow();
    //~^ ERROR: `Ref` held across `.await` — concurrent borrows will panic
    fetch(cfg.url()).await;
}
```

### Does not fire

```rust
// Guard scoped before .await.
async fn good(mtx: &std::sync::Mutex<Vec<Job>>) {
    let job = {
        let guard = mtx.lock().unwrap();
        guard.last().cloned()
    };
    do_work(job).await; // OK — guard already dropped
}
```

```rust
// Using async-aware mutex — safe to hold across .await.
async fn good(mtx: &tokio::sync::Mutex<Vec<Job>>) {
    let guard = mtx.lock().await;
    do_work(guard.last()).await; // OK — tokio::sync::MutexGuard is designed for this
}
```

```rust
// Tracing's async-aware instrument pattern.
async fn good() {
    do_work()
        .instrument(tracing::info_span!("op"))
        .await; // OK — no Entered guard
}
```

```rust
// No .await in scope — synchronous code is fine.
fn sync_fn(mtx: &std::sync::Mutex<Data>) {
    let guard = mtx.lock().unwrap();
    process(&guard); // OK — not async
}
```

```rust
// Inside #[cfg(test)] — test code is excluded.
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_lock() {
        let mtx = std::sync::Mutex::new(42);
        let guard = mtx.lock().unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await; // OK — test code
    }
}
```

## Configuration

```toml
[must_not_suspend]
# Additional fully-qualified type paths to flag beyond the built-in defaults.
# additional_types = ["my_crate::ConnectionGuard"]

# If true, disables the built-in default types and uses only `additional_types`.
# skip_default_types = false
```

| Field | Type | Default | Description |
|---|---|---|---|
| `additional_types` | `Vec<String>` | `[]` | Extra type paths to flag when held across `.await` |
| `skip_default_types` | `bool` | `false` | If `true`, only `additional_types` are checked |

## Superseded Clippy lints

Disable these Clippy lints to avoid duplicate diagnostics:

```toml
[workspace.lints.clippy]
await_holding_lock        = "allow"  # superseded by must_not_suspend
await_holding_refcell_ref = "allow"  # superseded by must_not_suspend
```

| Clippy lint | Limitation | `must_not_suspend` improvement |
|---|---|---|
| `await_holding_lock` | Hardcoded to `std::sync` guards only | Covers `parking_lot`, `dashmap`, `crossbeam_epoch`, connection pools, `rusqlite`, and `tracing` out of the box |
| `await_holding_refcell_ref` | Hardcoded to `std::cell::Ref`/`RefMut` only | Unified with lock guards under a single configurable lint |

## Relation to nightly `#[must_not_suspend]`

Rust nightly has an experimental `#[must_not_suspend]` attribute (RFC 3014, tracking issue #87521). When stabilized, library authors will annotate their types directly and the compiler will enforce the constraint. Until then — and it has been unstable since 2021 — this lint provides the same protection on stable Rust with a configurable type list. Migration is straightforward: as upstream libraries adopt the attribute, remove those types from `additional_types`.

## Relation to other lints

| Lint | Catches |
|---|---|
| `must_not_suspend` | Guard types held across `.await` — deadlocks, panics, span corruption |
| `blocking_in_async` | Blocking calls (not guards) that stall the executor |
| `unsafe_send_missing_drop` | `unsafe impl Send` without `Drop` — unsound cross-thread destruction |
| `panic_in_drop` | Panicking inside `Drop` — abort during unwinding |
