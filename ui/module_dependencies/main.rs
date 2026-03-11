// Test cases for the `module_dependencies` lint.
//
// Config (in dylint.toml):
//   types = []
//   errors = ["types"]
//   utils = ["types", "errors"]
//   payments = ["types", "errors", "utils"]
//   server = ["types", "errors", "utils"]
#![allow(dead_code, unknown_lints, proper_error_type)]

// ── Module definitions ─────────────────────────────────────────────

mod types {
    pub struct UserId(pub u64);
    pub struct Amount(pub f64);
}

mod errors {
    use crate::types::UserId; // OK: errors → types is allowed

    pub struct AppError {
        pub context: String,
        pub user: Option<UserId>,
    }
}

mod utils {
    use crate::types::UserId; // OK: utils → types is allowed

    pub fn validate_id(id: &UserId) -> bool {
        id.0 > 0
    }

    pub fn format_error(err: &crate::errors::AppError) -> String {
        // OK: utils → errors is allowed
        err.context.clone()
    }
}

mod payments {
    use crate::types::UserId; // OK: payments → types is allowed
    use crate::utils::validate_id; // OK: payments → utils is allowed

    pub struct Order {
        pub user: UserId,
        pub total: crate::types::Amount, // OK: payments → types is allowed
    }

    pub fn create_order(user: UserId) -> Order {
        let _ = validate_id(&user);
        Order {
            user,
            total: crate::types::Amount(0.0),
        }
    }

    // SHOULD TRIGGER: payments → server is not allowed
    pub fn bad_dependency() -> crate::server::Session {
        crate::server::Session { active: true }
    }
}

mod server {
    pub struct Session {
        pub active: bool,
    }

    // SHOULD TRIGGER: server → payments is not allowed
    pub fn bad_dependency() -> crate::payments::Order {
        crate::payments::Order {
            user: crate::types::UserId(1), // OK: server → types is allowed
            total: crate::types::Amount(0.0),
        }
    }
}

// ── Test code should be excluded ───────────────────────────────────

#[cfg(test)]
mod tests {
    // Cross-module references in test code are always allowed.
    use crate::payments::Order;
    use crate::server::Session;

    fn _test_helper() -> (Order, Session) {
        let order = crate::payments::create_order(crate::types::UserId(1));
        let session = crate::server::Session { active: true };
        (order, session)
    }
}

fn main() {}
