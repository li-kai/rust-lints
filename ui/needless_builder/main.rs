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
