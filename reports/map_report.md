# The four real maps: realism and gameplay

Maps: **Algonquin** (Ontario), **Voyageurs** (Minnesota), **Bavarian Forest** (Bavaria), **Telemark** (Norway).
Date: 2026-11-22. Code base: commit `ebab563` plus the uncommitted work in the tree at the time (see *Method and provenance* at the end).

## Summary

**Ranking**

1. **Telemark**: the best mix of challenge and dam opportunities. It has 17 distinct dam sites (3 within 60 cells of camp). Its designed dam is the cheapest (224 sticks) and more than doubles the open water (947 to 2,159 wet cells). Food is scarce and 67% of trees are conifers, so the player has real decisions to make. It also looks like a believable Nordic conifer valley.
2. **Algonquin**: the richest and most forgiving map. It has the most water (13.9% of the map), the most trees, food, mud and pebbles near camp, and the best colony results (pop 10, about 125 days of food at year end). But its showpiece dam is slow: 460 sticks and about 185 days to fill. It also delivers only 56% of the pond its preview promises.
3. **Voyageurs**: the most beaver-like scenery (a braided wetland) and the most buildable shoreline. But **the designed dam is 303 cells from camp and unexplored**, so it is refused at start. There are no dam sites within 120 cells, and the colony does best without a dam. The game's core mechanic is missing on this map.
4. **Bavarian Forest**: the camp sits in a cleared meadow with only 63 trees within 40 cells. It has the slowest economy (warehouse finished on day 238), no immigrants under autoplay, local tree cover falling from 63 to 12–24 trees, and a tiny designed pond (341 m², 0.7 m rise). The lidar terrain also shows roads, embankments and building pads.

**Most important findings**

1. **Hands-off play dies the same way on every map.** All 8 beavers die of Cold (4 on day 325.6, 4 on day 347.9) on all four maps. Food never ran out, and nothing is ever built because nobody places a shelter. Without a player the map makes no difference at all.
2. **The planned first dams are taller than a one-row dam can be built.** `HEIGHT_PER_ROW = 1.0 m`, but the start planner proposes 1.5 m (Algonquin) and 1.2 m (Telemark, Voyageurs). Those dams stop at 87% and 93% built. The ponds reach 2,276 m² instead of the 4,051 m² previewed, and 1,294 m² instead of 2,365 m².
3. **Drawing the designed dam on day 0 leaves the family homeless for months.** The dam takes every stick (100 in the starting pile; the dam needs 224–460). The first shelter was finished on day 203 (Algonquin), 255 (Bavarian) and 148 (Telemark), compared with day 20–48 when the shelter comes first.
4. **Most map differences come from what lies near the camp, not from the map as a whole.** Tree density (471–506 trees per hectare), tree cover (4.7–5.1% of cells), clay deposits (exactly 8 per map) and pebble beds (375–423) are nearly the same on every map. Food plants and trees within 40 cells of camp, though, range from 128 to 451 shoots and 63 to 197 trees. Those numbers predict the order the colonies grew in.
5. **The second half of the year has little to do.** Attentive autoplay logs 68–157 days with the "Idle" alert. All buildings finish by about day 230 (day 287 on Bavarian), and after that no map offers a new goal.

---

## 1. Map metrics

All four maps are 512 × 512 cells at 1 m per cell, so each covers **0.512 km × 0.512 km (26.2 ha)**. They are built from 1 m lidar terrain models plus ESA WorldCover 2021 land cover. Unless noted otherwise, the numbers below are measured on the sim's own world and early-game state: the new game the game itself builds (`starts::new_game_on_map`), after the water spin-up. "Camp" is the designed camp. A "cell" is 1 m.

### 1.1 Terrain

| | Algonquin | Voyageurs | Bavarian | Telemark |
|---|---|---|---|---|
| Latitude | 45.6° N | 48.5° N | 49.0° N | 59.5° N |
| Elevation range p1–p99 (full range) | 23.4 m (28.6) | 14.9 m (17.1) | 32.7 m (39.0) | 30.9 m (38.5) |
| Mean / median land slope | 8.6% / 5.8% | 8.8% / 6.2% | 12.5% / 9.6% | 14.2% / 9.6% |
| Land steeper than 10% / 30% / 60% | 29% / 3.0% / 0.1% | 31% / 3.3% / 0.2% | 48% / 6.4% / 0.7% | 48% / 10.5% / 1.9% |
| Median / 90th percentile distance from land to water | 43 / 139 m | 57 / 190 m | 68 / 159 m | 65 / 184 m |

