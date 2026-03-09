#![allow(
    dead_code,
    unknown_lints,
    unused_variables,
    unused_must_use,
    clippy::allow_attributes_without_reason
)]
// Tests for the `debug_remnants` lint.

// ══════════════════════════════════════════════════════════════════════
// Should trigger: println! in regular function
// ══════════════════════════════════════════════════════════════════════

fn process_payment(amount: u64) {
    println!("Processing ${}", amount); //~ WARNING: debug remnant
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: eprintln! in regular function
// ══════════════════════════════════════════════════════════════════════

fn handle_request(req: &str) {
    eprintln!("Got request: {}", req); //~ WARNING: debug remnant
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: dbg! in regular function
// ══════════════════════════════════════════════════════════════════════

fn validate(config: &str) -> bool {
    dbg!(config); //~ WARNING: debug remnant
    config.len() > 0
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: print! in regular function
// ══════════════════════════════════════════════════════════════════════

fn show_header() {
    print!("debug: "); //~ WARNING: debug remnant
}

// ══════════════════════════════════════════════════════════════════════
// Should trigger: println! in fn main() — NO exemption
// ══════════════════════════════════════════════════════════════════════

fn main() {
    println!("Starting server"); //~ WARNING: debug remnant
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: inside #[cfg(test)] module
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    #[test]
    fn test_payment() {
        println!("Testing payment logic"); // OK: in #[test]
    }

    fn setup() {
        eprintln!("Setting up test environment"); // OK: in #[cfg(test)]
    }
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: suppressed with #[allow]
// ══════════════════════════════════════════════════════════════════════

#[allow(debug_remnants)]
fn cli_entrypoint() {
    println!("Usage: tool <command>"); // OK: explicitly suppressed
}
