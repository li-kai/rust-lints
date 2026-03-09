# Global Side-Effect Lints

**Lints:** `global_side_effect::time`, `global_side_effect::randomness`, `global_side_effect::env`
**Level:** `warn`

Three lints that flag direct calls to non-deterministic or environment-coupled functions. Each targets a different dependency — wall-clock time, random number generation, or environment variables — but the architecture and fix are identical: **accept the dependency as a parameter** so callers can inject a testable, deterministic implementation.

## Why

Code that calls these functions directly is untestable (you can't control the inputs), non-deterministic (same inputs, different outputs), and hard to mock without thread-local hacks.

## Flagged calls

Each lint ships with built-in defaults covering common crates.

### `global_side_effect::time`

| Path | Notes |
|---|---|
| `std::time::SystemTime::now` | Wall-clock time |
| `std::time::Instant::now` | Monotonic clock |
| `chrono::Utc::now` | |
| `chrono::Local::now` | |
| `chrono::offset::Utc::now` | Re-export |
| `chrono::offset::Local::now` | Re-export |
| `time::OffsetDateTime::now_utc` | |
| `time::OffsetDateTime::now_local` | |
| `time::Instant::now` | |
| `jiff::Zoned::now` | |
| `jiff::Timestamp::now` | |
| `tokio::time::Instant::now` | |

### `global_side_effect::randomness`

| Path | Notes |
|---|---|
| `std::random::random` | Direct random value |
| `rand::thread_rng` | Thread-local RNG |
| `rand::random` | Wrapper around thread-local RNG |
| `rand::rngs::OsRng::new` | OS randomness |
| `rand::rngs::StdRng::from_os_rng` | Seeded from OS |
| `fastrand::u32` | Direct random value |
| `fastrand::u64` | Direct random value |
| `fastrand::Rng::new` | RNG from OS seed |

### `global_side_effect::env`

| Path | Notes |
|---|---|
| `std::env::var` | Read a single env var |
| `std::env::vars` | Iterate all env vars |
| `std::env::args` | Command-line arguments |
| `dotenvy::var` | Read from `.env` file |
| `dotenvy::vars` | Iterate `.env` vars |
| `dotenv::var` | Older dotenv crate |

## Examples

All three lints follow the same pattern. These examples use `global_side_effect::time`; substitute the relevant function for the other two.

### Triggers

```rust
use std::time::Instant;

fn is_expired(&self) -> bool {
    //~^ WARNING: direct call to `std::time::Instant::now()`
    Instant::now() > self.deadline
}
```

### Does not trigger

```rust
// Injected as a parameter
fn is_expired(&self, now: Instant) -> bool {
    now > self.deadline
}

// Trait-based injection
fn is_expired(&self, clock: &impl Clock) -> bool {
    clock.now() > self.deadline
}

// Inside #[test] or #[cfg(test)]
#[test]
fn test_something() {
    let start = Instant::now(); // ok
}

// In main() — the composition root
fn main() {
    let start = Instant::now(); // ok
    run(start);
}
```

## Suppression zones

The lint does not fire in these contexts:

| Context | Detection |
|---|---|
| Test crates | Integration test files (`tests/`), or the main crate compiled with `cargo test`. Detected via `--test` flag (`is_test_crate()`). Covers test helpers that don't carry `#[test]` themselves. |
| Test functions | Any function registered with the test harness (`#[test]`, `#[tokio::test]`, `#[async_std::test]`, `#[rstest]`, `#[test_case::test]`, `#[googletest::test]`, etc.) — detected via the compiler's `#[rustc_test_marker]` |
| `#[cfg(test)]` modules | Item is inside a `#[cfg(test)]` module |
| `fn main()` | Enclosing function is `main` |
| `#[allow(global_side_effect)]` | Standard rustc attribute |

## Configuration

All three lints accept the same two fields. Use the lint name as the TOML section key.

```toml
[global_side_effect::time]
# Extra paths to flag, merged with built-in defaults.
additional_paths = ["my_crate::util::current_time"]

# Replace built-in defaults entirely.
# paths = ["std::time::Instant::now", "chrono::Utc::now"]
```

| Field | Type | Default | Description |
|---|---|---|---|
| `additional_paths` | `Vec<String>` | `[]` | Extra paths to flag, merged with defaults |
| `paths` | `Option<Vec<String>>` | `None` | If set, replaces built-in defaults entirely |

## Implementation

All three lints use identical `LateLintPass::check_expr` structure:

1. Match `ExprKind::Call` and `ExprKind::MethodCall`.
2. Resolve the callee's `DefId`.
3. Compare against the configured path list via `clippy_utils::match_def_path`.
4. If matched, emit the lint.

### Diagnostic format

```
warning[global_side_effect::time]: direct call to `chrono::Utc::now()`
  --> src/billing.rs:42:15
   |
42 |     let now = Utc::now();
   |               ^^^^^^^^^^
   |
   = help: accept a time parameter or use a clock trait so callers can
           control the time source in tests
```

The help text varies by lint:

| Lint | Help |
|---|---|
| `global_side_effect::time` | accept a time parameter or use a clock trait |
| `global_side_effect::randomness` | accept an `impl Rng` parameter so callers can inject a seeded RNG |
| `global_side_effect::env` | move this to your application's entry point and pass the value as a parameter |
