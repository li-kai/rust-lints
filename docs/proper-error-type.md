# `proper_error_type`

**Level:** `warn`

Flags error types in public APIs that are incomplete, unstructured, or missing error chain information.

## Why

Error handling is a contract between a function and its callers. When that contract is incomplete:

- **Unstructured errors** — `String`, `&str`, and `Box<dyn Error>` discard type information. Callers cannot distinguish failure modes without parsing text, and any wording change silently breaks them.
- **Broken error chains** — manual `impl Error` often omits `source()`, preventing logging frameworks and `anyhow`/`eyre` reporters from walking the causal chain.
- **Duplicated sources** — when `Display` renders an inner error that `source()` also returns, error reporters print the same message twice. The [convention][std-error]: return it via `source()` *or* render it in `Display`, not both.
- **Misleading types** — a type named `FooError` that does not implement `std::error::Error` cannot be used with `Box<dyn Error>`, `?` conversion via `From`, or error reporters.
- **Avoidable boilerplate** — hand-written `Display` + `Error` impls drift out of sync with enum variants. `thiserror` eliminates this class of bug.

This lint does not enforce error naming conventions (e.g., `config::Error` vs. `config::ConfigError`). See the [Rust API Guidelines][api-naming] on module-name stuttering.

[std-error]: https://doc.rust-lang.org/std/error/trait.Error.html
[api-naming]: https://rust-lang.github.io/api-guidelines/naming.html

### Relation to Clippy

No existing Clippy lint covers this space:

- `clippy::result_unit_err` — flags `Result<T, ()>`, not structural problems with the error type.
- `clippy::result_large_err` — flags error types that are large by size, not by correctness.
- `clippy::error_impl_error` — flags types *named* `Error` that implement `Error` (naming ambiguity).
- `clippy::map_err_ignore` — catches `.map_err(|_| ...)`, a related but distinct pattern.

## Steps

### Step 1 — Unstructured error types

Flags public functions returning `Result<T, E>` where `E` is `String`, `&str`, `Cow<'_, str>`, or `Box<dyn Error>` (including `Box<dyn Error + Send + Sync>`).

Also flags `anyhow::Error` and `miette::Report` in effectively public signatures — items reachable from the crate root per `tcx.effective_visibilities()`. These types are acceptable in binaries and internal functions but not on library API surfaces.

```rust
// Triggers
pub fn parse(input: &str) -> Result<Config, String> { .. }
//~^ WARNING: public function returns `Result<_, String>`

// Triggers
pub fn run(cmd: &str) -> Result<(), Box<dyn Error>> { .. }
//~^ WARNING: public function returns `Result<_, Box<dyn Error>>`

// Triggers — anyhow/miette in an effectively public function
pub fn load(path: &Path) -> anyhow::Result<Config> { .. }
//~^ WARNING: effectively public function returns `anyhow::Error`
pub fn check(input: &str) -> miette::Result<()> { .. }
//~^ WARNING: effectively public function returns `miette::Report`
```

```rust
// OK — anyhow in a non-effectively-public function
pub fn helper() -> anyhow::Result<()> { .. }  // inside pub(crate) module

// OK — binary entry point
fn main() -> anyhow::Result<()> { .. }

// OK — typed error
pub fn parse(input: &str) -> Result<Config, ParseError> { .. }

// OK — private function
fn helper() -> Result<(), String> { .. }
```

### Step 2 — Missing `source()`

Flags manual `impl Error` blocks that do not override `source()` when the type has fields that implement `Error`.

```rust
// Triggers
pub enum ConfigError { Io(io::Error) }
impl std::error::Error for ConfigError {}
//~^ WARNING: `ConfigError` has error-typed fields but does not implement `source()`
```

```rust
// OK — source() implemented
impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self { Self::Io(e) => Some(e) }
    }
}

// OK — no fields that implement Error
pub enum ConfigError { MissingField(&'static str) }
impl std::error::Error for ConfigError {}

// OK — thiserror
#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("io failed")]
    Io(#[from] io::Error),
}
```

### Step 3 — Duplicated source in `Display`

Flags `Display` impls that render an inner error also returned by `source()`. Error reporters already print each `source()` level, so duplicating it in `Display` produces double output.

```rust
// Triggers — Display renders `e`, source() also returns `e`
impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self { Self::Io(e) => write!(f, "config error: {e}") }
        //~^ WARNING: inner error rendered in Display is also returned by source()
    }
}
impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self { Self::Io(e) => Some(e) }
    }
}
```

```rust
// OK — Display describes this level only
impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self { Self::Io(_) => write!(f, "failed to read config file") }
    }
}

// OK — thiserror
#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("failed to read config file")]
    Io(#[source] io::Error),
}
```

### Step 4 — Manual `Error` + `Display` — use `thiserror`

Flags types where both `Error` and `Display` are implemented by hand.

