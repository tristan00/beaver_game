# Terrain targets from real beaver landscapes

Purpose: numeric ranges for tuning `crates/sim/src/worldgen.rs`, checkable with
`tools/terrain_metrics.py` instead of screenshots. Sites: Voyageurs NP (Minnesota), Algonquin PP (Ontario) and
Biebrza NP (Poland). Researched 2026-11-17.

**How the targets were made.** No paper I could open gives valley width or depth, slope distributions,
sinuosity, channel width or clearing size for these three sites. The main papers are paywalled; see section 5.
So I measured the twelve metrics myself on open data for each site, with `tools/real_landscape.py`.
That script lays a 10 km x 10 km grid of 10 m cells over each site and runs the **same metric code**
(`terrain_metrics.measure`) that scores a generated map. Every target below is the span of those site
measurements (M1-M3). Section 4 checks them against the published facts I could read (F1-F20).

## 1. Scale (a decision, not a finding)

The repo defines no physical scale: `worldgen.rs` uses 128 x 128 cells and "world units". This doc sets
**1 cell = 10 m and 1 height unit = 1 m**. The map is then 1.28 km square, 1 cell is 0.01 ha, and the relief
amplitude is 24 m. Both are flags in the script (`--cell-m`, `--height-m`). The real-data grids use the same
10 m cell. River width cannot be drawn narrower than one cell (10 m).

## 2. Data used for the site measurements (development only; nothing ships)

