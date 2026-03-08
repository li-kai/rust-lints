#![allow(dead_code, unknown_lints, clippy::allow_attributes_without_reason)]
// Tests for the `suggest_builder` lint.
// Threshold: 4 (from dylint.toml).

// Should trigger: 4 named fields, no builder derive.
struct Config {
    host: String,
    port: u16,
    timeout: u32,
    retries: u8,
}

// Should trigger: 5 named fields, no builder derive.
struct LargerConfig {
    host: String,
    port: u16,
    timeout: u32,
    retries: u8,
    verbose: bool,
}

// Should NOT trigger: has `#[derive(bon::Builder)]`.
#[derive(bon::Builder)]
struct WithBuilder {
    host: String,
    port: u16,
    timeout: u32,
    retries: u8,
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
