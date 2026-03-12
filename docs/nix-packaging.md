# Packaging rust-lints as a Nix Flake Output

## Problem

Consumers add our lints via:

```toml
[workspace.metadata.dylint]
libraries = [{ git = "https://github.com/li-kai/rust-lints" }]
```

This causes `cargo-dylint` to clone and build the library from source on every
consumer machine. This has two costs:

1. **Nix consumers can't build it.** The build requires `dylint-link` as a
   custom linker, which calls `rustup` internally. Nix environments don't have
   rustup. Our `flake.nix` solves this for _developers_ of this repo (via shims
   and wrappers), but consumers of the library don't get those shims.

2. **Every consumer rebuilds from source.** Building requires the exact pinned
   nightly with `rustc-dev` and `llvm-tools-preview`, plus `dylint-link`. This
   is slow and fragile.

## Background: how dylint loads and runs lints

Understanding dylint's architecture is key to the solution.

### The driver model

Dylint does **not** load lint dylibs into the consumer's rustc. Instead, it uses
its own **driver** — a small binary linked against `rustc_driver` from the
_library's_ toolchain. The flow:

```
cargo dylint --all
  ↓
cargo-dylint reads DYLINT_LIBRARY_PATH (or workspace.metadata.dylint)
  ↓
Groups libraries by toolchain (parsed from the @toolchain filename tag)
  ↓
For each toolchain group:
  - Finds or builds a dylint-driver linked against that toolchain's rustc_driver
  - Sets RUSTC_WORKSPACE_WRAPPER to that driver
  - Runs cargo check, which invokes the driver instead of rustc
  ↓
The driver loads the dylib and calls register_lints()
  ↓
Lint passes run during compilation
```

**Key insight:** The consumer's own rustc version is irrelevant. Dylint builds a
driver matched to the _library's_ toolchain. A consumer on stable Rust can load
a dylib compiled with nightly — dylint handles the mismatch by using a
toolchain-specific driver.

### The rustup problem (for nix)

Normally, when dylint encounters a library tagged `@nightly-2026-01-22-<triple>`
and has no matching driver, it uses **rustup** to:

1. Install that nightly toolchain (if missing)
2. Build a driver binary linked against that toolchain's `rustc_driver`
3. Cache the driver for future use

In a nix environment without rustup, step 1 fails. This is the core problem.

### The solution: ship both the dylib and the driver

Dylint supports two environment variables:

- **`DYLINT_LIBRARY_PATH`** — directories containing pre-built
  `lib<name>@<toolchain>.<ext>` files
- **`DYLINT_DRIVER_PATH`** — directory containing pre-built driver binaries,
  structured as `<toolchain>/dylint-driver`

If both are set, dylint needs **no rustup interaction at all**. It finds the
pre-built library, matches it to the pre-built driver, and runs.

## Solution: ship dylib + driver as a Nix flake package

We add a `packages.default` output to our `flake.nix` that produces two
artifacts:

```
$out/
  lib/
    librust_lints@nightly-2026-01-22-x86_64-unknown-linux-gnu.so
  drivers/
    nightly-2026-01-22-x86_64-unknown-linux-gnu/
      dylint-driver
```

### Consumer usage

A consumer's `flake.nix`:

```nix
{
  inputs = {
    rust-lints.url = "github:li-kai/rust-lints";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    # Consumer uses whatever Rust toolchain they want — no fenix required,
    # no nightly required. They just need cargo-dylint.
  };

  outputs = { self, nixpkgs, flake-utils, rust-lints, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        lints = rust-lints.packages.${system}.default;
        # cargo-dylint is a pure Rust binary — build it as a proper Nix package,
        # not via imperative `cargo install` in shellHook.
        cargo-dylint = pkgs.rustPlatform.buildRustPackage {
          pname = "cargo-dylint";
          version = "5.0.0";
          # ... or use a crane derivation, or pull from a nixpkgs overlay
        };
      in {
        devShells.default = pkgs.mkShell {
          DYLINT_LIBRARY_PATH = "${lints}/lib";
          DYLINT_DRIVER_PATH = "${lints}/drivers";

          buildInputs = [ cargo-dylint ];
        };
      }
    );
}
```

The consumer then runs `cargo dylint --all` as normal. No git clone, no rebuild,
no rustup, no nightly toolchain. Their project can use stable Rust.

Configuration via `dylint.toml` works exactly as before — it's read at lint
time, not build time.

### Naming convention

Dylint discovers libraries and drivers by filename/directory convention:

| Artifact | Convention | Example |
|---|---|---|
| Library | `<DLL_PREFIX><name>@<toolchain><DLL_SUFFIX>` | `librust_lints@nightly-2026-01-22-x86_64-unknown-linux-gnu.so` |
| Driver | `<toolchain>/dylint-driver` | `nightly-2026-01-22-x86_64-unknown-linux-gnu/dylint-driver` |

The `@toolchain` tag in the library filename is how dylint knows which driver to
pair it with. `dylint-link` (our custom linker) produces this tag automatically
during the build.