What this means for play: on Bavarian and Telemark, steeper ground means fewer flat building spots by the water and taller banks to tie a dam into. That gives Telemark many short, tall dam sites. On Voyageurs and Algonquin, gentle land means water spreads out in shallow sheets.

### 1.2 Water

| | Algonquin | Voyageurs | Bavarian | Telemark |
|---|---|---|---|---|
| Inflows (count, catchment km²) | 2 (3.45) | 3 (1.14) | 4 (1.70) | 2 (1.14) |
| River cells / traced length | 1,002 / 1.29 km | 1,313 / 1.67 km | 1,408 / 1.71 km | 1,187 / 1.44 km |
| River heads in the map | 2 | 3 | 4 | 2 |
| Lakes in the world grid | 0 | 0 | 1 (365 cells, isolated) | 0 |
| **Wet area at start (depth > 5 cm)** | **13.9%** (36,477 cells) | 2.3% (6,093) | 0.96% (2,516) | **0.35%** (926) |
| Depth median / p90 / max | 0.56 / 2.56 / 3.01 m | 0.10 / 0.55 / 1.10 m | 0.17 / 0.60 / 1.01 m | 0.07 / 0.14 / 0.38 m |
| Flow speed at start, p90 / max | 0.00 / 0.75 m/s | 0.04 / 0.22 m/s | 0.08 / 0.43 m/s | 0.17 / 0.29 m/s |
| Peak speed over a year, p99 / max | 0.93 / 1.12 m/s | 0.26 / 0.39 m/s | 0.57 / 0.77 m/s | 0.43 / 0.47 m/s |
| Wet cells across the year (min–max of the 30-day samples) | 35,020–36,510 | 4,359–6,382 | 2,017–2,793 | 437–1,173 |

The seasonal water figures come from a year of the water model with no colony, sampled every 30 days. Every map peaks in early spring (days 30–60) and is lowest around day 300.

- **Algonquin** is a lake landscape. Its large 3.45 km² inflow fills a lake in the south-west corner and a chain of ponds along the creek. 90% of the water is still (p90 speed 0.00), with only short fast reaches.
- **Telemark** is the opposite: narrow, shallow, fast streams (median depth 7 cm). Its wet area swings by a factor of 2.7 over the year. Here, and only here, a dam visibly changes the map.
- **Voyageurs** has the longest river network: three inflows and the widest braided wetland, all shallow.
- **Bavarian** has the most river heads (4) but the least water per stream. Its only lake (365 cells) is held up by a straight road embankment (see Realism).

### 1.3 Flood risk

| | Algonquin | Voyageurs | Bavarian | Telemark |
|---|---|---|---|---|
| Cells that became wet during the year but were dry at start | 97 | 798 | 370 | 353 |
| ...of which within 30 cells of camp | 35 | 71 | 13 | 25 |
| Camp 5×5 area ever wet | 0 | 0 | 0 | 0 |
| Camp height above the lowest water within 25 cells | 0.30 m | 0.16 m | 0.92 m | 0.63 m |

No camp floods. With the built-in climate, flooding is not a threat on any map. Voyageurs has the most seasonal spread of water, and its camp sits only 16 cm above the nearby water, yet even there the camp stayed dry. Every map uses the same climate model (`water/climate.rs`), so the only differences come from the terrain.

### 1.4 Dam sites and pond yield

Dam sites were counted with the game's own start-dam search (`starts::dam_candidates`) over the whole map. Candidates less than 20 cells apart were merged into one site. The search only accepts ponds of 300–6,000 cells and aims for about 1,500 cells, so pond sizes are capped by design.

| | Algonquin | Voyageurs | Bavarian | Telemark |
|---|---|---|---|---|
| Distinct dam sites on the map | 9 | 3 | 6 | **17** |
| Within 60 / 120 cells of camp | 3 / 5 | **0 / 0** | 1 / 1 | 3 / 5 |
| Pond area per site, median / max | 1,521 / 2,314 m² | 1,137 / 1,570 m² | 485 / 584 m² | 1,389 / 1,651 m² |
| **Designed dam**: cells, height | 23 cells, 1.5 m | 25 cells, 1.2 m | 19 cells, 0.8 m | 14 cells, 1.2 m |
| Distance from camp to dam | 38 cells | **303 cells** | 58 cells | 26 cells |
| Previewed pond, rise | 4,051 m², 1.37 m | 717 m², 1.15 m | 341 m², 0.70 m | 2,365 m², 1.15 m |
| Cost, days to fill | 460 sticks, 185 d | 400 sticks, 67 d | 228 sticks, 1.3 d | 224 sticks, 27 d |
| **Pond actually reached in a year** | 2,276 m² (dam 87% built) | not buildable | 326 m² (100%) | 1,294 m² (93%) |

