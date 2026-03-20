#!/bin/sh
# git wrapper for the dylint-driver build.
# The build script clones rust-clippy at the revision matching our pinned
# toolchain; this wrapper redirects that clone to a prefetched local checkout
# and synthesizes the minimal git history the build script expects.

set -u

GREP="@GREP@"
REAL_GIT="@REAL_GIT@"
CLIPPY_SRC="@CLIPPY_SRC@"
TOOLCHAIN_CHANNEL="@TOOLCHAIN_CHANNEL@"

case "$1" in
  clone)
    if echo "$2" | $GREP -q "rust-clippy"; then
      dest="${3:-$(basename "$2" .git)}"
      cp -r "$CLIPPY_SRC"/. "$dest"
      chmod -R u+w "$dest"
      # The dylint_driver build script iterates backward through git
      # history, only emitting a Rev when the channel changes between
      # consecutive commits. A single commit would never be emitted.
      # Create a 2-commit history: old commit with a dummy channel,
      # then HEAD with the real channel. This triggers the channel
      # change and the iterator emits the HEAD rev.
      $REAL_GIT -C "$dest" -c init.defaultBranch=master init --quiet
      # First commit: dummy channel so the iterator sees a change.
      sed -i.bak "s/$TOOLCHAIN_CHANNEL/nightly-2000-01-01/" "$dest/rust-toolchain.toml"
      $REAL_GIT -C "$dest" add .
      $REAL_GIT -C "$dest" -c user.email=nix -c user.name=nix commit -m old --quiet
      # Second commit (HEAD): restore real channel.
      mv "$dest/rust-toolchain.toml.bak" "$dest/rust-toolchain.toml"
      $REAL_GIT -C "$dest" add .
      $REAL_GIT -C "$dest" -c user.email=nix -c user.name=nix commit -m head --quiet
      exit 0
    fi
    exec $REAL_GIT "$@"
    ;;
  checkout)
    # No-op only inside the prefetched clippy source.
    if pwd | $GREP -q "rust-clippy"; then
      exit 0
    fi
    exec $REAL_GIT "$@"
    ;;
  *)
    exec $REAL_GIT "$@"
    ;;
esac
