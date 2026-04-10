#!/bin/sh
set -eu

command=$(jq -r '.tool_input.command')

# Rewrite the cargo invocations we standardize in this repo to their `just`
# equivalents, while preserving any trailing arguments.
updated_command=$(printf '%s\n' "$command" | sed -E \
  -e 's/^cargo fix( |$)/just fix\1/' \
  -e 's/^cargo clippy --fix( |$)/just fix\1/' \
  -e 's/^cargo clippy --lib[^ ]* --fix( |$)/just fix\1/' \
  -e 's/^cargo clippy( |$)/just check\1/' \
  -e 's/^cargo (test|check|build|fmt)( |$)/just \1\2/')

if [ "$updated_command" = "$command" ]; then
  exit 0
fi

jq -n --arg command "$updated_command" '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    updatedInput: {
      command: $command
    }
  }
}'
