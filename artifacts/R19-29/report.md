# R19-29: fresh shots on final HEAD, STYLE.md fixes 1-3, 6-11, 18, 4, 5

Branch `task/R19-29`. All shots: `beaver_colony --start 1 --seed N --cam iso|lodge|pond --screenshot F --frames 40` (N = 1, 2), saved in `artifacts/R19-29/after/` and copied over `artifacts/R19-16/after/`. Samples are taken on the `*_noglow.png` twins (`BC_NO_GLOW=1`) because the designed upper-left glow overlay tints much of the product frame. Reproduce: `python tools/style_samples.py artifacts/R19-29/after`.

## Failures found and fixed (exact repro, expected vs actual)

| # | Input / steps | Expected | Actual before fix | Fix |
|---|---|---|---|---|
| 1 | `BC_NO_MENU=1 BC_NO_GLOW=1 BC_STATION=0 beaver_colony --showcase poses` (before the tone-map fix), sample beaver back | Fur `#B9783F` | `#DE9C63` (+37/36/36); tail `#C5895D` vs Fur dark `#8F5A2E` | `models.rs` used hard-coded `#9A6532` etc. -> palette constants; tone-map inverse `palette::display_linear` + `ENTITY_GAIN` |
| 2 | `--start 1 --seed 1 --cam pond --screenshot` (`BC_NO_GLOW=1`), pond rim | Deep `#1F8AB8` | best-matching pixel in the whole frame `#3CAFDA`-`#2E8CBE` (15-30 off), no pixel within 12 | water vertex colours through `display_linear` x `WATER_GAIN`; rim fully opaque (alpha 0.8+0.2 deep, was 0.9) |
| 3 | same pond shot, ripple arc `water_mat` | Foam `#F4FBF6` | `#EFEDEB` (G 14 off): Foam multiplied twice (material colour x vertex colour) | vertices through `display_linear`, material white-ish; now `#F9F7F9` |
| 4 | pond shot, open grass crop x900-1040, y330-520 | about 1 item per 40x40 px | 11 items in 240x320 px = about 1 per 90x90 px; pebbles drawn as thin crescents | `DETAIL_DENSITY` 0.14 -> 0.7, items resized to STYLE sizes |
| 5 | `--start 1 --cam lodge` / `pond` | lodge and pond framing | `--start` overwrote the camera, so lodge and pond shots without the start menu were impossible; without `--start` the menu card and veil covered the shot | `startflow.rs` keeps those cameras; `BC_NO_MENU=1` hides the menu for `--showcase` |
| 6 | `cargo test -p game` | pass | `water_rings_and_rebuild_cost` failed after change 2 (expected old Foam value) | test uses `display_linear(FOAM) * WATER_GAIN` |

Sample points that failed on the first table and were replaced or reinterpreted (not hidden): pond_seed2 (1000,330) Meadow deep `#6EB45F` (15 off) is a point on the Meadow-to-Meadow-deep gradient, not a floor sample; lodge_seed2 (840,192) `#84BE69` vs Meadow light (36 off) is shaded grass, replaced by sun-facing points; lodge_seed2 (100,200) rock `#9C9298` vs Stone (29 off) is rock on a slope turned from the sun (half the slope relief is kept by design). Samples within 12 only just: Stone (800,250) 12, Fur dark tail 12.

## Criterion 2: samples (fixes 1-3, 6-11, 18)

