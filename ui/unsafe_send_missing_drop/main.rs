// Test cases for the `unsafe_send_missing_drop` lint.
#![allow(
    dead_code,
    unused,
    unsafe_code,
    unknown_lints,
    topological_ordering,
    clippy::non_send_fields_in_send_ty
)]

use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;
use std::rc::Rc;

// ── SHOULD TRIGGER ──────────────────────────────────────────────────

// Basic case: !Send field, unsafe impl Send, no Drop.
//~v WARNING: has `unsafe impl Send` but contains `!Send` fields and no `Drop` impl
struct Handle {
    inner: UnsafeCell<Rc<String>>,
}
unsafe impl Send for Handle {}

// Multiple !Send fields, still no Drop.
//~v WARNING: has `unsafe impl Send` but contains `!Send` fields and no `Drop` impl
struct Multi {
    a: Rc<u32>,
    b: *mut u8,
}
unsafe impl Send for Multi {}

// Nested wrapper: Option<Rc<T>> is !Send because Rc<T> is !Send.
//~v WARNING: has `unsafe impl Send` but contains `!Send` fields and no `Drop` impl
struct Nested {
    inner: Option<Rc<String>>,
}
unsafe impl Send for Nested {}

// Unbounded generic: `unsafe impl<T> Send` promises Send for ALL T,
// including T: !Send. The implicit drop of T would be unsound.
//~v WARNING: has `unsafe impl Send` but contains `!Send` fields and no `Drop` impl
struct Generic<T> {
    value: T,
}
unsafe impl<T> Send for Generic<T> {}

// Also with Sync — having Sync doesn't help with Drop.
//~v WARNING: has `unsafe impl Send` but contains `!Send` fields and no `Drop` impl
struct WithSync {
    inner: UnsafeCell<Rc<String>>,
}
unsafe impl Send for WithSync {}
unsafe impl Sync for WithSync {}

// ── SHOULD NOT TRIGGER ──────────────────────────────────────────────

// Has a Drop impl — author has taken responsibility for destruction.
struct WithDrop {
    inner: UnsafeCell<Rc<String>>,
}
unsafe impl Send for WithDrop {}
impl Drop for WithDrop {
    fn drop(&mut self) {
        // Dispatches destruction correctly.
    }
}

// No !Send fields — all fields are Send, implicit drop is fine.
struct AllSend {
    data: String,
    count: usize,
}
unsafe impl Send for AllSend {}

// ManuallyDrop suppresses implicit destruction — the field won't be
// dropped, so the lint should not count it as problematic.
struct WithManuallyDrop {
    inner: ManuallyDrop<Rc<String>>,
}
unsafe impl Send for WithManuallyDrop {}

// PhantomData<T> — no real value to drop.
struct WithPhantom<T> {
    _marker: std::marker::PhantomData<T>,
    data: String,
}
unsafe impl<T> Send for WithPhantom<T> {}

// Safe impl Send (via auto-trait) — not our concern, only unsafe impls.
// (Can't actually write `impl Send for X` without unsafe, so this case
// is a struct that just naturally implements Send — no lint needed.)
struct NaturallySend {
    data: String,
}

// Raw pointer field, but the struct has a Drop impl.
struct RawPtrWithDrop {
    ptr: *mut u8,
}
unsafe impl Send for RawPtrWithDrop {}
impl Drop for RawPtrWithDrop {
    fn drop(&mut self) {}
}

fn main() {}
