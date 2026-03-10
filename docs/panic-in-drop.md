# `panic_in_drop`

**Level:** `warn`

Flags `Drop` implementations that contain operations which can panic, since panicking during unwinding causes an immediate abort.

## Why

If a panic occurs while the runtime is already unwinding from a previous panic, Rust aborts the process immediately — no destructors run, no cleanup happens:

- **Double-panic abort** — a `.unwrap()` inside `Drop` works fine in normal code paths, but if the value is dropped during stack unwinding from an unrelated panic, the process aborts with no backtrace or error message.
- **Silent data loss** — destructors that flush buffers, close connections, or write checkpoints silently fail to complete. The abort prevents any subsequent destructors from running too.
- **Extremely hard to debug** — the abort happens at the OS level. You get a signal (SIGABRT) with no indication of which `Drop` impl caused it or what the original panic was about.
- **Contagious** — one bad `Drop` impl can take down an entire server. Libraries that panic in `Drop` are unsafe to use in any context that catches panics (e.g. `catch_unwind`, thread pool workers, test harnesses).

The fix is to handle errors in `Drop` gracefully: log them, ignore them, or store them for later retrieval. If cleanup is critical and fallible, provide an explicit `close()` / `flush()` method and document that callers should call it before dropping.

### Relation to Clippy

Clippy does not have a lint for this. The general restriction lints `unwrap_used`, `expect_used`, and `panic` fire everywhere and are not `Drop`-specific. There is no lint that specifically targets the dangerous combination of panicking code inside a destructor.

## Flagged expressions

The lint fires when the body of a `Drop::drop` implementation contains any of:

| Expression | Notes |
|---|---|
| `.unwrap()` | On `Result` or `Option` |
| `.expect("...")` | On `Result` or `Option` |
| `panic!(...)` | Direct panic |
| `unreachable!(...)` | Logically equivalent to panic |
| `assert!(...)` | Panics on failure |
| `assert_eq!(...)` / `assert_ne!(...)` | Panics on failure |

## Examples

### Triggers

```rust
impl Drop for TempFile {
    fn drop(&mut self) {
        //~^ WARNING: panic-able expression in `Drop` impl — this will abort during unwinding
        std::fs::remove_file(&self.path).unwrap();
    }
}
```

```rust
impl Drop for Connection {
    fn drop(&mut self) {
        //~^ WARNING: panic-able expression in `Drop` impl — this will abort during unwinding
        self.socket.shutdown(Shutdown::Both).expect("shutdown failed");
    }
}
```

```rust
impl Drop for Checkpoint {
    fn drop(&mut self) {
        //~^ WARNING: panic-able expression in `Drop` impl — this will abort during unwinding
        assert!(self.flushed, "dropped without flushing");
    }
}
```

### Does not trigger

```rust
// Errors are silently ignored — safe during unwinding
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

// Errors are logged but not propagated
impl Drop for Connection {
    fn drop(&mut self) {
        if let Err(e) = self.socket.shutdown(Shutdown::Both) {
            eprintln!("warning: shutdown failed: {e}");
        }
    }
}

// Uses if-let instead of unwrap
impl Drop for Pool {
    fn drop(&mut self) {
        if let Some(handle) = self.thread.take() {
            handle.join().ok();
        }
    }
}

// Normal code — not inside Drop
impl Processor {
    fn process(&self) {
        let data = self.load().unwrap(); // fine here
    }
}
```

## Configuration

No configuration. The lint always fires on panic-able expressions inside `Drop::drop`.

## Implementation notes

### Lint pass

`LateLintPass::check_impl_item` — check if the enclosing `impl` block is an `impl Drop for T`. If so, match `ImplItemKind::Fn` where the method name is `drop`. Walk the function body with a `rustc_hir::intravisit::Visitor` to find:

1. Method calls to `unwrap` and `expect` (via `ExprKind::MethodCall`).
2. Macro invocations of `panic!`, `unreachable!`, `assert!`, `assert_eq!`, `assert_ne!` (via `ExprKind::Call` on items whose `DefId` resolves to the known macro paths, or by checking `span.ctxt()` for macro expansion from these macros).

### Detecting the Drop impl

Check the parent `impl` block's trait ref: resolve the trait `DefId` and compare against `core::ops::drop::Drop`. This ensures it works regardless of imports.

### Skip conditions

| Condition | Reason |
|---|---|
| `span.from_expansion()` on the impl item | Macro-generated Drop impls |
| Expressions inside `if std::thread::panicking()` guards | Already guarded against double-panic |

The `std::thread::panicking()` guard is a common pattern:

```rust
impl Drop for Guard {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            // Safe to panic here — we know we're not already unwinding
            self.check().unwrap();
        }
    }
}
```

### Diagnostic

```
warning: `.unwrap()` in `Drop` impl — this will abort if called during unwinding
  --> src/tempfile.rs:12:9
   |
12 |         std::fs::remove_file(&self.path).unwrap();
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: handle the error with `if let Err(e) = ...` or ignore it with `let _ = ...`
   = note: panicking in `drop()` while already unwinding causes an immediate process abort
```

### Relation to other lints

This pairs well with `fallible_new` — together they cover the two most dangerous places to panic: constructors (surprising callers) and destructors (aborting the process).
