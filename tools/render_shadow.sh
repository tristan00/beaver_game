#!/bin/sh
# usage: render_shadow.sh OUTDIR  -> renders lit/shadowed pairs and tables for the acceptance views
out=$1; mkdir -p $out
bin=./target/release/beaver_colony
r() { # name seed cam season
  BEAVER_SEASON=$4 BC_NO_SUN_SHADOWS=1 $bin --seed $2 --cam $3 --screenshot $out/$1_lit.png >/dev/null 2>&1
  BEAVER_SEASON=$4 $bin --seed $2 --cam $3 --screenshot $out/$1.png >/dev/null 2>&1
  python tools/shadow_check.py $out/$1_lit.png $out/$1.png $out/$1_table.md >/dev/null
  echo "== $1"; cat $out/$1_table.md
}
r close_s1_summer 1 iso-close summer
r close_s1_autumn 1 iso-close autumn
r wide_s1_summer 1 iso summer
r wide_s1_autumn 1 iso autumn
r wide_s2_summer 2 iso summer
r wide_s2_autumn 2 iso autumn