What this means for play:

- **Telemark** gives the most choice. It has the most sites, and its designed dam is short, cheap and quick to fill. It raised the map's total wet area from 947 to 2,159 cells.
- **Algonquin's** designed dam is impressive on paper. But it costs 4.6 times the starting sticks and needs half a year to fill, and the one-row height cap (1.0 m) leaves it short of the 1.5 m target.
- **Bavarian's** dam fills in about a day and floods only 326 m², which is nearly invisible next to the 365-cell embankment pond already there.
- **Voyageurs** has no usable dam. The designed dam was refused as "Unexplored" in every run. Attentive autoplay retried it every 10 days, 35 times between day 0 and day 352, and never fell back to a nearer site. There is none within 120 cells anyway.

**Canals.** All four maps have many valid straight canal routes from water within 30 cells of camp: 466 (Algonquin), 709 (Voyageurs), 396 (Bavarian) and 311 (Telemark). But the best one reaches only 16–19 trees within 4 cells of its route, and none reaches 20. On all four maps the forest already comes close enough to the water that a canal adds little.

### 1.5 Forest

| | Algonquin | Voyageurs | Bavarian | Telemark |
|---|---|---|---|---|
| Trees (per ha); share of cells with a tree | 13,255 (506); 5.1% | 12,972 (495); 5.0% | 12,748 (486); 4.9% | 12,342 (471); 4.7% |
| Stages: young / mature / old | 15 / 75 / 10% | 15 / 75 / 10% | 15 / 75 / 10% | 15 / 75 / 10% |
| Species | Maple 32, Pine 18, Aspen 14, Birch 12, Beech 10, Alder 5, Spruce 5, Willow 3 | Aspen 35, Pine 27, Birch 19, Spruce 7, Maple 6, Alder 4, Willow 2 | Spruce 42, Beech 32, Maple 9, Birch 4, Pine 4, Aspen 3, Alder 3, Willow 2 | Pine 36, Spruce 31, Birch 21, Aspen 7, Alder 3, Willow 2 |
| Conifer share (no bark food) | 23% | 34% | 46% | **67%** |
| Trees within 20 / 40 / 80 cells of camp | 44 / **197** / 988 | 7 / 78 / 689 | 10 / **63** / 337 | 29 / 131 / 720 |
| Trees within 40 cells at year end (attentive / planned) | 164 / 124 | 51 / 52 | 24 / **12** | 75 / 44 |

The species mixes are realistic for each region: maple and pine in Algonquin, aspen in the Minnesota boreal fringe, a spruce and beech mountain forest in Bavaria, and pine and spruce in Norway. Density, though, is essentially fixed by the generator (471–506 trees per hectare everywhere). The land cover layer moves trees around the map but barely changes how many there are. Telemark's 67% conifers matter for play, because conifers give no bark food. Bavarian's camp clearing matters even more: by year end the colony had cut the trees within 40 cells down to 12–24, and the "ForestFalling" alert appeared.

### 1.6 Food, clay, mud and pebbles

| | Algonquin | Voyageurs | Bavarian | Telemark |
|---|---|---|---|---|
| Land shoots: count (food amount) | 2,395 (16,765) | 2,991 (20,937) | 2,570 (17,990) | 2,004 (14,028) |
| Shoots within 40 cells of camp | 392 | **451** | 322 | **128** |
| Plant food within 30 / 60 / 100 cells | 1,820 / 4,326 / 6,951 | 1,981 / 4,676 / 7,112 | 1,309 / 3,878 / 7,364 | **560** / 1,841 / 4,249 |
| Water lilies after a year (map / within 40) | **466** / 19 | 112 / 13 | 70 / 7 | 44 / 6 |
| Mud banks: count, nearest, within 40 | 160, 12 m, **42** | 111, 11 m, 3 | 153, 28 m, **2** | 136, 9 m, 23 |
| River clay: count, nearest | 8, 52 m | 8, 27 m | 8, 8 m | 8, 21 m |
| Pebble beds: count, nearest, within 40 | 419, 20 m, 49 | 423, 41 m, 0 | 382, 48 m, 0 | 375, 24 m, 18 |

