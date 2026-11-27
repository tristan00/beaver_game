#!/usr/bin/env bash
set -euo pipefail
what=$1; cam=$2; out=$3; pose=${4:-}
here=$(cd "$(dirname "$0")/.." && pwd)
export BC_NO_FOG=1 BC_NO_TREES=1 BEAVER_WINDOW=${BEAVER_WINDOW:-1600x900}
export BC_GALLERY="$cam" BC_GALLERY_WHAT="$what"
if [ -n "$pose" ]; then export BC_GALLERY_POSE="$pose"; fi
timeout 300 "$here/target/release/beaver_colony" --sandbox --screenshot "$out" --frames 90 >"$out.log" 2>&1 \
  || { echo "gallery_shot: failed, see $out.log" >&2; tail -5 "$out.log" >&2; exit 1; }
rm -f "$out.log"
echo "$out"
