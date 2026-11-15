#!/usr/bin/env bash
# Print the path of the working tree's lint library under the
# `@<toolchain>`-tagged name `cargo dylint --lib-path` requires. dylint-link
# writes that tagged copy to target/debug when it links the library.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Rustup names a toolchain `<channel>-<host>`. The Nix dev shell exports
# RUSTUP_TOOLCHAIN in that form; derive it otherwise.
toolchain="${RUSTUP_TOOLCHAIN:-}"
if [[ -z "$toolchain" ]]; then
  channel="$(sed -n 's/^channel = "\(.*\)"$/\1/p' "$REPO_ROOT/rust-toolchain")"
  host="$(rustc -vV | sed -n 's/^host: //p')"
  toolchain="$channel-$host"
fi

case "$(uname -s)" in
  Darwin) suffix=.dylib ;;
  *) suffix=.so ;;
esac

echo "$REPO_ROOT/target/debug/librust_lints@$toolchain$suffix"
