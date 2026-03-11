#![allow(
    dead_code,
    unknown_lints,
    unused_variables,
    unused_must_use,
    clippy::allow_attributes_without_reason
)]
// Tests for the `unstructured_log_fields` lint.

// ══════════════════════════════════════════════════════════════════════
// Should trigger: all captures are positional format args, none are
// structured key-value fields.
// ══════════════════════════════════════════════════════════════════════

fn all_positional() {
    let user_id = 42;
    let path = "/api/v1";
    tracing::info!("user {} hit {}", user_id, path); //~ WARNING: unstructured
}

fn single_positional() {
    let count = 10;
    tracing::warn!("processed {} items", count); //~ WARNING: unstructured
}

fn debug_format_positional() {
    let req = "GET /";
    tracing::debug!("request: {:?}", req); //~ WARNING: unstructured
}

fn error_positional() {
    let code = 500;
    tracing::error!("failed with status {}", code); //~ WARNING: unstructured
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: already uses structured fields.
// ══════════════════════════════════════════════════════════════════════

fn fully_structured() {
    let user_id = 42;
    let path = "/api/v1";
    tracing::info!(user_id, path, "user hit endpoint"); // OK: structured
}

fn structured_with_debug() {
    let req = "GET /";
    tracing::debug!(?req, "incoming request"); // OK: ?field shorthand
}

fn structured_with_display() {
    let addr = "127.0.0.1";
    tracing::info!(%addr, "listening"); // OK: %field shorthand
}

fn key_value_explicit() {
    let user_id = 42;
    tracing::info!(user_id = user_id, "processing"); // OK: explicit key=value
}

fn mixed_structured_and_format() {
    let user_id = 42;
    let action = "login";
    // Has at least one structured field — don't fire.
    // (Partial structuring is an incremental improvement, not worth blocking on.)
    tracing::info!(user_id, "user performed {}", action); // OK: partially structured
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: bare message with no captures.
// ══════════════════════════════════════════════════════════════════════

fn bare_message() {
    tracing::info!("server started"); // OK: no values to structure
}

fn bare_message_with_literal() {
    tracing::info!("version: 1.0.0"); // OK: literal text, nothing to capture
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: non-tracing macros (log crate, println, etc.)
// ══════════════════════════════════════════════════════════════════════

fn log_crate_info() {
    // The `log` crate doesn't support structured fields — nothing to suggest.
    // log::info!("user {}", user_id);  // would be OK
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: inside test zones.
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    #[test]
    fn test_logging() {
        tracing::info!("got {}", 42); // OK: in test
    }
}

// ══════════════════════════════════════════════════════════════════════
// Should NOT trigger: suppressed with #[allow].
// ══════════════════════════════════════════════════════════════════════

#[allow(unstructured_log_fields)]
fn allowed_positional() {
    tracing::info!("user {}", 42); // OK: explicitly allowed
}

fn main() {}
