#!/usr/bin/env bash
set -euo pipefail
# Fixed-point `cargo clippy --fix` loop over the workspace, restricted to the
# machine-applicable lints this library tracks.
#
# fixable-lints.cargo.toml is the single source of truth for the set: it's
# auto-generated (scripts/scrape-clippy-applicability.py) and lists every
# MachineApplicable lint as `<name> = "allow"`. We re-enable each as `-W` here so
# clippy applies its machine-applicable fix at commit time — deriving the flags
# from that file means the hook can't drift from the published list.
#
# --fix skips fixes with overlapping spans, so a single pass can leave code
# unfixed; iterate until clippy reports no more "Fixed" lines (bounded at 5).

# Build the -W flags from the tracked fixable-lints list.
mapfile -t flags < <(awk '
  /^[a-z_0-9]+ = "allow"/ { print "-W"; print "clippy::" $1 }
' fixable-lints.cargo.toml)

for (( i=0; i<5; i++ )); do
  output=$(cargo clippy --fix --allow-dirty --allow-staged -- "${flags[@]}" 2>&1 || true)
  grep -Eq 'Fixed' <<<"$output" || break
done

# Surface any residual diagnostics after the loop settles.
if grep -Eq '^error|^warning\[' <<<"$output"; then
  echo "$output"
fi
