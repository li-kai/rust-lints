#!/usr/bin/env bash
set -euo pipefail
# Auto-generated — do not edit manually.
# --fix only applies MachineApplicable suggestions, so enabling
# entire groups is safe — unfixable lints produce no code changes.

cargo clippy --fix --allow-dirty --allow-staged -- \
  -W clippy::all \
  -W clippy::pedantic \
  -W clippy::nursery 2>/dev/null

