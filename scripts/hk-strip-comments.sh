#!/usr/bin/env bash
set -euo pipefail
# Strip decorative comment dividers from the given source files in place.
# Args: the files to rewrite (passed by hk as {{files}}).

for f in "$@"; do
  [[ -f "$f" ]] || continue
  awk -f scripts/strip-decorative-comments.awk "$f" > "$f.tmp" \
    && mv "$f.tmp" "$f" || { rm -f "$f.tmp"; exit 1; }
done
