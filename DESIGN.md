Create a dylint lint library crate called `rust-lints`. This is a reusable `cdylib` crate that provides custom Rust lints via the dylint ecosystem. Users consume it by adding it to their `[workspace.metadata.dylint]` and can enable/disable individual lints with standard `#[allow(...)]` / `#[warn(...)]` attributes.

## Setup

- dylint v5.0.0 (`dylint_linting = "5.0.0"`, `dylint_testing = "5.0.0"`)
- `clippy_utils` pinned to the git rev matching the nightly toolchain
- `crate-type = ["cdylib"]`
- `.cargo/config.toml` with `linker = dylint-link`
- `rust-toolchain` pinned to a recent nightly with `rustc-dev` and `llvm-tools-preview` components
- `serde` dependency for lint configuration via `dylint.toml`
- `[package.metadata.rust-analyzer] rustc_private = true`

## Architecture

Multiple lints bundled into a single `cdylib`. Use `dylint_linting::dylint_library!()` in `lib.rs` with a manual `register_lints` function that registers all lints. Each lint lives in its own module under `src/lints/`.

Each lint module should use `dylint_linting`'s `constituent` feature so it can be bundled without emitting its own `dylint_library!()`.

```
rust-lints/
  .cargo/config.toml
  rust-toolchain
  Cargo.toml
  dylint.toml              # default config values for testing
  src/
    lib.rs                 # dylint_library!(), register_lints, mod declarations
    config.rs              # shared config loading
    lints/
      mod.rs
      suggest_builder.rs   # suggest bon::Builder for large structs
      needless_builder.rs  # warn when bon::Builder is on small structs
      large_struct.rs      # warn when structs exceed a field count limit
  ui/
    suggest_builder/
      main.rs
      main.stderr
    needless_builder/
      main.rs
      main.stderr
    large_struct/
      main.rs
      main.stderr
```

## Lints to implement

### 1. `suggest_builder` (Warn)

Suggests adding `#[derive(bon::Builder)]` to structs with many named fields.

**Triggers when:** A struct has `>= threshold` named fields (default: 4) and does not have `#[derive(bon::Builder)]`, `#[derive(Builder)]`, or `#[bon::builder]` attribute.

**Skips:** Tuple structs, unit structs, macro-expanded structs (`item.span.from_expansion()`).

**Config** (via `dylint.toml`):
```toml
[suggest_builder]
threshold = 4
```

**Implementation:** `LateLintPass::check_item`, match `ItemKind::Struct` with `VariantData::Struct`, count `fields.len()`, check attrs via `cx.tcx.hir().attrs(item.hir_id())` for derive attributes containing `bon::Builder` or `Builder` in the token stream.

**Message:**
```
warning: struct `Config` has 5 fields but does not derive `bon::Builder`
  --> src/lib.rs:10:1
   = help: add `#[derive(bon::Builder)]` to enable the builder pattern
```

### 2. `needless_builder` (Warn)

The inverse — warns when `bon::Builder` is derived on a struct with very few fields where a builder adds ceremony without value.

**Triggers when:** A struct has `<= threshold` named fields (default: 2) and has `#[derive(bon::Builder)]` or `#[derive(Builder)]`.

**Config:**
```toml
[needless_builder]
threshold = 2
```

**Message:**
```
warning: struct `Point` has only 2 fields; `bon::Builder` may be unnecessary
  --> src/lib.rs:5:1
   = help: consider using a plain constructor or struct literal instead
```

### 3. `large_struct` (Warn)

Warns when a struct has an excessive number of fields, suggesting it should be split into smaller types.

**Triggers when:** A struct has `>= threshold` named fields (default: 12). Independent of builder usage.

**Skips:** Macro-expanded structs.

**Config:**
```toml
[large_struct]
threshold = 12
```

**Message:**
```
warning: struct `MegaConfig` has 15 fields, consider splitting into smaller types
  --> src/lib.rs:20:1
   = help: group related fields into separate structs to improve readability
```

## `lib.rs` registration pattern

```rust
#![feature(rustc_private)]

extern crate rustc_hir;
extern crate rustc_lint;
extern crate rustc_session;
extern crate rustc_ast;
extern crate rustc_span;
extern crate rustc_middle;

mod config;
mod lints;

use rustc_lint::{LintStore, Session};

dylint_linting::dylint_library!();

#[unsafe(no_mangle)]
pub fn register_lints(_sess: &Session, lint_store: &mut LintStore) {
    lint_store.register_lints(&[
        lints::suggest_builder::SUGGEST_BUILDER,
        lints::needless_builder::NEEDLESS_BUILDER,
        lints::large_struct::LARGE_STRUCT,
    ]);
    lint_store.register_late_pass(|_| Box::new(lints::suggest_builder::SuggestBuilder::new()));
    lint_store.register_late_pass(|_| Box::new(lints::needless_builder::NeedlessBuilder::new()));
    lint_store.register_late_pass(|_| Box::new(lints::large_struct::LargeStruct::new()));
}
```

## Shared attribute checking

Write a shared helper in `lints/mod.rs` for detecting bon builder derives in attribute lists — both `suggest_builder` and `needless_builder` need it. Parse the derive attribute's delimited token stream and check for path segments matching `bon::Builder` or `Builder`.

## Testing

Each lint gets a `ui/` subdirectory with a `.rs` input file and `.stderr` expected output. Register each as a separate test:

```rust
#[test]
fn ui_suggest_builder() {
    dylint_testing::ui_test_example(env!("CARGO_PKG_NAME"), "suggest_builder");
}
```

UI test files should cover: triggering the lint, not triggering below threshold, suppression with `#[allow(lint_name)]`, structs with bon builder already applied, tuple structs, unit structs, and macro-generated structs.

## Configuration loading

Each lint struct loads its config via `dylint_linting::config_or_default(env!("CARGO_PKG_NAME"))` in its `new()` constructor. Config structs derive `serde::Deserialize` with `#[serde(default)]` on all fields.

## Key references

- dylint repo: https://github.com/trailofbits/dylint
- `dylint_linting` API: `declare_late_lint!`, `impl_late_lint!`, `dylint_library!`, `config_or_default`
- `clippy_utils::diagnostics`: `span_lint_and_help`, `span_lint_and_sugg`
- rustc HIR: `Item`, `ItemKind::Struct`, `VariantData::Struct`, `FieldDef`
- `LateLintPass` trait: `check_item` method
- Scaffold with `cargo dylint new` to get the correct nightly rev and `clippy_utils` rev, then restructure into the multi-lint layout above