| Id | Dataset | What it gives | Citation / location |
|---|---|---|---|
| D1 | Copernicus DEM GLO-30 (2021 release), DSM | heights at 30 m. This is a surface model, so it includes the tree canopy | ESA / Airbus, doi:10.5270/ESA-c5d3d65; tiles `Copernicus_DSM_COG_10_{N48_00_W093_00, N45_00_W079_00, N53_00_E022_00}_DEM.tif` from `s3://copernicus-dem-30m` (https://copernicus-dem-30m.s3.amazonaws.com) |
| D2 | USGS 3DEP 1/3 arc-second DEM, DTM | bare-earth heights at about 10 m (Voyageurs) | U.S. Geological Survey, tile `USGS_13_n49w093.tif`, https://prd-tnm.s3.amazonaws.com/StagedProducts/Elevation/13/TIFF/current/n49w093/USGS_13_n49w093.tif |
| D3 | NRCan Medium Resolution DEM (MRDEM-30), DTM | bare-earth heights at 30 m (Algonquin); EPSG:3979 | Natural Resources Canada, https://canelevation-dem.s3.ca-central-1.amazonaws.com/mrdem-30/mrdem-30-dtm.tif (only the tiles under the window are range-read) |
| D4 | ESA WorldCover 10 m 2021 v200 | land cover at 10 m: class 10 = tree cover, 80 = permanent water | Zanaga et al. 2022, doi:10.5281/zenodo.7254221; tiles `ESA_WorldCover_10m_2021_v200_{N48W093, N45W081, N51E021}_Map.tif` from https://esa-worldcover.s3.eu-central-1.amazonaws.com |
| D5 | OpenStreetMap | `waterway=river/stream/canal` lines; water areas (`natural=water`, `waterway=riverbank`) | © OpenStreetMap contributors, ODbL; Overpass API, retrieved 2026-11-17 |

Windows, each 10 x 10 km centred on: **Voyageurs** 48.52 N, 92.86 W (Kabetogama Peninsula, Cranberry Creek as the
main river); **Algonquin** 45.56 N, 78.42 W (Madawaska River headwaters); **Biebrza** 53.53 N, 22.82 E (middle
basin, Biebrza river). Water is the union of WorldCover class 80 and OSM water areas. Rivers are the OSM lines
drawn into cells. Ponds are water bodies not cut by the window edge. Valley metrics use only river cells
outside WorldCover water, because in the game the main river never crosses a pond.

Reproduce with `python tools/real_landscape.py --data tmp/realdata` (needs numpy, scipy, tifffile, imagecodecs,
requests and Pillow). Place the D1, D2 and D4 tiles in `tmp/realdata/dl/` first; D3 and D5 are fetched and cached
automatically. Log of the run behind this doc: `tmp/real_final.txt`, not committed.

## 3. Site measurements (M1-M3)

Same definitions as `terrain_metrics.py`. Each height-based metric is given as main river / all OSM streams.
For height-based metrics the bare-earth DTM is the value used. The DSM value follows in brackets because
canopy inflates it (see the disagreements below).

| Metric (unit) | M1 Voyageurs | M2 Algonquin | M3 Biebrza (DSM only) |
|---|---|---|---|
| water_fraction (of window) | 0.284 (includes cut-off parts of Kabetogama Lake) | 0.113 | 0.014 |
| pond_median_area (ha) | 0.435 (n = 150) | 0.62 (n = 143) | 0.13 (n = 51) |
| pond_mean_area (ha) | 2.90 | 2.80 | 0.42 |
| pond quartiles (ha) | 0.11-1.66 | 0.17-2.01 | 0.08-0.28 |
| pond density (per km²) | 1.50 | 1.43 | 0.51 |
| pond_spacing_median (m, nearest pond centre) | 261 | 268 | 155 |
| valley_floor_width (m) | 80 / 120 (DSM 50 / 45) | 225 / 130 (DSM 10 / 10) | 1 110 / 1 190, i.e. wider than the 600 m search on each side |
| valley_depth (m, bed to first rim) | 22.1 / 11.8 (DSM 28.2 / 14.8) | 11.5 / 23.4 (DSM 11.7 / 16.9) | 0.66 / 0.87 (rim not reached within 600 m) |
| floor_slope_median (% grade) | 2.6 / 2.6 (DSM 11.7 / 9.8) | 5.6 / 6.5 (DSM 14.5 / 14.8) | 1.1 / 0.9 |
| slope p50 / p75 / p90 / p95 (% grade, land) | 6.7 / 11.9 / 18.5 / 23.8 (DSM p90 20.6) | 10.5 / 16.8 / 24.2 / 30.1 (DSM p90 29.7) | 1.3 / 3.1 / 6.8 / 10.7 |
| relief, 1st to 99th height percentile (m) | 64.7 | 132.2 | 35.3 |
| river_sinuosity (300 m windows) | 1.09 / 1.11 | 1.22 / 1.16 | 1.15 / 1.22 |
| river_width (m, water across the main river) | 0: Cranberry Creek is narrower than one 10 m pixel | 0 outside lakes, narrower than 10 m | 30 |
| forest_fraction (WorldCover tree cover / land) | 0.926 | 0.980 | 0.222 |
| clearing_median_area (ha, dry non-tree patches ≥ 0.1 ha) | 0.43 (n = 415) | 0.30 (n = 220) | 0.33 (n = 95) |
| same, all patches including 1-pixel gaps (ha) | 0.04 | 0.04 | 0.03 |

## 4. Targets (what `terrain_metrics.py` checks)

Each range is the min..max of the site measurements in section 3, rounded outward to two significant figures.
Where a site was left out, the reason is given. Values in cells assume 10 m cells and 1 m height units.

| Metric (script name) | Target | In cells / units | Source values (value, unit) |
|---|---|---|---|
| Water fraction of map (`water_fraction`) | 0.10-0.28 | 10-28 % of cells | M2 0.113, M1 0.284 (fraction of window). Matches F1, 13 % of 294 km² impounded (Voyageurs), and F11, 10 % water (Algonquin). M3 0.014 left out: Biebrza's wetland is marsh, not open water |
| Pond median area (`pond_median_area`) | 0.13-0.62 ha | 13-62 cells | M3 0.13 ha, M1 0.435 ha, M2 0.62 ha |
| Pond mean area (`pond_mean_area`) | 0.42-2.9 ha | 42-290 cells | M3 0.42 ha, M2 2.80 ha, M1 2.90 ha. F1/F2 give 4.6 ha impounded per dam, which is an upper bound |
| Pond spacing, nearest-centre median (`pond_spacing_median`) | 150-270 m | 15-27 cells | M3 155 m, M1 261 m, M2 268 m. F3 gives 256-500 m between dams along a stream, a different measure |
| Valley floor width (`valley_floor_width`) | 80-230 m | 8-23 cells | M1 80 m (main) / 120 m (streams), M2 225 m / 130 m (DTM). M3 > 1 100 m left out: it does not fit a 1.28 km map (F15: upper basin 1.5-3.0 km wide) |
| Valley depth, bed to rim (`valley_depth`) | 11-24 m | 11-24 units | M2 11.5 m / 23.4 m, M1 22.1 m / 11.8 m (DTM). M3 left out: its rim lies beyond 600 m |
| Valley-floor median slope (`floor_slope_median`) | 0.9-6.5 % | 0.09-0.65 units/cell | M3 0.9 % / 1.1 %, M1 2.6 %, M2 5.6 % / 6.5 % (DTM for M1 and M2) |
| Land slope, 90th percentile (`slope_p90`) | 6.8-24 % | 0.68-2.4 units/cell | M3 6.8 %, M1 18.5 %, M2 24.2 % |
| Main-river sinuosity (`river_sinuosity`) | 1.09-1.22 | ratio | M1 1.09 / 1.11, M3 1.15 / 1.22, M2 1.22 / 1.16. On 300 m windows; the reach-scale value is higher (F18 "countless meanders"; F19 meandering means ≥ 1.5) |
| Main-river width (`river_width`) | 10-30 m | 1-3 cells | M3 30 m. M1 and M2 are narrower than 10 m and floored to one 10 m cell |
| Forest fraction of dry land (`forest_fraction`) | 0.90-0.98 | fraction | M1 0.926, M2 0.980. M3 0.222 left out as the open-marsh case (F17: Biebrza park 155.47 of 592.23 km² forest = 0.26) |
| Clearing median area (`clearing_median_area`) | 0.30-0.43 ha | 30-43 cells | M2 0.30 ha, M3 0.33 ha, M1 0.43 ha. Clearing = connected dry non-forest patch ≥ 0.1 ha (10 cells). Smaller gaps are mostly 1-pixel land-cover noise; their median is 0.03-0.04 ha at all three sites |

### Disagreements and limits

* **DSM vs DTM.** At Voyageurs, D1 (DSM) and D2 (DTM) can be compared on the same grid. The DSM gives
  floor slope 11.7 % against 2.6 % and slope p90 20.6 % against 18.5 %. Its valley floor is narrower
  (50 m against 80 m) because canopy edges read as walls. Algonquin shows the same pattern (floor slope 14.5 %
  against 5.6 %, floor width 10 m against 225 m). So height targets use only the DTMs (D2, D3). Biebrza has only
  the DSM; its window is 22 % forest, so the bias is smaller. Its 0.9-1.1 % floor slope is probably DSM noise, since
  the river itself falls far less. That makes the lower end of the floor-slope range weakly supported.
* **Main river vs all streams.** The two disagree by up to a factor of 2 for valley width and depth (for example
  Algonquin depth 11.5 m vs 23.4 m). Both are inside the target ranges.
* **Literature vs measurement.** F1 (13 % of the Voyageurs peninsula impounded) lies between M2 and M1.
  F3 (dams 256-500 m apart along streams) is wider than the measured nearest-pond spacing, as expected:
  ponds on different streams can be closer to each other than dams on one stream.
* **One window per site.** Each value comes from a single 10 km window and a single run. Treat the ranges as
  ±20 % estimates, not tolerances.
* **Sizes below one pixel are invisible.** Water and land cover come from 10 m data, so creeks narrower than
  10 m and ponds smaller than 0.01 ha are missed. River width is therefore floored at 1 cell, the same floor
  the game has.

## 5. Published facts (read at the source)

| # | Fact | Value, unit | Source | Status |
|---|---|---|---|---|
| F1 | Voyageurs, Kabetogama Peninsula area impounded by beaver, 1940 to 1986 | <1 % to 13 % of 294 km² | Naiman, Johnston & Kelley 1988, *BioScience* 38(11):753-762, p. 758 (JSTOR copy) | read |
| F2 | Voyageurs dams, 1940 to 1986 | 71 to 835 dams over >300 km of stream | same | read |
| F3 | Voyageurs dam frequency | 2.0-3.9 dams/km, mean 2.5 dams/km | same, p. 755 | read |
| F4 | Quebec North Shore (analogue) dam frequency | 8.6-16.0 dams/km, mean 10.6 | same (cites Naiman et al. 1986) | read; analogue |
| F5 | Colony density in favourable habitat | 0.4-0.8 colonies/km², up to 3 | same (cites Aleksiuk 1968 etc.) | read |
| F6 | One large Voyageurs dam | 80 m long, 2 m high, about 1 km² influenced | same, Fig. 1 caption | read; outlier |
| F7 | Riparian uplands altered by browsing, Voyageurs | 12-15 % of uplands | same | read |
| F11 | Algonquin at its 1893 boundary: water share, lakes, streams | 10 % water; >2 400 lakes; 1 200 km of streams | https://en.wikipedia.org/wiki/Algonquin_Provincial_Park | read |
| F15 | Biebrza upper basin | 40 km long, 1.5-3.0 km wide | https://archiwum2.biebrza.org.pl/153,the-biebrza-national-park-geology | read (re-checked 2026-11-17) |
| F16 | Biebrza middle / lower basin | 40 x 20 km / 30 km long, 12-15 km wide; oxbows in a 1-2 km zone along the river | same | read (re-checked) |
| F17 | Biebrza park area, forest, marsh | 592.23 km²; forest 155.47 km²; marsh 254.94 km² | https://en.wikipedia.org/wiki/Biebrza_National_Park | read |
| F18 | Biebrza river | 164 km long; basin 7 092 km² (pl.wikipedia: about 165 km, 7 051 km²); "countless meanders", no sinuosity number | https://en.wikipedia.org/wiki/Biebrza, https://pl.wikipedia.org/wiki/Biebrza | read (re-checked) |
| F19 | Meandering threshold | "single channel and sinuosities of 1.5 or more" | https://en.wikipedia.org/wiki/Meander | read (re-checked) |
| F20 | Voyageurs park area | 883 km² (no forest or water share given) | https://en.wikipedia.org/wiki/Voyageurs_National_Park | read |

The disagreements among these sources do not affect any target. One search snippet gave 15.1 % impounded at
Voyageurs, against F1's 13 %; F1 is the primary text. Algonquin's area is 7 723 km² in the Wikipedia infobox
and about 7 653 km² in the lead. The Biebrza length is 164 km on en.wikipedia and about 165 km on pl.wikipedia.

**Earlier citations withdrawn.** None of these were checked at the source, and no target uses them:

* Johnston & Naiman 1990, *Landscape Ecol.* 4:5-19 (741 ponds in 250 km²): Springer login wall.
* Thompson et al. 2022, *Wildl. Soc. Bull.* doi:10.1002/wsb.1322 (Algonquin 0.55 colonies/km²): Wiley returned 403.
* Donkor & Fryxell 1999 (Algonquin ponds < 2 ha): ScienceDirect returned 403.
* Sci. Rep. s41598-020-73095-z (median pond 1 419 m²): Nature login wall.
* The Canadian Field-Naturalist Algonquin review (0.29 and 0.40-0.80 colonies/km², fires ≤ 1 ha): the PDF
  text could not be extracted on re-check.
* The "1-3 % gradient, 3-4 m channel, 25-35 m valley floor" figures attributed to Suzuki & McComb 1998 and
  Pollock et al. 2003 via doi:10.3389/feart.2021.653623: the full text of that review was read, and it
  contains neither the numbers nor either reference. Only the snippet made the attribution, so it is withdrawn.

## 6. Real data stays out of the game

Real data is read only by `tools/real_landscape.py` and lives under `tmp/realdata/`, which is git-ignored.
`crates/game` and `crates/sim` contain no file reads of real data; `grep` finds no `File::open`, `fs::read`,
`.tif` or `realdata` in `crates/`. The only Rust addition is `crates/sim/examples/terrain_dump.rs`, which writes
a generated map (now including `flow_acc`) and reads nothing but its arguments.

## 7. Current generator vs targets (seeds 1 and 2, 2026-11-17)

```
cargo run -p sim --example terrain_dump -- --seed 1 --out tmp/t1.json
python tools/terrain_metrics.py tmp/t1.json          # --strict exits 1 if any metric is out of range
```

| Metric | seed 1 | seed 2 | Target |
|---|---|---|---|
| water_fraction | 0.051 LOW | 0.049 LOW | 0.10-0.28 |
| pond_median_area (ha) | 0.15 OK | 0.30 OK | 0.13-0.62 |
| pond_mean_area (ha) | 0.15 LOW | 0.30 LOW | 0.42-2.9 |
| pond_spacing_median (m) | 622 HIGH | 100 LOW | 150-270 |
| valley_floor_width (m) | 410 HIGH | 240 HIGH | 80-230 |
| valley_depth (m) | 5.8 LOW | 13.0 OK | 11-24 |
| floor_slope_median (%) | 0.50 LOW | 0.69 LOW | 0.9-6.5 |
| slope_p90 (%) | 16.3 OK | 17.8 OK | 6.8-24 |
| river_sinuosity | 1.28 HIGH | 1.18 OK | 1.09-1.22 |
| river_width (m) | 10 OK | 10 OK | 10-30 |
| forest_fraction | 0.43 LOW | 0.40 LOW | 0.90-0.98 |
| clearing_median_area (ha) | 0.43 OK | 0.70 HIGH | 0.30-0.43 |

Compared with the real sites, the generator makes too little water and too few, too small ponds (two per map).
Its valleys are too wide and flat-floored, and it has far too little forest. Each map has only two ponds, so
pond spacing is noisy.
