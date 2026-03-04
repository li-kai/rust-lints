#![allow(dead_code, unknown_lints)]
// Tests for the `large_struct` lint.
// Threshold: 12 (from dylint.toml).

// Should trigger: 13 fields.
struct MegaConfig {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8,
    g: u8,
    h: u8,
    i: u8,
    j: u8,
    k: u8,
    l: u8,
    m: u8,
}

// Should trigger: exactly 12 fields (>= threshold).
struct ExactThreshold {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8,
    g: u8,
    h: u8,
    i: u8,
    j: u8,
    k: u8,
    l: u8,
}

// Should NOT trigger: 11 fields (below threshold).
struct JustBelow {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8,
    g: u8,
    h: u8,
    i: u8,
    j: u8,
    k: u8,
}

// Should NOT trigger: suppressed with `#[allow]`.
#[allow(large_struct)]
struct Suppressed {
    a: u8, b: u8, c: u8, d: u8, e: u8, f: u8,
    g: u8, h: u8, i: u8, j: u8, k: u8, l: u8, m: u8,
}

fn main() {}
