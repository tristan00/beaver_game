#!/usr/bin/env bash
set -euo pipefail
if [ $# -lt 3 ]; then
  echo "usage: tools/sandbox_shot.sh SCRIPT|MAP X,Y[,DIST[,YAW[,PITCH]]] OUT.png [notrees]" >&2
  exit 2
fi
src=$1; at=$2; out=$3; trees=${4:-}
IFS=, read -r x y dist yaw pitch <<<"$at"
dist=${dist:-40}; yaw=${yaw:-0.8}; pitch=${pitch:-1.0}
here=$(cd "$(dirname "$0")/.." && pwd)
if [ -f "$src" ]; then args=(--sandbox-script "$src"); else args=(--sandbox --map "$src"); fi
export BC_NO_FOG=1 BEAVER_WINDOW=${BEAVER_WINDOW:-1600x900}
export BC_CAM_AT="$x,$y,$dist,$yaw,$pitch"
if [ "$trees" = notrees ]; then export BC_NO_TREES=1; fi
timeout 300 "$here/target/release/beaver_colony" "${args[@]}" --screenshot "$out" --frames 30 >"$out.log" 2>&1 \
  || { echo "sandbox_shot: failed, see $out.log" >&2; tail -5 "$out.log" >&2; exit 1; }
echo "$out"
