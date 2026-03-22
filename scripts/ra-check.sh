#!/usr/bin/env bash
# rust-analyzer check override: runs clippy + dylint, merging JSON diagnostics.
# rust-analyzer requires --message-format=json output on stdout.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Run clippy with JSON output, passing through any extra args from rust-analyzer
cargo clippy --lib --tests --benches --bins --message-format=json "$@" -- -D warnings 2>/dev/null

# Run dylint with JSON output (requires the lint library to be built)
if [[ -d "$REPO_ROOT/target/debug" ]]; then
  DYLINT_LIBRARY_PATH="$REPO_ROOT/target/debug" \
    cargo dylint --lib rust_lints -- --message-format=json --lib --tests --benches --bins "$@" 2>/dev/null
fi
