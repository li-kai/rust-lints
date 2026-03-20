# Dev-shell bootstrap for cargo-dylint.
# This runs when entering the supported mkDevShell environment: it prioritizes
# the pinned toolchain on PATH and ensures the matching cargo-dylint version is
# installed in the user's cargo bin directory.

export PATH="@RUST_TOOLCHAIN_BIN@:@RUSTUP_SHIM_BIN@:$HOME/.cargo/bin:$PATH"
echo "Rust toolchain: $(@RUSTC_BIN@ --version)"

# Install cargo-dylint if missing or version has changed.
# Not packaged as a Nix derivation due to dylint's build.rs
# requiring a ../driver sibling directory (see note above).
mkdir -p "$HOME/.cargo/bin"
_cargo_dylint_marker="$HOME/.cargo/bin/.cargo-dylint-version"
if [ "$(cat "$_cargo_dylint_marker" 2>/dev/null)" != "@DYLINT_VERSION@" ]; then
  echo "Installing cargo-dylint v@DYLINT_VERSION@..."
  "@CARGO_BIN@" install cargo-dylint --version "@DYLINT_VERSION@" --quiet
  echo "@DYLINT_VERSION@" > "$_cargo_dylint_marker"
fi
