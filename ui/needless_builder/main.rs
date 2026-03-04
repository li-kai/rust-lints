#![allow(dead_code, unknown_lints)]
// Tests for the `needless_builder` lint.
// Threshold: 2 (from dylint.toml).

// Should trigger: 1 field with builder derive.
#[derive(bon::Builder)]
struct Singleton {
    value: u32,
}

// Should trigger: 2 fields with builder derive.
#[derive(bon::Builder)]
struct Point {
    x: f64,
    y: f64,
}

// Should NOT trigger: 3 fields (above threshold).
#[derive(bon::Builder)]
struct Triple {
    a: u8,
    b: u8,
    c: u8,
}

// Should NOT trigger: 2 fields without builder derive.
struct Pair {
    x: f64,
    y: f64,
}

// Should NOT trigger: suppressed with `#[allow]`.
#[allow(needless_builder)]
#[derive(bon::Builder)]
struct Suppressed {
    only: u8,
}

fn main() {}

// ── Name-collision limitation ──
// The pre-expansion collector matches by name only.  If *any* struct named
// `Collider` has `#[derive(bon::Builder)]`, all structs named `Collider`
// are considered to have it – a known false positive for needless_builder.
mod inner {
    #[derive(bon::Builder)]
    pub struct Collider {
        a: u8,
        b: u8,
        c: u8,
    }
}

// Known false positive: this `Collider` does NOT derive Builder, but the
// name-only lookup sees `inner::Collider`'s derive and fires the lint.
struct Collider {
    x: f64,
    y: f64,
}