Water lilies are seeded only after the water settles, so the counts above come from the end of the year. Land shoots double over the first spring (Algonquin: 2,398 to 5,402).

What this means for play: Algonquin's camp has everything nearby. Telemark's camp is food-poor, with 31% of Algonquin's plant food within 30 cells. Bavarian's camp lacks both mud and trees. Every map has exactly 8 river-clay deposits, and pebble counts are nearly the same everywhere. These materials do not set the maps apart.

### 1.7 Room to build

| | Algonquin | Voyageurs | Bavarian | Telemark |
|---|---|---|---|---|
| Valid shelter spots within 30 cells of camp | 1,065 | 978 | 433 | **365** |
| Valid shelter spots map-wide (every 2nd cell) | **10,430** | 2,998 | 1,626 | 951 |
| Dry, clear land under 15% slope within 8 m of water | 7.3% | **11.7%** | 5.8% | 5.3% |
| Same, within 40 cells of camp | 1,879 | 2,391 | 1,438 | 1,318 |
| Site score at camp (verdict) | 0.72 (Excellent) | 0.77 (Excellent) | 0.74 (Excellent) | 0.77 (Excellent) |

All four camps score "Excellent", even though the colonies on them grow very differently. The score's tree component is 0.0 on three of the four maps, and the score never looks at how far away the dam is (303 cells on Voyageurs). So the score does not predict how a map plays.

---

## 2. Year-long strategy runs

A year is 360 days (four 90-day seasons, `calendar.rs`). Every run covers days 0–360 from the designed camp. The four strategies:

- **Hands-off**: `policy handsoff`. The colony is settled and then left alone (automatic job staffing stays on).
- **Attentive**: `policy attentive`. The built-in autoplayer places a shelter, the dam (designed, or one near camp), warehouse, larder, forester, logistics and extra shelters, and applies the top alert's fix.
- **Expansion**: a fixed script with no reactions to alerts. It draws the designed dam and a shelter on day 0, then places a warehouse (day 5), larder (12), forester (25) and shelters (60, 120).
- **Planned**: a fixed script, shelter first. It places a shelter and adds a builder on day 0, then a warehouse (day 3), larder (10), designed dam plus one more logger (30), forester (45), the best canal from the canal search (60), and shelters (90, 150).

### 2.1 Results at day 360

Abbreviations: "Home" is the day the first shelter was finished, "Ware" the day the warehouse was finished, "Homeless" the number of days some beaver had no home, "Food" the stock at year end as days of current need, "Dam" how much of the designed dam was built.

| Map | Strategy | Pop | Births | Deaths | Immigr. | Food (days) | Min food | Homeless | Home / Ware | Dam / pond | Sticks / Clay |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Algonquin | hands-off | **0** (day 348) | 4 | 8 Cold | 0 | 55.9 (–) | 13.8 | 348 | – / – | – | 100 / 30 |
| | attentive | 10 | 4 | 0 | 2 | 109 (125) | 14.7 | 21 | 21 / 65 | 87% / 2,271 | 50 / 36 |
| | expansion | 8 | 4 | 0 | 0 | 74 (119) | 13.8 | **203** | 203 / 218 | 87% / 2,276 | 49 / 37 |
| | planned | 10 | 4 | 0 | 2 | 111 (127) | 14.7 | 20 | 20 / 37 | 87% / 2,276 | 250 / 38 |
| Voyageurs | hands-off | **0** (day 348) | 4 | 8 Cold | 0 | 61.2 (–) | 13.8 | 348 | – / – | – | 100 / 30 |
| | attentive | 10 | 4 | 0 | 2 | 101 (116) | 14.7 | 23 | 23 / 31 | none drawn | 52 / 34 |
| | expansion | 10 | 4 | 0 | 2 | 103 (117) | 13.8 | 34 | 34 / 24 | refused (Unexplored) | 46 / 34 |
| | planned | 10 | 4 | 0 | 2 | 106 (121) | 14.7 | 43 | 43 / 42 | refused (Unexplored) | 250 / 35 |
| Bavarian | hands-off | **0** (day 348) | 4 | 8 Cold | 0 | 54.4 (–) | 14.7 | 348 | – / – | – | 100 / 30 |
| | attentive | **8** | 4 | 0 | **0** | 67 (106) | 13.8 | 48 | 48 / **238** | 100% / 326 | 52 / 32 |
| | expansion | 8 | 4 | 0 | 0 | 64 (102) | 14.7 | **255** | 255 / 262 | 100% / 326 | 50 / 34 |
| | planned | 10 | 4 | 0 | 2 | 98 (112) | 13.8 | 39 | 39 / 21 | 100% / 326 | 244 / 36 |
| Telemark | hands-off | **0** (day 348) | 4 | 8 Cold | 0 | 49.3 (–) | 13.8 | 348 | – / – | – | 100 / 30 |
| | attentive | 10 | 4 | 0 | 2 | 94 (108) | 13.8 | 48 | 48 / 171 | 93% / 1,294 | 60 / 42 |
| | expansion | 10 | 4 | 0 | 2 | 82 (93) | 13.8 | **148** | 148 / 163 | 93% / 1,294 | 49 / 37 |
| | planned | 10 | 4 | 0 | 2 | 101 (115) | 13.8 | 40 | 40 / 21 | 93% / 1,294 | 250 / 36 |

