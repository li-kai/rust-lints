// Test cases for the `acyclic_modules` lint.
#![allow(dead_code, unknown_lints, topological_ordering)]

// ── Mutual cycle at crate root: payments <-> server ──────────────

mod types {
    pub struct UserId(pub u64);
}

mod payments {
    pub mod checkout {
        pub fn process() {
            let _ = crate::server::auth::verify(); // payments → server
        }
    }

    pub mod billing {
        pub struct Invoice {
            pub amount: f64,
        }

        pub fn create_invoice() -> Invoice {
            Invoice { amount: 0.0 }
        }
    }
}

mod server {
    pub mod auth {
        pub fn verify() -> bool {
            // server → payments (creates cycle with payments → server above)
            let _ = crate::payments::billing::create_invoice();
            true
        }
    }

    pub mod routes {
        pub fn index() -> &'static str {
            "ok"
        }
    }
}

// ── Cycle at a deeper level: checkout <-> billing ────────────────

mod shop {
    pub mod checkout {
        pub struct CartItem {
            pub name: String,
        }

        pub fn finalize() {
            // checkout → billing
            let _ = crate::shop::billing::total();
        }
    }

    pub mod billing {
        pub fn total() -> f64 {
            // billing → checkout (cycle within shop)
            let _item = crate::shop::checkout::CartItem {
                name: String::new(),
            };
            0.0
        }
    }
}

// ── Multiple edges between same siblings: first span wins ────────

mod multi {
    pub mod alpha {
        pub fn first_ref() {
            let _ = crate::multi::beta::one(); // alpha → beta (first edge, reported)
        }

        pub fn second_ref() {
            let _ = crate::multi::beta::two(); // alpha → beta (duplicate, NOT reported)
        }
    }

    pub mod beta {
        pub fn one() -> u32 {
            crate::multi::alpha::first_ref(); // beta → alpha (first edge, reported)
            0
        }

        pub fn two() -> u32 {
            1
        }
    }
}

// ── No cycle: one-directional dependency ─────────────────────────

mod utils {
    pub fn helper() -> u64 {
        42
    }
}

mod consumer {
    pub fn use_helper() -> u64 {
        crate::utils::helper() // consumer → utils, no reverse edge
    }
}

// ── Test code should be excluded ─────────────────────────────────

#[cfg(test)]
mod tests {
    // Cross-module references in test code are always allowed.
    use crate::payments::billing::Invoice;
    use crate::server::auth::verify;

    fn _test_helper() -> (Invoice, bool) {
        (crate::payments::billing::create_invoice(), verify())
    }
}

fn main() {}
