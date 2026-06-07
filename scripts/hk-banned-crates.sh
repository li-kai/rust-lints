#!/usr/bin/env bash
set -euo pipefail
# Fail the commit if any staged Cargo.toml declares a banned dependency.
# Args: the Cargo.toml files to scan (passed by hk as {{files}}).
#   crate|reason (one line each; | does not appear in reasons)

rc=0
while IFS='|' read -r crate reason; do
  [[ -n "$crate" ]] || continue
  for toml in "$@"; do
    [[ -f "$toml" ]] || continue
    if grep -Eq "^[[:space:]]*${crate}[[:space:]]*(=|\{)" "$toml"; then
      echo "banned dependency '${crate}' in ${toml} — ${reason}"
      rc=1
    fi
  done
done <<'BANNED'
lazy_static|use std::sync::LazyLock (Rust 1.80+)
once_cell|use std::sync::OnceLock / LazyLock (Rust 1.70+/1.80+)
failure|use thiserror for libraries, anyhow for applications
dashmap|use RwLock<HashMap> — DashMap deadlocks when a Ref is held across map calls
openssl|use rustls
md5|MD5 is cryptographically broken; use SHA-256 or SHA-3
sha1|SHA-1 collision resistance is broken; use SHA-256 or SHA-3
BANNED

exit "$rc"
