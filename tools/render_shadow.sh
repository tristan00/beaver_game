#!/bin/sh
# usage: tools/render_shadow.sh OUTDIR [VIEW...] -> per view (docs/STYLE.md shared lighting rule):
#   NAME.png         product frame
#   NAME_noglow.png  same without the warm UI glow overlay (colours measurable over the whole frame)
#   NAME_lit.png     same without sun shadows (classifies terrain colours)
#   NAME_mask.png    terrain painted by sun visibility: shadow magenta, penumbra cyan, lit yellow
#   NAME_table.md    tools/shadow_check.py on the no-glow frame, then on the product frame outside the glow
out=$1; shift; mkdir -p "$out"
bin=./target/release/beaver_colony
fail=0
r() { # name seed cam season
  args="--start 1 --seed $2 --cam $3"
  rm -f "$out/$1.png" "$out/$1_lit.png" "$out/$1_mask.png" "$out/$1_noglow.png"
  shot() { # file env...
    f=$1; shift
    env BEAVER_SEASON=$season "$@" $bin $args --screenshot "$out/$f" > "$out/$f.log" 2>&1 || { echo "render $f failed:"; grep -i error "$out/$f.log"; exit 2; }
    rm -f "$out/$f.log"
  }
  season=$4
  shot "$1_lit.png" BC_NO_GLOW=1 BC_NO_SUN_SHADOWS=1
  shot "$1_mask.png" BC_NO_GLOW=1 BC_SHADOW_MASK=1
  shot "$1_noglow.png" BC_NO_GLOW=1
  shot "$1.png"
  t="$out/$1_table.md"
  printf '## %s: seed %s, --start 1 --cam %s, %s\n\n' "$1" "$2" "$3" "$4" > "$t"
  python tools/shadow_check.py "$out/$1_lit.png" "$out/$1_noglow.png" "$out/$1_mask.png" --title "whole frame, $1_noglow.png" >> "$t" || fail=1
  python tools/shadow_check.py "$out/$1_lit.png" "$out/$1.png" "$out/$1_mask.png" --glow --title "product frame outside the glow, $1.png" >> "$t" || fail=1
  cat "$t"
}
views=${*:-"close_s1_summer close_s1_autumn wide_s1_summer wide_s1_autumn wide_s2_summer wide_s2_autumn"}
for v in $views; do
  case $v in
    close_s1_summer) r $v 1 iso-close summer ;;
    close_s1_autumn) r $v 1 iso-close autumn ;;
    wide_s1_summer) r $v 1 iso-wide summer ;;
    wide_s1_autumn) r $v 1 iso-wide autumn ;;
    wide_s2_summer) r $v 2 iso-wide summer ;;
    wide_s2_autumn) r $v 2 iso-wide autumn ;;
  esac
done
exit $fail
