#!/usr/bin/env bash
# Screenshot the model gallery: every lodge and food-cache stage, dam cells, and every beaver pose,
# lined up on the flattest dry ground of the sandbox map.
#
#   tools/gallery_shot.sh WHAT DIST,YAW,PITCH[,DZ,DX] OUT.png [POSE]
#
# WHAT is build, beaver or all. DZ/DX shift the camera target from the gallery centre (buildings
# are at DZ -1 to -28, dam cells at -31, beavers at +3.5). POSE (0-9) shows that one pose alone. With BC_BURST=N the
# clock runs and N frames are saved (OUT_000.png ...). Build with `cargo build -p game --release`.
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