```rust
// Triggers
pub enum ConfigError {
    Parse(toml::de::Error),
    Io(io::Error),
}
impl std::fmt::Display for ConfigError { .. }
impl std::error::Error for ConfigError { .. }
//~^ WARNING: manual `Error` + `Display` impl — use `#[derive(thiserror::Error)]`
```

```rust
// OK — thiserror
#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("parse error")]
    Parse(#[from] toml::de::Error),
    #[error("io error")]
    Io(#[from] io::Error),
}
```

### Step 5 — `*Error` types without `Error` impl

Flags public types whose name ends in `Error` or `Err` that do not implement `std::error::Error`.

```rust
// Triggers
pub enum ParseError {
    //~^ WARNING: `ParseError` is named as an error type but does not implement `std::error::Error`
    InvalidSyntax,
    UnexpectedEof,
}

// Triggers
pub struct ConnectionError { pub message: String, pub code: u32 }
//~^ WARNING: `ConnectionError` does not implement `std::error::Error`
```

```rust
// OK — implements Error (via thiserror or manually)
#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    #[error("invalid syntax")]
    InvalidSyntax,
}

// OK — not named *Error
pub enum ParseProblem { InvalidSyntax }

// OK — private type
enum InternalError { Oops }
```

## Skip conditions

| Condition | Reason |
|---|---|
| `span.from_expansion()` | Macro-generated code |
| Non-`pub` functions (step 1) | Private functions may use `String` errors |
| Not effectively public + `anyhow`/`miette` (step 1) | Acceptable in binaries and internal code |
| Trait impl methods | Signature dictated by the trait |
| `#[cfg(test)]` modules | Test helpers commonly use informal error types |
| `fn main()` | Entry points commonly use `anyhow::Result` |
| `#[derive(thiserror::Error)]` (steps 2–5) | thiserror handles correctness |
| No fields implementing `Error` (step 2) | No source to chain |
| `#[error(transparent)]` (step 3) | Intentionally forwards both `Display` and `source()` |
| Non-`pub` types (step 5) | Private types do not form a public contract |

## Implementation notes

### Lint pass

**Step 1** — `check_fn` + `check_item`: resolve the return type to `core::result::Result`, extract `E`, check whether it resolves to `String` / `str` / `Cow<str>` / `Box<dyn Error>`. For `anyhow::Error` and `miette::Report`, only flag if `tcx.effective_visibilities()` marks the item as reachable from the crate root.

**Step 2** — `check_impl_item`: find `impl Error for T` blocks without `source()`. Inspect `T`'s fields via trait resolution — if any field implements `Error`, emit.

**Step 3** — `check_impl_item`: for types with both `Display` and `Error` impls, walk `Display::fmt`. If a format argument matches a field returned by `source()` (by `HirId`), emit. Skip `#[error(transparent)]`.

**Step 4** — `check_item`: find types with both manual `impl Display` and manual `impl Error`. Emit.

**Step 5** — `check_item`: find `pub` types named `*Error`/`*Err`. Check via trait resolution whether `Error` is implemented. If not, emit.

### Detection of thiserror

Check `#[derive(...)]` for `thiserror::Error` or `Error` with `use thiserror::Error` in scope. Types using thiserror are exempt from steps 2–4.

### Effective visibility (step 1)

Uses `tcx.effective_visibilities()` — the same mechanism as `unreachable_pub`. An item is effectively public if exported through a chain of `pub` modules from the crate root. Items that are `pub` inside a `pub(crate)` or `pub(super)` module are not effectively public and may use `anyhow::Error`.

### Diagnostics

**Step 1 — unstructured:**
```
warning: public function returns `Result<_, String>` — use a type that implements `Error`
  --> src/config.rs:5:40
   = help: define an error enum with `#[derive(thiserror::Error)]`
```

**Step 1 — anyhow/miette:**
```
warning: effectively public function returns `anyhow::Error` — use a typed error with `thiserror`
  --> src/lib.rs:12:35
   = help: define an error enum with `#[derive(thiserror::Error)]` for library API surfaces
   = note: `anyhow::Error` erases type information — callers cannot match on failure modes
```

**Step 2:**
```
warning: `ConfigError` wraps error types but does not implement `source()`
  --> src/error.rs:10:1
   = help: use `#[derive(thiserror::Error)]` with `#[source]` / `#[from]`
```

**Step 3:**
```
warning: `Display` renders inner error that is also returned by `source()`
  --> src/error.rs:18:38
   = help: describe only what went wrong at this level — the source chain handles the rest
```

**Step 4:**
```
warning: manual `Error` + `Display` impl — use `#[derive(thiserror::Error)]`
  --> src/error.rs:8:1
```

**Step 5:**
```
warning: `ParseError` is named as an error type but does not implement `std::error::Error`
  --> src/parser.rs:3:1
   = help: add `#[derive(thiserror::Error, Debug)]`
```
