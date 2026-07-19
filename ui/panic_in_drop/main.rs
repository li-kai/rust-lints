// Test cases for the `panic_in_drop` lint.
#![allow(dead_code, unknown_lints, debug_remnants, topological_ordering)]

// ── SHOULD TRIGGER ──────────────────────────────────────────────────

struct TempFile {
    path: std::path::PathBuf,
}

impl Drop for TempFile {
    //~v ERROR: panic-able expression in `Drop` impl
    fn drop(&mut self) {
        std::fs::remove_file(&self.path).unwrap();
    }
}

struct Connection {
    active: bool,
}

impl Drop for Connection {
    //~v ERROR: panic-able expression in `Drop` impl
    fn drop(&mut self) {
        assert!(self.active, "dropped inactive connection");
    }
}

struct Flusher {
    data: Option<Vec<u8>>,
}

impl Drop for Flusher {
    //~v ERROR: panic-able expression in `Drop` impl
    fn drop(&mut self) {
        let data = self.data.take().expect("already flushed");
        // simulate flush
        let _ = data;
    }
}

struct MultiPanic {
    x: i32,
}

impl Drop for MultiPanic {
    //~v ERROR: panic-able expression in `Drop` impl
    fn drop(&mut self) {
        panic!("oops");
    }
}

struct WithUnreachable;

impl Drop for WithUnreachable {
    //~v ERROR: panic-able expression in `Drop` impl
    fn drop(&mut self) {
        unreachable!("should never drop");
    }
}

struct WithAssertEq {
    val: i32,
}

impl Drop for WithAssertEq {
    //~v ERROR: panic-able expression in `Drop` impl
    fn drop(&mut self) {
        assert_eq!(self.val, 0);
    }
}

struct WithAssertNe {
    val: i32,
}

impl Drop for WithAssertNe {
    //~v ERROR: panic-able expression in `Drop` impl
    fn drop(&mut self) {
        assert_ne!(self.val, 42);
    }
}

struct IifeDrop;

impl Drop for IifeDrop {
    //~v ERROR: panic-able expression in `Drop` impl
    fn drop(&mut self) {
        (|| panic!("boom"))();
    }
}

// The `else` branch of an `if !std::thread::panicking()` guard runs ONLY while
// unwinding — exactly the double-panic-abort case. This SHOULD trigger, and does:
// the visitor still descends into the branches of a `!panicking()` guard, so the
// `self.val.unwrap()` in the `else` arm is flagged.
struct UnwindBranchPanic {
    val: Option<i32>,
}

impl Drop for UnwindBranchPanic {
    //~v ERROR: panic-able expression in `Drop` impl
    fn drop(&mut self) {
        if !std::thread::panicking() {
            // safe: not unwinding
        } else {
            self.val.unwrap();
        }
    }
}

// ── SHOULD NOT TRIGGER ──────────────────────────────────────────────

// Errors silently ignored — safe during unwinding
struct SafeTempFile {
    path: std::path::PathBuf,
}

impl Drop for SafeTempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

// Errors logged but not propagated
struct SafeConnection {
    active: bool,
}

impl Drop for SafeConnection {
    fn drop(&mut self) {
        if !self.active {
            eprintln!("warning: dropping inactive connection");
        }
    }
}

// Uses if-let instead of unwrap
struct SafePool {
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for SafePool {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().ok();
        }
    }
}

// Guarded by std::thread::panicking() — safe
struct GuardedDrop {
    val: Option<i32>,
}

impl Drop for GuardedDrop {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            // Safe: we know we're not already unwinding
            self.val.unwrap();
        }
    }
}

// Normal code — not inside Drop
struct Processor {
    data: Option<Vec<u8>>,
}

impl Processor {
    fn process(&self) {
        let _data = self.data.as_ref().unwrap(); // fine here
    }
}

// Panic inside a closure stored in a field — doesn't panic during drop itself
struct Deferred {
    callback: Option<Box<dyn FnOnce()>>,
}

impl Drop for Deferred {
    fn drop(&mut self) {
        if let Some(cb) = self.callback.take() {
            // We're storing the closure, not calling it in a panicky way.
            // The lint should not flag closures.
            let _ = cb;
        }
    }
}

// todo!/unimplemented! are intentional development placeholders — the
// compiler already surfaces them, so the lint never reports them
struct UnfinishedDrop;

impl Drop for UnfinishedDrop {
    fn drop(&mut self) {
        todo!()
    }
}

struct NotDoneDrop;

impl Drop for NotDoneDrop {
    fn drop(&mut self) {
        unimplemented!()
    }
}

// The with-args forms expand *through* `panic!`, so the exemption must clear
// the whole expansion backtrace, not just the innermost frame
struct UnfinishedWithArgsDrop;

impl Drop for UnfinishedWithArgsDrop {
    fn drop(&mut self) {
        todo!("cleanup for {:?}", "resource")
    }
}

// Macro-generated Drop impl should be skipped (not easily testable in UI,
// but noted for completeness)

fn main() {
    // Needed for UI test compilation
}
