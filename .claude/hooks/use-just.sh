#!/usr/bin/env bash
# Redirect cargo commands to their just recipe equivalents.
set -euo pipefail

CMD=$(jq -r '.tool_input.command')

case "$CMD" in
  "cargo test"*) R="just test${CMD#cargo test}" ;;
  "cargo check"*) R="just check${CMD#cargo check}" ;;
  "cargo clippy"*) R="just check${CMD#cargo clippy}" ;;
  "cargo build"*) R="just build${CMD#cargo build}" ;;
  "cargo fmt"*) R="just fmt${CMD#cargo fmt}" ;;
  *) exit 0 ;;
esac

jq -nc --arg cmd "$R" '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    updatedInput: {command: $cmd}
  }
}'