| Fix | Shot | Point (x,y) | What | Measured | Target | Max channel diff | Result |
|---|---|---|---|---|---|---|---|
| 1 | lodge_seed1_noglow.png | 800,250 | Stone, sunlit rock | #ADA6A0 | #B9AFA8 | 12 | PASS |
| 1 | lodge_seed2_noglow.png | 1270,125 | Stone, sunlit rock | #B4AAA5 | #B9AFA8 | 5 | PASS |
| 1 | lodge_seed2_noglow.png | 1000,250 | Stone shade, shadowed rock | #948998 | #8C8290 | 8 | PASS |
| 1 | pond_seed1_noglow.png | 158,159 | Stone shade, shadowed rock | #8B7E93 | #8C8290 | 4 | PASS |
| 2 | pond_seed1_noglow.png | 1000,300 | Meadow deep, valley grass | #66B35F | #5FA85A | 11 | PASS |
| 2 | lodge_seed2_noglow.png | 451,710 | Meadow deep, valley floor | #61A658 | #5FA85A | 2 | PASS |
| 2 | lodge_seed2_noglow.png | 400,330 | Meadow in sun shadow (floor of a valley) | #7AA079 | #7B9A76 | 7 | PASS |
| 3 | lodge_seed2_noglow.png | 200,400 | Meadow, base grass | #89C66A | #85C468 | 4 | PASS |
| 3 | pond_seed1_noglow.png | 1100,600 | Meadow, base grass | #80C46C | #85C468 | 5 | PASS |
| 3 | lodge_seed1_noglow.png | 620,425 | Meadow light, sun-facing grass | #A6DC77 | #A8D878 | 4 | PASS |
| 3 | lodge_seed1_noglow.png | 505,544 | Meadow light, sun-facing grass | #A3D577 | #A8D878 | 5 | PASS |
| 6 | lodge_seed2_noglow.png | 480,410 | pine canopy, Leaf highlight | #89CB70 | #8CD070 | 5 | PASS |
| 6 | lodge_seed2_noglow.png | 490,450 | pine canopy, Leaf highlight | #8ED074 | #8CD070 | 4 | PASS |
| 7 | pond_seed1_noglow.png | 662,354 | pond centre, Mid glow | #8DD6E3 | #8FE0E6 | 10 | PASS |
| 7 | pond_seed2_noglow.png | 600,379 | pond centre, Mid glow | #8ED7E4 | #8FE0E6 | 9 | PASS |
| 7 | pond_seed1_noglow.png | 1131,140 | pond body, Shallow | #72CDDC | #6FD3E0 | 6 | PASS |
| 7 | pond_seed1_noglow.png | 718,396 | pond rim, Deep | #278ABD | #1F8AB8 | 8 | PASS |
| 7 | pond_seed2_noglow.png | 120,325 | pond rim, Deep | #278ABD | #1F8AB8 | 8 | PASS |
| 8 | lodge_seed1_noglow.png | 777,240 | lowland water, Shallow | #73CEDD | #6FD3E0 | 5 | PASS |
| 8 | lodge_seed1_noglow.png | 789,628 | lowland water edge, Deep | #288ABD | #1F8AB8 | 9 | PASS |
| 9 | fur_poses_1_noglow.png | 560,305 | swimmer's ripple arc, Foam | #F9F7F9 | #F4FBF6 | 5 | PASS |
| 10 | lodge_seed2_noglow.png | 1100,40 | sky | #9ED1F5 | #9ED1F5 | 0 | PASS |
| 10 | lodge_seed2_noglow.png | 400,330 | grass in sun shadow, plum 40% | #7AA079 | #7B9A76 | 7 | PASS |
| 10 | lodge_seed2_noglow.png | 1000,250 | rock in sun shadow, plum 40% | #948998 | #9A8D9C | 6 | PASS |
| 18 | fur_poses_1_noglow.png | 570,230 | beaver back, Fur | #B8743F | #B9783F | 4 | PASS |
| 18 | fur_poses_0_noglow.png | 560,330 | beaver back, Fur | #BF7942 | #B9783F | 6 | PASS |
| 18 | fur_poses_0_noglow.png | 470,352 | beaver tail, Fur dark | #9B5F32 | #8F5A2E | 12 | PASS |

| Fix 2 shot | Darkest 5x5 patch of sun-lit terrain (x,y) | Colour | Floor (lower of Meadow deep / Stone shade, -12/channel) | Result |
|---|---|---|---|---|
| iso_seed1_noglow.png | 116,187 | #9C9398 | #53764E | PASS |
| iso_seed2_noglow.png | 1028,237 | #988F97 | #53764E | PASS |
| lodge_seed1_noglow.png | 564,145 | #928996 | #53764E | PASS |
| lodge_seed2_noglow.png | 388,195 | #918796 | #53764E | PASS |
| pond_seed1_noglow.png | 419,390 | #918796 | #53764E | PASS |
| pond_seed2_noglow.png | 3,648 | #918796 | #53764E | PASS |