## What we need to build

The `packages.default` derivation must produce two things:

### 1. The lint library (cdylib)

- Compile `rust-lints` as a cdylib using the pinned nightly from fenix.
- Use `dylint-link` as the linker (required by `.cargo/config.toml`).
- `dylint-link` produces the `@toolchain`-tagged filename automatically.
- Output: `$out/lib/librust_lints@<toolchain>.<ext>`

### 2. The dylint driver

- Build the `dylint-driver` binary from crates.io, linked against the same
  nightly toolchain's `rustc_driver`.
- The driver binary dynamically links to `librustc_driver` — the sysroot
  libraries must be reachable at runtime (via `-rpath` baked in at link time,
  or via `LD_LIBRARY_PATH`).
- `dylint-driver` has internal version coupling with `cargo-dylint` — version
  mismatches produce silent failures or cryptic errors. Pin both to the same
  version.
- Output: `$out/drivers/<toolchain>/dylint-driver`

### Build-time dependencies (not needed by consumers)

| Dependency | Why | Source |
|---|---|---|
| Rust nightly (`nightly-2026-01-22`) | Compile the cdylib and driver against `rustc_private` APIs | fenix (already in our flake) |
| `rustc-dev`, `llvm-tools-preview` | Provide `rustc_driver` and compiler internals for linking | fenix toolchain components |
| `dylint-link` | Custom linker that produces `@toolchain`-tagged output | Built from crates.io |
| `rustup` shim | `dylint-link` calls `rustup which rustc` internally | Already in our flake |

### Implementation

See `flake.nix` for the full implementation using `crane` with the fenix
toolchain. The key derivations are `dylintLink`, `dylintDriver`, `rustLintsLib`,
and the final `rustLints` symlinkJoin.

### Runtime linking: the `-rpath` detail

The dylint driver dynamically links against `librustc_driver` from the nightly
toolchain's sysroot. Normally this works because rustup sets up `LD_LIBRARY_PATH`
(or `DYLD_LIBRARY_PATH` on macOS) to point at the sysroot.

In Nix, the sysroot lives in `/nix/store/...`. We must bake this path into the
driver binary at build time using `-rpath`, so the driver can find
`librustc_driver` without any environment variable setup. This is standard
practice for Nix — it's how most dynamically-linked binaries work in nixpkgs.

If `-rpath` proves difficult, an alternative is to wrap the driver binary in a
shell script that sets `LD_LIBRARY_PATH` before exec. **Caveat:** the wrapper
changes the binary path, which may confuse dylint's driver discovery (it expects
`<toolchain>/dylint-driver`). If wrapping, ensure the wrapper script itself is
named `dylint-driver` and placed at the expected path:

```nix
dylintDriverWrapper = pkgs.writeShellScriptBin "dylint-driver" ''
  export LD_LIBRARY_PATH="${rustToolchain}/lib:$LD_LIBRARY_PATH"
  exec ${dylintDriver}/bin/dylint-driver "$@"
'';
```

## Multi-platform support

The dylib and driver are both platform-specific. Our flake already uses
`eachDefaultSystem`, so each system produces its own `packages.default` with the
correct target triple. Consumers referencing
`rust-lints.packages.${system}.default` automatically get their platform's
artifacts.

## What this does NOT solve

- **Non-nix consumers.** Teams using rustup can continue with `{ git = "..." }`
  — dylint builds from source and handles the driver automatically. For pre-built
  binaries outside nix, GitHub release artifacts are a separate concern.
- **`cargo-dylint` installation.** Consumers still need `cargo-dylint` to invoke
  the lints. It's a pure Rust binary with no rustup dependency.

## Implementation steps

> **Spike the driver build first.** The driver build (linking against fenix's
> `rustc_driver` with correct `-rpath`) is the highest-risk step. Before writing
> any other nix code, manually verify you can build `dylint-driver` against the
> fenix sysroot and have it find `librustc_driver` at runtime. If this doesn't
> work cleanly, the whole approach needs rethinking.

1. **Spike:** build `dylint-driver` against the fenix toolchain in a standalone
   derivation. Verify it starts and can find `librustc_driver` at runtime.
2. Add `crane` to flake inputs.
3. Build `dylint-link` as a nix derivation from crates.io (build-time only).
4. Build the `rust-lints` cdylib using crane + dylint-link + rustup shim.
5. Combine into a single `packages.default` with the correct output structure.
6. Verify: the output has `lib/librust_lints@<toolchain>.<ext>` and
   `drivers/<toolchain>/dylint-driver`.
7. **Smoke test:** add a Nix derivation that creates a minimal Rust project on
   **stable** Rust, sets `DYLINT_LIBRARY_PATH` and `DYLINT_DRIVER_PATH`, runs
   `cargo dylint --all`, and asserts a lint fires. This is the acceptance
   criterion.
8. Update README with nix consumer instructions.
9. Set up binary cache (see [docs/nix-cachix.md](./nix-cachix.md)).