Notes:

- No run ever ran out of food. The lowest food stock in every run (13.8–14.7) was the starting larder itself, and food need was 0.63–0.88 a day. "Sticks 250" means the store was full.
- All kits were born on day 47, four per run. Every death was from Cold.
- **Attentive build order:**
  - Algonquin: dam on day 0, shelter 21, warehouse 65, larder 147, shelter 187, forester 215, logistics 224.
  - Voyageurs: shelter 23, warehouse 31, larder 45, forester 62, logistics 65, shelters 116 and 132.
  - Bavarian: dam on day 0, shelter 48, warehouse 238, shelter 264, larder 275, forester 282, logistics 287.
  - Telemark: dam on day 0, shelter 48, warehouse 171, shelter 196, larder 213, forester 224, logistics 225.
- **Days with each alert, attentive runs**:
  - Algonquin: Idle 113, Stranger 17, NoBuilders 6, WinterFood 6.
  - Voyageurs: Idle 157, Stranger 16.
  - Bavarian: Idle 68, Stranger 29.
  - Telemark: Idle 113, Stranger 17, WinterFood 9.
  - In the expansion runs the NoBuilders alert stayed up for 185–262 days on the maps with a dam.

### 2.2 Same strategy on different maps

**Hands-off makes every map the same.** All four collapse on day 347.9: four births on day 47, four Cold deaths on day 325.6 and four more on day 347.9. Food stays above 13.8, and the starting 100 sticks and 30 clay are never touched. The automatic staffing gathers food but never builds a home. The map makes no difference, because the colony dies of a rule (no shelter, so cold kills) long before terrain or resources matter.

**Attentive autoplay** survives on every map. It reveals the maps' economies:

- **Voyageurs grows fastest** (warehouse on day 31) because it never builds a dam. All the sticks go into buildings. The metrics explain this: no dam site within 120 cells, and the designed dam 303 cells away and unexplored.
- **Algonquin** builds its 460-stick dam before most of the town (warehouse on day 65, larder 147). It still ends with the most food (125 days), thanks to 392 shoots and 197 trees within 40 cells of camp.
- **Telemark** takes longer (warehouse on day 171). With 128 shoots within 40 cells, foraging takes more of the workforce. It still ends at pop 10 with 108 days of food.
- **Bavarian is the slowest by far** (warehouse on day 238). It is the only map without immigrants (pop 8): strangers came for 29 days but were not taken in. The metrics explain this. Only 63 trees lie within 40 cells, so sticks run low: the stock fell to 18.8 between days 120 and 210. The nearest mud bank is 28 m away, with only 2 within 40 cells.

**Expansion (dam on day 0, no reaction to alerts)** shows the cost of the designed dam. On the three maps where the dam could be drawn, the 100 starting sticks went straight into it (the dam costs 224–460 sticks). Then a single builder and logger worked for months, and the first shelter was finished on day 148 (Telemark), 203 (Algonquin) and 255 (Bavarian). The more the dam costs and the fewer trees near camp, the longer the wait. Algonquin and Bavarian lost their immigrants (pop 8), which matches the 203 and 255 homeless days. Voyageurs was unaffected only because its dam was refused.

