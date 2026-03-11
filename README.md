# rust-lints

Custom Rust lints via the [dylint](https://github.com/trailofbits/dylint) ecosystem.

## Lints

| Lint | Level | Description |
|------|-------|-------------|
| [`blocking_in_async`](#blocking_in_async) | deny | Blocking operations inside `async fn` or `async {}` blocks |
| [`debug_remnants`](#debug_remnants) | warn | Debug macros (`println!`, `eprintln!`, `dbg!`) in non-test code |
| [`fallible_new`](#fallible_new) | deny | `fn new()` constructors that can panic |
| [`global_side_effect::env`](#global_side_effect) | warn | Direct calls to `std::env::var` and similar outside `main()` |
| [`global_side_effect::logging_init`](#global_side_effect) | deny | Global logger initialization outside `main()` |
| [`global_side_effect::randomness`](#global_side_effect) | warn | Direct calls to random number generators outside `main()` and tests |
| [`global_side_effect::time`](#global_side_effect) | warn | Direct calls to wall-clock or monotonic time outside `main()` and tests |
| [`map_init_then_insert`](#map_init_then_insert) | warn | `HashMap`/`BTreeMap`/`IndexMap` created empty then immediately populated with `insert()` |
| [`needless_builder`](#needless_builder) | warn | Structs with ≤ 2 named fields that unnecessarily derive `bon::Builder` |
| [`panic_in_drop`](#panic_in_drop) | deny | Panic-able expressions inside `Drop` implementations |
| [`proper_error_type`](#proper_error_type) | warn | Incomplete or unstructured error types in public APIs |
| [`result_result`](#result_result) | warn | Nested `Result<Result<T, E1>, E2>` in function signatures |
| [`suggest_builder`](#suggest_builder) | warn | Structs with ≥ 4 named fields that don't derive `bon::Builder` |
| [`suggest_fn_builder`](#suggest_fn_builder) | warn | Functions with many parameters that could use `#[bon::builder]` |
| [`unbounded_channel`](#unbounded_channel) | deny | Creation of unbounded channels that can exhaust memory |

---

### `blocking_in_async`

Flags known-blocking operations inside `async fn` or `async {}` blocks. Suggests using async-aware alternatives or `spawn_blocking` instead.

```
warning: blocking call to `std::fs::read_to_string()` inside async function
  --> src/loader.rs:12:13
   |
12 |     let data = std::fs::read_to_string(path)?;
   |               ^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: use `tokio::fs::read_to_string()` instead
           or wrap the blocking call in `tokio::task::spawn_blocking()`
```

Flagged by default: `std::fs::read/write/read_dir/metadata/canonicalize`, `std::io::stdin().read*`, `std::net::TcpStream::connect`, `std::thread::sleep`, `std::sync::Mutex::lock`, `std::sync::RwLock::read/write`, `parking_lot::Mutex::lock`, `parking_lot::RwLock::read/write`, `tokio::task::block_in_place`.

Does not fire inside `#[test]` / `#[tokio::test]` or `tokio::task::spawn_blocking`.

### `debug_remnants`

Flags `println!`, `print!`, `eprintln!`, and `dbg!` outside test code. Suggests structured logging replacements (`tracing` or `log`).

```
warning: debug remnant in committed code
  --> src/api.rs:42:5
   |
42 |     println!("request: {:?}", req);
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: replace with `tracing::info!(?req, "incoming request")`
```

Does not fire inside `#[test]` functions or `#[cfg(test)]` modules. Supersedes `clippy::dbg_macro`, `clippy::print_stdout`, and `clippy::print_stderr` with actionable replacements and unified configuration.

### `fallible_new`

Warns when a `fn new()` constructor contains `.unwrap()`, `.expect()`, `panic!`, `unreachable!`, `todo!`, or `unimplemented!`. These can abort the program in cases the caller cannot handle.

```
warning: constructor `new` can panic — consider returning `Result` or renaming to `try_new`
  --> src/config.rs:8:5
   |
 8 |     pub fn new(path: &str) -> Self {
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: `.unwrap()` at src/config.rs:9:55 can panic — use `?` with a `Result` return type instead
```

Does not fire when the return type is already `Result`, when the constructor is private, or inside trait impls.

### `global_side_effect`

Four lints that flag direct calls to non-deterministic or environment-coupled functions. The fix for `time`, `randomness`, and `env` is to accept the dependency as a parameter. The fix for `logging_init` is to move initialization to `main()`.

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

None of the four lints fire inside `#[test]` functions, `#[cfg(test)]` modules, or `fn main()`.

**`global_side_effect::time`** — flags: `std::time::SystemTime::now`, `std::time::Instant::now`, `chrono::Utc::now`, `chrono::Local::now`, `time::OffsetDateTime::now_utc`, `jiff::Zoned::now`, `tokio::time::Instant::now`, and more.

**`global_side_effect::randomness`** — flags: `rand::thread_rng`, `rand::random`, `rand::rngs::OsRng::new`, `fastrand::Rng::new`, and more.

**`global_side_effect::env`** — flags: `std::env::var`, `std::env::vars`, `std::env::args`, `dotenvy::var`, `dotenvy::vars`, `dotenv::var`.

**`global_side_effect::logging_init`** — flags: `tracing_subscriber::fmt::init`, `env_logger::init`, `log::set_logger`, `fern::Dispatch::apply`, `simplelog::TermLogger::init`, and more.

### `map_init_then_insert`

Warns when a `HashMap`, `BTreeMap`, or `IndexMap` is created empty and then immediately populated with two or more sequential `.insert()` calls. Suggests `::from([...])` instead.

```
warning: immediately inserting into a newly created map — consider using `HashMap::from([..])`
  --> src/config.rs:12:5
   |
12 | /   let mut m = HashMap::new();
13 | |   m.insert("a", 1);
14 | |   m.insert("b", 2);
15 | |   m.insert("c", 3);
   | |________________________^
   |
   = help: use `let m = HashMap::from([..])` to initialize the map inline
```

Does not fire when there is intervening control flow, reads, or borrows between creation and the insert sequence, or when there is only one insert. Complements Clippy's `vec_init_then_push`.

### `needless_builder`

Warns when `bon::Builder` is derived on a struct with very few fields.

```
warning: struct `Point` has only 2 fields; `bon::Builder` may be unnecessary
  --> src/lib.rs:5:1
   = help: consider using a plain constructor or struct literal instead
```

### `panic_in_drop`

Flags `.unwrap()`, `.expect()`, `panic!`, `unreachable!`, `assert!`, `assert_eq!`, and `assert_ne!` inside `Drop` implementations. Panicking during unwinding causes an immediate process abort with no cleanup.

```
warning: `.unwrap()` in `Drop` impl — this will abort if called during unwinding
  --> src/tempfile.rs:12:9
   |
12 |         std::fs::remove_file(&self.path).unwrap();
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: handle the error with `if let Err(e) = ...` or ignore it with `let _ = ...`
   = note: panicking in `drop()` while already unwinding causes an immediate process abort
```

Does not fire on macro-generated `Drop` impls or inside `if !std::thread::panicking()` guards.

### `proper_error_type`

Flags error types in public APIs that are incomplete, unstructured, or missing error chain information. Fires in five cases:

1. Public functions returning `Result<T, String>`, `Result<T, &str>`, `Result<T, Box<dyn Error>>`, or `anyhow::Error`/`miette::Report` on effectively-public surfaces.
2. Manual `impl Error` blocks missing `source()` when the type wraps other errors.
3. `Display` impls that render an inner error also returned by `source()` (double-printing).
4. Types with both manual `impl Display` and `impl Error` (use `thiserror` instead).
5. Public types named `*Error` or `*Err` that don't implement `std::error::Error`.

```
warning: public function returns `Result<_, String>` — use a type that implements `Error`
  --> src/config.rs:5:40
   = help: define an error enum with `#[derive(thiserror::Error)]`
```

### `result_result`

Flags `Result<Result<T, E1>, E2>` in function signatures and type aliases. Nested results force callers into awkward double-matching and usually indicate `.map()` where `.and_then()` was intended.

```
warning: nested `Result<Result<_, _>, _>` — consider flattening into a single Result
  --> src/loader.rs:5:34
   |
 5 | fn load(path: &str) -> Result<Result<Config, toml::de::Error>, io::Error> {
   |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: use `.and_then()` to chain fallible operations, or unify the error
           types into a single enum
```

Complements Clippy's `option_option` (pedantic), which catches `Option<Option<T>>`.

### `suggest_builder`

Suggests adding `#[derive(bon::Builder)]` to structs with many named fields.

```
warning: struct `Config` has 5 fields but does not derive `bon::Builder`
  --> src/lib.rs:10:1
   = help: add `#[derive(bon::Builder)]` to enable the builder pattern
```

### `suggest_fn_builder`

Suggests adding `#[bon::builder]` to functions with many parameters to enable named arguments at call sites. Similar to Clippy's `too_many_arguments` but with a lower default threshold and a concrete fix.

```
warning: function `connect` has 5 parameters — consider adding `#[bon::builder]`
  --> src/net.rs:10:1
   |
10 | fn connect(host: &str, port: u16, timeout: u64, retries: u32, tls: bool) {
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: add `#[bon::builder]` to enable named parameters at call sites
```

Does not fire on functions that already have `#[bon::builder]`, trait impl methods, or extern functions.

### `unbounded_channel`

Flags creation of unbounded channels, which can cause memory exhaustion under backpressure.

```
warning: unbounded channel created — can exhaust memory under backpressure
  --> src/logger.rs:42:29
   |
42 |     let (tx, rx) = mpsc::unbounded_channel();
   |                         ^^^^^^^^^^^^^^^^^^
   |
   = help: use `mpsc::channel(capacity)` instead with an explicit bound
           (e.g., `channel(1000)`) to enable backpressure
```

Flagged by default: `std::sync::mpsc::channel`, `tokio::sync::mpsc::unbounded_channel`, `flume::unbounded`, `crossbeam::channel::unbounded`.

Does not fire inside `#[test]` / `#[tokio::test]`, `#[cfg(test)]` modules, or `fn main()`.

---

## Usage

Add to your workspace `Cargo.toml`:

```toml
[workspace.metadata.dylint]
libraries = [
    { git = "https://github.com/li-kai/rust-lints" },
]
```

Configure thresholds and options in `dylint.toml`:

```toml
[suggest_builder]
threshold = 4

[needless_builder]
threshold = 2

[suggest_fn_builder]
threshold = 4

[fallible_new]
check_new_variants = true

[debug_remnants]
suggested_strategy = "tracing"  # or "log" for libraries
allow_in_tests = true
allow_in_test_modules = true

[unbounded_channel]
# additional_paths = ["my_app::channels::create_unbounded"]

[blocking_in_async]
# additional_paths = ["my_lib::database::connect_blocking"]

[global_side_effect::time]
# additional_paths = ["my_crate::util::current_time"]

[global_side_effect::randomness]
# additional_paths = []

[global_side_effect::env]
# additional_paths = []

[global_side_effect::logging_init]
# additional_paths = []
```

## Development

Requires `dylint-link`:

```sh
cargo install dylint-link
```

Build and test:

```sh
just check-all
```
