# Terrain targets from real beaver landscapes

Purpose: numeric ranges for tuning `crates/sim/src/worldgen.rs`, checkable with
`tools/terrain_metrics.py` (no screenshots). Sites: Voyageurs NP (Minnesota), Algonquin PP (Ontario),
Biebrza NP (Poland). Researched 2026-11-17 by web search and page fetches.

**Read this first: evidence is thin.** Paywalled papers (Johnston & Naiman 1990 *Landscape Ecology*,
Thompson et al. 2022 *Wildl. Soc. Bull.*, the Ecoscience 2017 habitat review) could not be opened
(HTTP 403 / login redirect). Only one primary paper was read in full text (Naiman et al. 1988).
No source I could open gave valley depth, hillslope distribution, clearing size or Biebrza
sinuosity/channel width. Those rows are **placeholders (tier P)**, not measurements.
Tiers: **S** = number read from a source, **D** = arithmetic on sourced numbers (shown), **P** = placeholder.
Confidence tags in the "Confidence" column: *read* = I read the text; *snippet* = taken from a search-result
summary, not verified at the source; *analogue* = not one of the three sites.

## 1. Sourced facts (all values as found)

| # | Fact | Value, unit | Source | Confidence |
|---|---|---|---|---|
| F1 | Voyageurs, Kabetogama Peninsula area impounded by beaver, 1940 to 1986 | <1 % to 13 % of 294 km² | Naiman, Johnston & Kelley 1988, *BioScience* 38(11):753-762, p. 758 (JSTOR copy, text extracted) | read |
| F2 | Voyageurs dams, 1940 to 1986 | 71 to 835 dams over >300 km stream channel | same | read |
| F3 | Voyageurs dam frequency | 2.0-3.9 dams/km, mean 2.5 dams/km | same, p. 755 | read |
| F4 | Quebec North Shore (analogue) dam frequency | 8.6-16.0 dams/km, mean 10.6 | same (cites Naiman et al. 1986) | read; analogue |
| F5 | Colony density, favourable habitat | 0.4-0.8 colonies/km², up to 3 | same (cites Aleksiuk 1968 etc.) | read |
| F6 | One large Voyageurs dam | 80 m long, 2 m high, ~1 km² influenced | same, Fig. 1 caption | read; outlier |
| F7 | Riparian uplands altered by browsing, Voyageurs | 12-15 % of uplands | same | read |
| F8 | Voyageurs pond sample | 741 ponds in 250 km² (85 % of the peninsula) | search summary of Johnston & Naiman 1990, *Landscape Ecol.* 4:5-19, doi:10.1007/BF02573947 | snippet |
| F9 | Algonquin colony density | 0.55 colonies/km² (2019 survey, 15 000 km²); 0.29 (1940), 0.40-0.80 (1955-71) | Thompson et al. 2022 *Wildl. Soc. Bull.* (https://doi.org/10.1002/wsb.1322); Canadian Field-Naturalist biota review (https://www.canadianfieldnaturalist.ca/index.php/cfn/article/download/142/142), where the 0.29 and 0.40-0.80 figures were read in the PDF | read / snippet |
| F10 | Algonquin beaver-occupied ponds in a lowland-boreal study | < 2 ha each | "Impact of beaver foraging on structure of lowland boreal forests of Algonquin", via search summary | snippet |
| F11 | Algonquin 1893 boundary water share; lakes; streams | 10 % water; >2 400 lakes; 1 200 km streams | https://en.wikipedia.org/wiki/Algonquin_Provincial_Park | read |
| F12 | Algonquin fires | rarely exceed 1 ha | Canadian Field-Naturalist biota review (above) | read |
| F13 | Beaver dam frequency peak | gradient 0.01-0.03 (1-3 %), channel width 3-4 m, valley floor 25-35 m; meadows where valley > 4 stream widths | Suzuki & McComb 1998 and Pollock et al. 2003, as quoted in a Yellowstone paper and a Frontiers review (doi:10.3389/feart.2021.653623) | snippet; analogue (Pacific NW / Yellowstone) |
| F14 | Boreal plain pond median surface area | 1 419 m² (0.14 ha), negative-exponential distribution | Scientific Reports, https://www.nature.com/articles/s41598-020-73095-z (Riding Mountain NP, Manitoba) | snippet; analogue |
| F15 | Biebrza upper basin | 40 km long, 1.5-3.0 km wide | https://archiwum2.biebrza.org.pl/153,the-biebrza-national-park-geology | read |
| F16 | Biebrza middle / lower basin | 40 x 20 km / 30 km long, 12-15 km wide; oxbow zone along river 1-2 km wide | same | read |
| F17 | Biebrza park area, forest, marsh | 592.23 km²; forest 155.47 km²; marsh 254.94 km² | https://en.wikipedia.org/wiki/Biebrza_National_Park | read |
| F18 | Biebrza river | 164 km long; basin 7 092 km²; mean discharge 35.3 m³/s; "countless meanders" (no sinuosity number); riverbed "from 10 m" wide (truncated snippet) | https://en.wikipedia.org/wiki/Biebrza; park geology page; search snippet | read / snippet |
| F19 | Meandering threshold | sinuosity (channel / straight length) > 1.5 | generic definition, https://en.wikipedia.org/wiki/Meander and a search snippet | snippet; analogue |
| F20 | Voyageurs park | 883 km² (no forest or water share found) | https://en.wikipedia.org/wiki/Voyageurs_National_Park | read |

Disagreements: impounded share at Voyageurs is 13 % in Naiman 1988 (read) but 15.1 % in one search snippet;
I use 13 % (primary text). Pond counts for the peninsula: 741 (F8, 250 km²) vs 1 009 sites in 302 km² (another
snippet), both about 3 ponds/km², so they are consistent. Algonquin area is 7 723 km² (infobox) vs about 7 653 km²
(lead) on the same page; irrelevant to the targets.

## 2. Scale: cells and heights (a decision, not a finding)

The repo defines no physical scale (`worldgen.rs` uses 128 x 128 cells and "world units"). This document
proposes **1 cell = 10 m, 1 height unit = 1 m** (map 1.28 km square, 1 cell = 0.01 ha, relief 24 m).
Both are flags (`--cell-m`, `--height-m`) in the script. At 10 m a 3-4 m stream cannot be drawn narrower than one
cell, so river width is floored at 10 m (1 cell).

## 3. Targets (what the script checks)

| Metric (script name) | Target range | In cells (10 m) | Tier | Basis |
|---|---|---|---|---|
| Water fraction of map (`water_fraction`) | 0.05-0.15 | 5-15 % of cells | S | F1 13 % impounded (Voyageurs); F11 10 % water (Algonquin 1893) |
| Pond median area (`pond_median_area`) | 0.14-2.0 ha | 14-200 cells | S | F14 0.14 ha (analogue, low end); F10 < 2 ha (Algonquin, high end) |
| Pond mean area (`pond_mean_area`) | 0.5-4.6 ha | 50-460 cells | D | F1 / F2: 0.13 x 294 km² = 38.2 km² = 3 822 ha, divided by 835 dams = 4.6 ha per dam (includes meadow and woody impoundment, so an upper bound) |
| Pond spacing, nearest-centre median (`pond_spacing_median`) | 250-500 m | 25-50 cells | D | F3: 1 / (2.0-3.9 dams/km) = 256-500 m along a stream. The script measures nearest-neighbour distance, a proxy |
| Valley floor width (`valley_floor_width`) | 30-300 m | 3-30 cells | P | Lower bound F13 (25-35 m); upper bound is a design choice far below Biebrza F15 (1 500-3 000 m), which cannot fit a 1.28 km map. Median over main-river transects; floor ends where grade > 10 % |
| Valley depth, bed to rim (`valley_depth`) | 3-15 m | 3-15 units | P | No source found for any site |
| Valley-floor median slope (`floor_slope_median`) | 1-3 % | 0.1-0.3 units/cell | S | F13 gradient 0.01-0.03 (analogue) |
| Land slope, 90th percentile (`slope_p90`) | 8-25 % | 0.8-2.5 units/cell | P | No source found; the sim's MAX_STEP of 1.5 units/cell (15 %) is the walkability reference |
| Main-river sinuosity (`river_sinuosity`) | 1.1-1.6 | n/a | P | Only F18 "countless meanders" and F19's 1.5 threshold; no measured value for any of the three sites. Median over 30-cell windows |
| Main-river width (`river_width`) | 10-30 m | 1-3 cells | P | F13 3-4 m (below one cell, so floored); F18 "from 10 m" (Biebrza) |
| Forest fraction of dry land (`forest_fraction`) | 0.25-0.80 | n/a | S (weak) | F17 Biebrza park 155.47 / 592.23 = 26 % (lower bound; marsh is 43 %). The 0.80 upper bound is a design choice: no Voyageurs or Algonquin forest share was found |
| Clearing median area (`clearing_median_area`) | 0.5-5 ha | 50-500 cells | P | F7 (12-15 % of riparian upland browsed) and F12 (fires at most 1 ha) bound the idea only; no measured clearing size. Clearing = connected dry non-forest cells |

Replace the P rows when site GIS data is obtained. Likely sources: Minnesota DNR / USGS 3DEP LiDAR for
Kabetogama, the Ontario provincial DEM for Algonquin, Polish GUGiK LiDAR for Biebrza, and the Johnston &
Windels 2015 pond shapefiles on HydroShare (resource 20f1b8be86d74d198848a6d254556a40). LiDAR would give valley
width, depth and slope directly. Such real data stays in development folders (e.g. `tmp/`); `crates/game` and
`crates/sim` read no real-data file, and this task added none. The only new Rust code is
`crates/sim/examples/terrain_dump.rs`, which writes a generated map and reads nothing but its arguments.

## 4. Current generator vs targets (seeds 1 and 2, 2026-11-17)

Run: `cargo run -p sim --example terrain_dump -- --seed 1 --out tmp/t1.json`, then
`python tools/terrain_metrics.py tmp/t1.json` (needs numpy and scipy; `--strict` exits 1 if any metric is out of range).

* In range for both seeds: pond median, valley depth, p90 slope, sinuosity, river width, forest fraction.
* Out of range for both: pond mean area (0.15 and 0.30 ha vs 0.5-4.6), floor slope (0.5 and 0.7 % vs 1-3),
  clearing size (0.06 and 0.08 ha vs 0.5-5).
* Seeds differ: water fraction (0.051 vs 0.049), pond spacing (622 vs 100 m), valley floor width (410 vs 240 m).
  Each map has only 2 ponds, so spacing is noisy.

Read these as tuning hints, not verdicts: half the ranges are placeholders.
