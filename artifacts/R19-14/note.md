# R19-14 Start 2 / Start 3 screenshots (`--cam lake`, seed 1)

Commands (release build, HEAD d7e3365): `beaver_colony.exe --start N --cam lake --seed 1 --screenshot artifacts/R19-14/startN.png` (default 10 frames). Full program output: `start2.log`, `start3.log`.
Both images are 1280x720; distinct colours counted over every pixel: start2 = 53402, start3 = 74283.

## start2.png (--start 2)
Program output: `15 dam cells, 241 lake cells at level 6.75`
- dam box  x 160..591 y 65..310 (431x245 px, 11.4% of window)
- lake box x 200..1120 y 29..691 (920x662 px, 66.1% of window)
Shows: a large filled blue lake, one red-roofed lodge on the right bank, a small domed hut and brown log piles top-left, rocks, birch trees, 2-3 beavers visible (one swimming, selected ring). HUD only: season/day, speed buttons, bottom menu bar. No ledger cards or predator/flood/drought/guard/trade/scouting/rival UI.

## start3.png (--start 3)
Program output: `23 dam cells, 1304 lake cells at level 7.04`
- dam box  x 196..523 y 106..299 (327x193 px, 6.8% of window)
- lake box x 217..1084 y 29..691 (867x662 px, 62.3% of window)
Shows: a visibly larger lake (1304 vs 241 cells), 3-4 red-roofed buildings and a stacked-lumber building at the top edge, brown mounds, a few beavers, forests and cliffs. Same HUD only.

## Defects found (FAIL of criterion 1)
1. start2.png: the dam is not visible. Cropping the reported dam box (x160..591,y65..310) shows only tree canopy over the shore plus a brown log pile; no dam wall across a river. Expected: dam visible. Actual: occluded by trees / not identifiable.
2. start3.png: cropping the dam box (x196..523,y106..299) shows only cliff, trees and shoreline; no dam structure. Expected: visible dam, canals, advanced buildings. Actual: no dam, no canals visible; buildings are few.
3. start2.png shows only ~3 beavers, not "many"; elementary buildings are 1 lodge + hut.
Repro: build release, run the commands above, open the images and crop the boxes printed in the logs.
