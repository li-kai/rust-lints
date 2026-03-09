#![allow(
    dead_code,
    unknown_lints,
    unused_variables,
    clippy::allow_attributes_without_reason
)]
// Tests for the `global_side_effect` lint group.

use std::time::Instant;

// ══════════════════════════════════════════════════════════════════════
// Should trigger: global_side_effect_time
// ══════════════════════════════════════════════════════════════════════

fn check_deadline() -> bool {
    let now = Instant::now(); //~ WARNING: direct call to `std::time::Instant::now()`
    now.elapsed().as_secs() > 10
}

fn get_system_time() -> std::time::SystemTime {
    std::time::SystemTime::now() //~ WARNING: direct call to `std::time::SystemTime::now()`
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: global_side_effect_env
// ══════════════════════════════════════════════════════════════════════

fn read_config() -> Option<String> {
    std::env::var("MY_CONFIG").ok() //~ WARNING: direct call to `std::env::var()`
}

fn get_all_env() {
    let _vars: Vec<_> = std::env::vars().collect(); //~ WARNING: direct call to `std::env::vars()`
}

fn get_args() {
    let _args: Vec<_> = std::env::args().collect(); //~ WARNING: direct call to `std::env::args()`
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: suppression zones
// ══════════════════════════════════════════════════════════════════════

// fn main() is a suppression zone.
fn main() {
    let _now = Instant::now(); // OK: in main
    let _var = std::env::var("HOME"); // OK: in main
}

// #[cfg(test)] is a suppression zone.
#[cfg(test)]
mod tests {
    use std::time::Instant;

    #[test]
    fn test_timing() {
        let _now = Instant::now(); // OK: in #[cfg(test)]
    }
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: value passed as parameter (no call to flagged fn)
// ══════════════════════════════════════════════════════════════════════

fn is_expired(now: Instant, deadline: Instant) -> bool {
    now > deadline // OK: time injected as parameter
}
