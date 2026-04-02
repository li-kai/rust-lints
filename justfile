# Justfile for rust-lints dylint library
# Install: cargo install just
# Usage: just build

set positional-arguments := true
set dotenv-load := true
set quiet := true
set shell := ["bash", "-euo", "pipefail", "-c"]

# Default: show all recipes
default:
    just --list

# Build the lint library
build *args:
    cargo build {{ args }}

# Run UI tests (quiet on pass, shows failures and summary)
[no-exit-message]
test *args:
    cargo test -q {{ args }}

# Update expected UI test output (.stderr files)
bless *args:
    DYLINT_BLESS=1 cargo test {{ args }}

# Check code with clippy (no modifications)
check *args:
    cargo clippy --lib --tests --benches --bins {{ args }} -- -D warnings
    DYLINT_LIBRARY_PATH="$PWD/target/debug" cargo dylint --lib rust_lints -- --lib --tests --benches --bins

# Auto-fix clippy issues and format code
fix *args:
    cargo clippy --lib --tests --benches --bins --fix --allow-dirty {{ args }} -- -D warnings
    DYLINT_LIBRARY_PATH="$PWD/target/debug" cargo dylint --fix --lib rust_lints -- --allow-dirty --lib --tests --benches --bins
    just fmt

# Format code (use --check to verify without changing)
fmt *args:
    cargo fmt --all {{ args }}

# Watch and rebuild on changes
watch *args='build':
    cargo watch -x {{ args }}

# Clean build artifacts
[confirm("This will delete all build artifacts. Continue?")]
clean:
    cargo clean

# Generate documentation (use --open to open in browser)
doc *args='--open':
    cargo doc --no-deps {{ args }}

# Run all checks
check-all:
    just check
    just test
