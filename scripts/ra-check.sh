#!/usr/bin/env bash
# Check override: clippy + dylint diagnostics.
# Pass --message-format=json for rust-analyzer, or omit for short (default).
set -uo pipefail

# Skips --examples to avoid compiling ui/ test fixtures in this repo.
# Consumers should use --all-targets instead.
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGETS="--lib --tests --benches --bins"

# Default to short format unless caller specifies otherwise.
FMT="--message-format=short"
IS_JSON=false
for arg in "$@"; do
  case "$arg" in
    --message-format=json*) FMT=""; IS_JSON=true; break;;
    --message-format=*)     FMT=""; break;;
  esac
done

# Run clippy and dylint in parallel (they use separate target dirs),
# buffer output, then emit sequentially.
CLIPPY_OUT=$(mktemp)
DYLINT_OUT=$(mktemp)
trap 'rm -f "$CLIPPY_OUT" "$DYLINT_OUT"' EXIT

# json: diagnostics on stdout, discard stderr (cargo noise).
# short: diagnostics on stderr, merge into stdout.
cargo clippy $TARGETS $FMT "$@" -- -D warnings >"$CLIPPY_OUT" 2>&1 || true &

if [[ -d "$REPO_ROOT/target/debug" ]]; then
  DYLINT_LIBRARY_PATH="$REPO_ROOT/target/debug" \
    cargo dylint --quiet --lib rust_lints -- $TARGETS $FMT "$@" >"$DYLINT_OUT" 2>&1 || true &
fi

wait

if $IS_JSON; then
  # Only emit valid JSON lines; discard cargo status noise.
  cat "$CLIPPY_OUT" "$DYLINT_OUT" | grep '^{'
else
  cat "$CLIPPY_OUT" "$DYLINT_OUT"
fi
