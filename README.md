# rust-lints

Custom Rust lints via the [dylint](https://github.com/trailofbits/dylint) ecosystem.

## Lints

| Lint | Level | Description |
|------|-------|-------------|
| [`suggest_builder`](#suggest_builder) | warn | Structs with ≥ 4 named fields that don't derive `bon::Builder` |
| [`needless_builder`](#needless_builder) | warn | Structs with ≤ 2 named fields that unnecessarily derive `bon::Builder` |

### `suggest_builder`

Suggests adding `#[derive(bon::Builder)]` to structs with many named fields.

```
warning: struct `Config` has 5 fields but does not derive `bon::Builder`
  --> src/lib.rs:10:1
   = help: add `#[derive(bon::Builder)]` to enable the builder pattern
```

### `needless_builder`

Warns when `bon::Builder` is derived on a struct with very few fields.

```
warning: struct `Point` has only 2 fields; `bon::Builder` may be unnecessary
  --> src/lib.rs:5:1
   = help: consider using a plain constructor or struct literal instead
```

## Usage

Add to your workspace `Cargo.toml`:

```toml
[workspace.metadata.dylint]
libraries = [
    { git = "https://github.com/li-kai/rust-lints" },
]
```

Configure thresholds in `dylint.toml`:

```toml
[suggest_builder]
threshold = 4

[needless_builder]
threshold = 2

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