**Planned (shelter first, dam on day 30, one more builder and logger, canal on day 60)** gave the best result on every map: pop 10 everywhere, 112–127 days of food, and the store full of sticks. Moving the dam 30 days later cost nothing. The ponds ended the same size as when the dam was drawn on day 0, because the one-row height cap, not time, limits them. The canals were dug on all four maps (19–26 cells). On Algonquin and Bavarian they filled (25 of 26 cells wet, 0.20–0.26 m deep). On Voyageurs and Telemark they stayed almost dry (2 and 1 wet cells). None of them measurably changed the outcome, as the canal search predicted (no route reaches 20 trees). Bavarian benefits most from planning: its warehouse was done by day 21 instead of 238, and population rose from 8 to 10.

### 2.3 Collapses

The only collapses were the four hands-off runs. Every colony with a player survived the year with zero deaths. Over the first year, a map's difficulty shows up as how long the family waits for housing and how many immigrants it can take in, never as risk of death.

---

## 3. Realism

The images are of three kinds:

- **In-game views**, rendered by the game itself in a hidden window (see *Method and provenance*): the camp at the start, an oblique view and a wide view of the whole map, all at day 0.
- **In-game views at day 360** from the planned run. Day 360 is mid-winter, so the water is frozen.
- **Top-down analysis maps** drawn from the sim grids. In these, yellow-green dots are land shoots, pink dots are water lilies, brown dots are mud banks, grey dots are pebble beds, dark green dots are broadleaf trees and darker blue-green dots are conifers. The yellow ring marks the camp, white outlines mark the designed dam, a brown line is a built dam, orange is a canal and red squares are buildings. The scale bar is 50 m.

### Algonquin

![Algonquin, whole map from above](map_report_images/algonquin_start_topdown.jpg)
![Algonquin wide view](map_report_images/algonquin_wide.jpg)
![Algonquin camp at start](map_report_images/algonquin_camp.jpg)
![Algonquin oblique view](map_report_images/algonquin_oblique.jpg)
![Algonquin camp and dam after a year, from above](map_report_images/algonquin_year_topdown.jpg)
![Algonquin camp after a year, in game](map_report_images/algonquin_year_game.jpg)

- **Rivers and lakes:** the most convincing lake country. A creek winds from the north into a chain of ponds and a real lake in the south-west corner. The flooded trees along the lake shore read as a natural beaver pond.
- **Terrain:** low rolling Canadian Shield relief (23 m) with knobs and hollows, clear in the hillshade.
- **Forest:** a dense mixed forest of maple, pine and birch fills the frame.
- **What breaks the illusion:** the frozen winter pond around the camp looks good, but the camp is almost an island at year end. In the wide view the 23 m of relief cannot be seen.

### Voyageurs

![Voyageurs, whole map from above](map_report_images/voyageurs_start_topdown.jpg)
![Voyageurs wide view](map_report_images/voyageurs_wide.jpg)
![Voyageurs camp at start](map_report_images/voyageurs_camp.jpg)
![Voyageurs oblique view](map_report_images/voyageurs_oblique.jpg)
![Voyageurs camp after a year, from above](map_report_images/voyageurs_year_topdown.jpg)
![Voyageurs camp after a year, in game](map_report_images/voyageurs_year_game.jpg)

- **Rivers and wetland:** the most beaver-like landscape. A braided marsh of channels, oxbows and open water sits between parallel bedrock ridges, which show clearly in the hillshade. Aspen and pine stand on the ridges.
- **What breaks the illusion:** in the wide view, part of the marsh turns into speckled single-cell puddles. That looks like noise rather than a fen. The big lake at the bottom edge is cut off by the map border.

### Bavarian Forest

![Bavarian Forest, whole map from above](map_report_images/bavarian_start_topdown.jpg)
![Bavarian Forest wide view](map_report_images/bavarian_wide.jpg)
![Bavarian Forest camp at start](map_report_images/bavarian_camp.jpg)
![Bavarian Forest oblique view](map_report_images/bavarian_oblique.jpg)
![Bavarian Forest camp and dam after a year, from above](map_report_images/bavarian_year_topdown.jpg)
![Bavarian Forest camp after a year, in game](map_report_images/bavarian_year_game.jpg)

- **What works:** the spruce and beech forest on the slopes is believable, and the mountain streams look right.
- **What breaks the illusion:** the lidar keeps human works the game does not explain.
  - Forest roads and a hairpin road show as raised ridges in the hillshade.
  - A long straight embankment runs down the east side.
  - Rectangular building pads sit in the north-east.
  - The map's only lake is a triangle dammed by that straight embankment.
  - The camp is in a large cleared meadow, which is real land use but empty-looking in game.
