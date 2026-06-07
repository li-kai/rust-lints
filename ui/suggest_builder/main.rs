#![allow(
    dead_code,
    unknown_lints,
    clippy::allow_attributes_without_reason,
    topological_ordering
)]
use std::marker::PhantomData;

use bon::bon;
// Tests for the `suggest_builder` lint.
// Threshold: 4 (from dylint.toml).

// Should trigger: 4 named fields, no builder derive, has constructor.
struct Config {
    host: String,
    port: u16,
    timeout: u32,
    retries: u8,
}
impl Config {
    fn new(host: String, port: u16, timeout: u32, retries: u8) -> Self {
        Self { host, port, timeout, retries }
    }
}

// Should trigger: 5 named fields, no builder derive, has constructor.
struct LargerConfig {
    host: String,
    port: u16,
    timeout: u32,
    retries: u8,
    verbose: bool,
}
impl LargerConfig {
    fn new(host: String, port: u16, timeout: u32, retries: u8, verbose: bool) -> Self {
        Self { host, port, timeout, retries, verbose }
    }
}

// Should NOT trigger: has `#[derive(bon::Builder)]`.
#[derive(bon::Builder)]
struct WithBuilder {
    host: String,
    port: u16,
    timeout: u32,
    retries: u8,
}

// Should NOT trigger: has a `#[bon] impl` with a `#[builder]` constructor.
struct User {
    id: u32,
    name: String,
}

#[bon]
impl User {
    #[builder]
    fn new(id: u32, name: String) -> Self {
        Self { id, name }
    }
}

// Should NOT trigger: 3 fields (below threshold).
struct Small {
    host: String,
    port: u16,
    timeout: u32,
}

// Should NOT trigger: tuple struct.
struct Coords(f64, f64, f64, f64);

// Should NOT trigger: unit struct.
struct Marker;

// Should NOT trigger: derives Default (in skip_derives).
#[derive(Default)]
struct DefaultConfig {
    host: String,
    port: u16,
    timeout: u32,
    retries: u8,
}

// Should NOT trigger: manual Default impl.
struct ManualDefaultConfig {
    host: String,
    port: u16,
    timeout: u32,
    retries: u8,
}
impl Default for ManualDefaultConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 8080,
            timeout: 30,
            retries: 3,
        }
    }
}

// Should NOT trigger: struct name ends with `Builder`.
struct ConnectionBuilder {
    host: String,
    port: u16,
    timeout: u32,
    retries: u8,
}

// Should NOT trigger: has lifetime parameters (borrowed view / visitor).
struct Visitor<'a> {
    context: &'a str,
    items: &'a [u8],
    count: usize,
    done: bool,
}

// Should NOT trigger: `#[repr(C)]` FFI struct.
#[repr(C)]
struct FfiPoint {
    x: f64,
    y: f64,
    z: f64,
    w: f64,
}

// Should NOT trigger: PhantomData fields don't count (3 real + 1 phantom = below threshold).
struct TypedHandle<T> {
    id: u64,
    generation: u32,
    flags: u8,
    _marker: PhantomData<T>,
}

// Should trigger: 4 real fields even with PhantomData (4 real + 2 phantom), has constructor.
struct TypedContainer<T, U> {
    name: String,
    capacity: usize,
    items: Vec<u8>,
    label: String,
    _t: PhantomData<T>,
    _u: PhantomData<fn(U)>,
}
impl<T, U> TypedContainer<T, U> {
    fn new(name: String, capacity: usize, items: Vec<u8>, label: String) -> Self {
        Self { name, capacity, items, label, _t: PhantomData, _u: PhantomData }
    }
}

// Should NOT trigger: no constructors (no inherent fn returning Self).
struct InternalRecord {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
}

// Should NOT trigger: has no real constructor — only a getter returning
// `Option<&Self>`. Per the documented contract (a constructor is an inherent
// fn returning `Self`, `Result<Self, _>`, or `Box<Self>`) this struct has no
// constructor. `has_ctor` peels only `Result`/`Box` and then requires an exact
// `Self` match, so a return type that merely *contains* `Self` (here
// `Option<&Self>`) is correctly not treated as a constructor.
struct ListNode {
    value: u64,
    weight: u32,
    flags: u8,
    label: String,
}
impl ListNode {
    fn next(&self) -> Option<&Self> {
        None
    }
}

// Should NOT trigger: suppressed with `#[allow]`.
#[allow(suggest_builder)]
struct Suppressed {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
}

fn main() {}

// ── Name-collision limitation ──
// The pre-expansion collector matches by name only.  If *any* struct named
// `Collider` has `#[derive(bon::Builder)]`, all structs named `Collider`
// are considered to have it – a known false negative for suggest_builder.
mod inner {
    #[derive(bon::Builder)]
    pub struct Collider {
        a: u8,
        b: u8,
    }
}

// Known false negative: this `Collider` does NOT derive Builder, but the
// name-only lookup sees `inner::Collider`'s derive and suppresses the lint.
struct Collider {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
}