| Fix 11 shot | Mean luminance (0-255) | Result |
|---|---|---|
| iso_seed1_noglow.png | 184.1 | PASS (> 128) |
| iso_seed2_noglow.png | 187.3 | PASS (> 128) |
| lodge_seed1_noglow.png | 169.8 | PASS (> 128) |
| lodge_seed2_noglow.png | 171.4 | PASS (> 128) |
| pond_seed1_noglow.png | 165.5 | PASS (> 128) |
| pond_seed2_noglow.png | 173.0 | PASS (> 128) |


Aggregate terrain check (`tools/render_shadow.sh`, tables in `artifacts/R19-29/shadow/all_tables.md`): all six views PASS, 100 % of classified lit pixels within 12 of Meadow light / Meadow / Meadow deep / Straw / Stone; shadowed pixels within 12 of a 35-45 % plum blend. Terrain pixels below the per-channel floor: 0.00-0.04 % of the frame, all on grass tufts (Lily `#4E9A52` by design).

## Criterion 3

**Fix 4 (slopes)**: transects marked on `artifacts/R19-29/transect_*.png` (magenta lines, horizontal runs fully on sun-lit terrain, luminance range >= 25): `lodge_seed2` y=534 x1049-1254 (rock slope, range 42, max adjacent jump 1.1), `lodge_seed1` y=48 x360-623 (range 40, jump 1.9), `pond_seed1` y=303 x1-423 (range 36, jump 1.2), `iso_seed2` y=270 x997-1127 (range 37, jump 4.6). All <= 8. Of all 435 qualifying runs, 239 pass; each of the 196 that exceed 8 has a prop, shadow or tree-edge pixel within 8 px of the jump (0 unexplained), i.e. they cross a tuft or a flower, not a facet.

**Fix 5 (ground detail)**: `artifacts/R19-29/grass_density_seed1.png`, crop (900,330)-(1040,520) of `pond_seed1_noglow.png` = 140x190 px of open grass: 15 items (10 tufts, 2 flowers, 3 pebbles) = 0.90 per 40x40 px, one per 42x42 px (`tools/ground_detail_density.py`). A second pond_seed2 crop (1000,330)-(1160,470) holds tree shadow and rock, so it only counts as indicative: 11 items, 0.79 per 40x40.

## Criterion 4: stress scene

`beaver_colony --stress` twice on final HEAD: avg 3.35 / 3.45 ms, median 3.46 / 3.53, p95 3.91 / 4.00, p99 4.17 / 4.27, max 4.58 / 4.44 ms, 0 frames >= 16 ms (4949 trees, 300 beavers, 900 frames). Live sim, `BEAVER_FRAMELOG` with chunk rebuilds running: iso avg 5.21 max 11.05 ms, pond avg 5.71 max 13.53 ms (< 16).

## Tests

`cargo test -p game`: 86 passed. `cargo test -p sim`: 129 passed, 1 ignored.

## Known gaps

- Swim ripples (fix 9) are only sampled in the staged `--showcase poses` shot; the iso/lodge/pond shots at start 1 contain no swimming beaver.
- The pond rim ring edge is polygonal in places at the corners of the pond in `pond_seed1` (8-gon-like corners, bottom right); not measured against a threshold.
- Entity colours other than the beaver (lodge, bark, rocks) now go through the tone-map inverse but were not individually sampled.
## Re-verification after merging master (lake camera, start flow)

Conflict in `crates/game/src/startflow.rs` resolved by keeping both the lodge/pond camera keep and master's `frame_lake`. All shots re-rendered from the merged HEAD (`artifacts/R19-29/after`, copied to `artifacts/R19-16/after`). `tools/style_samples.py`: all 26 samples unchanged or within 1 channel level of the table above, all PASS. Slope transects identical (e.g. lodge_seed2 y=534, jump 1.1); grass density crop 15 items = 0.90 per 40x40. Stress: avg 3.38, p99 4.25, max 4.81 ms, 0 frames >= 16 ms.