- The cuttings and pads are invisible in the 3D view, so the ground looks smooth while the water and paths follow invisible straight lines.

### Telemark

![Telemark, whole map from above](map_report_images/telemark_start_topdown.jpg)
![Telemark wide view](map_report_images/telemark_wide.jpg)
![Telemark camp at start](map_report_images/telemark_camp.jpg)
![Telemark oblique view](map_report_images/telemark_oblique.jpg)
![Telemark camp and dam after a year, from above](map_report_images/telemark_year_topdown.jpg)
![Telemark camp after a year, in game](map_report_images/telemark_year_game.jpg)
![Telemark dam and pond after a year](map_report_images/telemark_year_dam.jpg)

- **What works:** two narrow streams meet below a steep, ice-scoured hillside. The forest is mostly pine and spruce with birch, which looks right for southern Norway. The oblique view is the most convincing forest scene of the four.
- **The dam:** the long narrow pond behind it after a year reads exactly like a beaver pond in a valley floor.
- **What breaks the illusion:**
  - Thin straight lines in the hillshade, probably ditches or tracks.
  - The 31 m of relief barely shows in the wide 3D view.
  - Mud-bank clusters show up as dark brown blobs along the streams (visible in the wide view).

### Across all maps

- The 3D view flattens relief. At 1 m per cell, 15–39 m of relief over 512 m is gentle, and the wide view shows almost none of it.
- Tree stands look equally dense everywhere (about 490 trees per hectare). Bavarian's cleared meadow is the only place where land cover visibly changes the forest.
- Water looks good in every view. In winter the frozen ponds show the colony's footprint well.

---

## 4. Strengths, weaknesses and fixes

### Telemark (rank 1)

**Strengths**
- 17 dam sites, the most on any map.
- The cheapest designed dam: 14 cells and 224 sticks, filling in 27 days. It more than doubles the open water.
- Food is tight (560 food within 30 cells, 67% conifers), which makes foraging and forestry real choices.
- The most convincing scenery.

**Weaknesses**
- The designed dam is 1.2 m tall but stops at 93% built under the 1.0 m one-row cap. The pond is 1,294 m², not the 2,365 m² previewed.
- The fewest shelter spots: 365 near camp, 951 map-wide.
- The "HighAndDry" alert was up for 28 days in the expansion and planned runs.

**Fixes**
- Plan the start dam within the one-row cap (≤ 1.0 m), or make the preview show the pond reached at 1.0 m.
- Add a few water-lily spots in the new pond so building the dam pays back in food.

### Algonquin (rank 2)

**Strengths**
- The richest camp: 197 trees, 392 shoots, 42 mud banks and 49 pebble beds within 40 cells.
- The most water and the most water lilies (466).
- The best results: pop 10 and up to 127 days of food.
- The best-looking lake and pond scenery.

**Weaknesses**
- The designed dam is the most expensive (460 sticks), the slowest to fill (185 days), and the most cut down by the height cap (87% built, 56% of the previewed pond).
- Drawing it on day 0 leaves the family homeless until day 203.
- Water is 90% still, so there are few fast-water decisions.

**Fixes**
- Offer a cheaper first dam. The same crossing at 0.8 m height (19 cells, 39 cells from camp) still makes a 1,649 m² pond, which is more than the 1.5 m design delivers once capped. There are 3 sites within 60 cells.
- Warn when a dam drawn before the first shelter will use up every stick.

### Voyageurs (rank 3)

**Strengths**
- The most beaver-like landscape.
- The most shoots (451 within 40 cells) and the most buildable shoreline (11.7% of the map).
- The colony thrives under every strategy that builds homes.

**Weaknesses**
- The designed dam is 303 cells away and unexplored, so it is refused at start.
- No dam site within 120 cells; attentive autoplay retried the unreachable designed dam 35 times and never built one.
- The year passes with 157 Idle days.
- Speckled puddles in the marsh.

**Fixes**
- Make the start planner keep the dam within reach of the camp (`CAMP_DAM_DIST` is 20–60, but the river-spot path does not enforce it). Or move the camp next to the river that has a dam site.
- Smooth out single-cell puddles under a minimum area.

### Bavarian Forest (rank 4)

**Strengths**
- The highest relief and the best spruce and beech mountain forest.
- The busiest colony (Idle on only 68 days).
- The planned strategy shows that good play pays most here: warehouse on day 21 instead of 238, pop 8 to 10.

