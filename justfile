# Justfile for rust-lints dylint library
# Install: cargo install just
# Usage: just build

set positional-arguments := true
set dotenv-load := true
set shell := ["bash", "-euo", "pipefail", "-c"]

# Default: show all recipes
default:
    @just --list

# Build the lint library
build *args:
    cargo build {{ args }}

# Run UI tests
[no-exit-message]
test *args:
    cargo test {{ args }}

# Check code with clippy (no modifications)
check *args:
    cargo clippy --lib --tests {{ args }} -- -D warnings
    DYLINT_LIBRARY_PATH="$PWD/target/debug" cargo dylint --lib rust_lints

# Auto-fix clippy issues and format code
fix *args:
    cargo clippy --lib --tests --fix --allow-dirty {{ args }} -- -D warnings
    DYLINT_LIBRARY_PATH="$PWD/target/debug" cargo dylint --lib rust_lints
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
