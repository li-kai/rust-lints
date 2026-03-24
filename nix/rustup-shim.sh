#!/bin/sh
# rustup shim for nix-managed toolchains.
# Dylint and related tooling expect a small subset of `rustup` commands; this
# wrapper serves those from the pinned toolchain exposed by the flake.

set -u

# Strip `+toolchain` prefix (e.g. `rustup +stable which cargo`).
# In a Nix-managed environment the toolchain selector is meaningless —
# there is only one toolchain on PATH.
case "${1-}" in
  +*) shift ;;
esac

case "$1" in
  which)
    exec which "$2"
    ;;
  show)
    echo "nix-managed $(rustc --version)"
    ;;
  toolchain)
    echo "@TOOLCHAIN_FULL@ (nix-managed)"
    ;;
  run)
    # `rustup run <toolchain> <cmd> [args...]` — skip toolchain arg, run cmd
    if [ $# -lt 3 ]; then
      echo "rustup shim: 'run' requires a toolchain and command" >&2
      exit 1
    fi
    shift 2
    exec "$@"
    ;;
  *)
    echo "rustup shim: unsupported command '$*'" >&2
    exit 1
    ;;
esac