**Weaknesses**
- The camp has 63 trees within 40 cells, and 12–24 remain at year end.
- The nearest mud bank is 28 m away.
- The designed pond is 326 m² with a 0.7 m rise and fills in about a day, so the dam is barely noticeable.
- Lidar roads, embankments and building pads, including the embankment-held lake.
- No immigrants under autoplay.

**Fixes**
- Move the camp to the forest edge in the south-west, or add riparian trees around the clearing.
- Allow a start dam further from camp. The other sites make 485–584 m² ponds, and 5 of the 6 pass the start-dam fill and visibility test, but only one lies within 120 cells of camp.
- Flatten road embankments and pads in the height data, or explain them as old logging roads.

### Changes for every map

1. **Give the hands-off colony a way through its first winter.** Automatic staffing could place a first shelter. As things stand, every map's hands-off outcome is set by the rules, not the map.
2. **Keep start dams to one row (≤ 1.0 m), or preview the pond as built.** Two of the three buildable designed dams overpromise by about 45%.
3. **Make site scoring include the dam.** Score distance to the planned dam and the trees within reach, so the four "Excellent" camps stop hiding a 4× spread in nearby trees and a dam 303 cells away.
4. **Let land cover change tree density, not just where trees stand.** Density is fixed at 470–506 per hectare and river clay at 8 deposits on every map. Varying them would give each map its own economy.
5. **Add goals for the second half of the year.** All four colonies run out of things to do after about day 230. Candidates: a second dam, canal routes that unlock far groves, or wetland growth that rewards a larger pond.

---

## Method and provenance

- **Map metrics** came from a throwaway Rust project outside the repository. It used a snapshot of `crates/sim` taken at 00:26 on 2026-11-22, built with its own target directory. Two private helpers were made public in the snapshot only, so they could be called from outside: `starts::dam_candidates` and `starts::river_spots`. The project measured the world, water and resources of `new_game_on_map(id)`, ran a 360-day water-only year for the seasonal figures, and tested candidate canals with `canals::plan` / `canals::check`.
- **Strategy runs.** The same snapshot ran all 16 scripts headless through `sim::script::run_to_state`. Two cross-checks against the game itself:
  - Seven of the eight hands-off and attentive runs had already finished on a copy of `target/release/beaver_colony.exe` (built 00:19) before those game windows were stopped. They match: the hands-off results are identical, the attentive runs give the same populations, and building completion days agree within 0–8 days.
  - The four planned runs were repeated in the game exe built from the current tree. They produced exactly the same SUMMARY lines.
- **Images.** The in-game views come from that game exe, using the same settings as `tools/game_shot.sh` (1600×900, `BC_NO_FOG`, `BC_SETTLE`, `BC_CAM_AT`), and `--script ... --report-dir` for the day-360 views. They were rendered in a hidden window: see the next section. The top-down maps were drawn in Python from grid dumps of the sim state.

### Change made to the game: automated runs no longer show a window

During this work, automated game runs opened visible windows on the user's screen. That has been fixed in `crates/game/src/main.rs`; no other file changed.

**The change.** `primary_window()` now takes an `automated` flag. The flag is set when the game runs with `--screenshot` (which includes the gallery tool), `--script`, `--report-dir` or `BC_PERF`. For automated runs the window is created:
- hidden (`visible: false`);
- without taking focus (`focused: false`);
- left out of the taskbar (`skip_taskbar: true`);
- windowed at `BEAVER_WINDOW`, or 1600×900 by default.

Bevy keeps updating and rendering an invisible window, and `Screenshot::primary_window()` still captures it. Interactive play (no automation flags) uses the same code path as before.

**Checks.**
- A watcher polled the game's windows and the foreground window every 100 ms throughout each test run. In every run (a `game_shot`-style screenshot, the 12 report screenshots, a `--script --report-dir` run, and a `BC_PERF` benchmark) the game had no visible window and the foreground window never changed. The only window flagged was winit's own helper window (class "Winit Thread Event Target"). Winit creates it in every app, interactive play included. It is a 16×16 layered, transparent, non-activating tool window that never appears on screen.
- Screenshots come out at 1600×900 with normal content.
- `cargo check --workspace --all-targets` passes, with one warning in `crates/sim/src/perf_bench.rs` (another session's new file, not this change).
- `cargo test -p sim --lib` passes: 90 tests, 0 failures.
- All builds used a separate target directory.
- Nothing was committed.
