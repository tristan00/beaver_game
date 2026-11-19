#!/usr/bin/env bash
# Screenshot the running game at one spot of a map, the way a player sees it.
#
#   tools/game_shot.sh MAP X,Y[,DIST[,YAW[,PITCH]]] OUT.png [DAYS] [notrees]
#
# Builds nothing: run `cargo build -p game --release` first. Starts a new game on MAP, settles the
# family at the planned camp (BC_SETTLE), runs DAYS of sim time (default 0), lifts the fog of war,
# aims the camera at map cell (X, Y) (DIST default 30 cells, top-down-ish pitch 1.1) and saves a
# 1600x900 picture. `notrees` hides standing trees so the ground and streams under the canopy show.
set -euo pipefail
map=$1; at=$2; out=$3; days=${4:-0}; trees=${5:-}
IFS=, read -r x y dist yaw pitch <<<"$at"
dist=${dist:-30}; yaw=${yaw:-0.8}; pitch=${pitch:-1.1}
here=$(cd "$(dirname "$0")/.." && pwd)
export BC_NO_FOG=1 BC_SETTLE=1 BEAVER_WINDOW=${BEAVER_WINDOW:-1600x900} BC_WAIT_DAYS=$days
export BC_CAM_AT="$x,$y,$dist,$yaw,$pitch"
if [ "$trees" = notrees ]; then export BC_NO_TREES=1; fi
timeout 300 "$here/target/release/beaver_colony" --map "$map" --screenshot "$out" --frames 30 >"$out.log" 2>&1 \
  || { echo "game_shot: failed, see $out.log" >&2; tail -5 "$out.log" >&2; exit 1; }
echo "$out"
