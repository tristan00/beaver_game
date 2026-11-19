# Water model

Maps are 512 m windows of 1 m cells from 1 m lidar, heights in metres.

## Physics

Water is simulated with the local-inertial form of the two-dimensional shallow-water equations
(Bates et al. 2010, the scheme of LISFLOOD-FP and similar flood models) on the 1 m grid: mass is
conserved to floating-point tolerance, and each face's discharge is driven by the water-surface slope against Manning
bed friction. Only the advection of momentum is left out; at the speeds of beaver streams it
changes little, and leaving it out makes a cell update a few multiply-adds.

* **Faces.** Discharge per metre q on each face: q' = (q - g h_f dt dη/dx) / (1 + g dt n² |q| /
  h_f^(7/3)), with h_f the depth over the higher of the two beds (so still water over any bed
  stays still). A little of the neighbouring faces' discharge is mixed in (de Almeida et al. 2012,
  θ = 0.8), which damps the sloshing the plain scheme leaves undamped.
* **Steps in the bed.** Where the water falls freely off a step (a dam crest, a bank), the face is
  a weir: its discharge is capped at critical flow, 1.705 H^1.5 for a head H over the crest.
  Elsewhere the Froude number is capped at 1.
* **Mass.** No cell may send out more water than it holds: outflows are scaled down when they
  would, so depths never go negative and every cubic metre is accounted for.
* **Friction.** Manning, n = 0.04 (natural stream), semi-implicit.
* **Time step.** Explicit, 0.7 dx / sqrt(g h) for the deepest water the solver steps, at most 10 s.
  About 0.25 to 0.45 s on the shipped maps.
* **Still water.** Flat, still water (a pond behind a dam, a wetland, a deep hole) is found every
  5 minutes of game time and held as a *pool*: one level for the whole body, set from its volume
  by its bed shape. Only the faces on its rim are computed, it gains what crosses them, and its
  depth no longer limits the time step. When the water starts to move or tilt (a breach, a new
  inflow) the cells leave the pool and are stepped again.
* **Active area.** Only rows of cells holding water, and their neighbours, are stepped, in row
  bands on a small worker gang (up to 12 threads), with SIMD kernels (AVX2 when the CPU has it,
  bit-identical to the SSE2 path).

Nothing about where water goes, how wide or deep it is, or how fast it moves is prescribed: it
follows from the terrain, the dams and canals, and the inflows.

## Cost

The calendar targets one game day per **30 real seconds** at normal speed (ten times the
previous pace). Keys 2 and 3 request 6x and 24x: **5 seconds** and **1.25 seconds** per day.
The speed-button tooltips show these targets. Large floods can limit the achieved rate;
the performance log reports actual real seconds per day as well as the speed multiplier.
This changes the shared game clock for both colony play and sandbox, without changing
hydraulic time steps, water volumes, or per-game-day production and climate rates.

With the new pace, the same Telemark clear-spring sandbox measured **29.99–30.01 real
seconds per day at 75–78 fps** on the machine below. Log: `tmp/river-polish/pace-normal.err`.

`SimState::step_timings` and `Water::timings` record where time goes. Set `BC_PERF=5` to log
every 5 seconds the frame rate, the game speed reached, the sim's milliseconds per second by
system and the water solver's breakdown; `BC_SPEED=1|2|3` and `BC_PERF_EXIT=S` make it a
benchmark. A normal-speed sandbox frame advances water in ten-second slices; faster settings
use twenty/thirty-second slices. Pool detection, rainfall and catchment forcing updates are
batched every five simulation minutes. Terrain height range is computed once per update,
instead of scanning the entire map for each rebuilt chunk.

Earlier hydraulic measurements, before the calendar acceleration, on Core Ultra 9 285K /
RTX 5090, Windows Vulkan, 1440x900, forest visible, Telemark camera
`170,424,65,0.8,1.0`. In this historical table 1x means the former five-minute day
(288 simulation seconds/second), not the current 30-second day:

| scenario | simulation speed reached | frame rate |
|---|---|---|
| Clear spring, 1x requested | 1x | 110–114 fps |
| Spring storm, 1x requested | 1x | 116–117 fps |
| Clear spring, 24x requested | 21–22x | 46–51 fps |
| Inherited engine, 24x requested, same map/camera | 18.5–19.7x | 44–47 fps |

These are short local runs, not cross-hardware guarantees. At the former normal speed the clear-stream
water update costs about 43–44 ms of wall time per second of play. Finite lakes and heavy
overland runoff require more work than a narrow stream. Detailed logs and scenario results
are in the workspace's `tmp/river-polish` directory.

## Terrain

The bed is the lidar terrain as measured (no carved channels, no filled hollows), plus dam
heights. Two map corrections are applied when a map is built:

* **Culverts.** Bare-earth lidar keeps road and rail embankments but not the culverts under them.
  A hollow on a stream's path is cut through when ground lower than its floor lies at most 25 m
  beyond its rim.
* **Open-water lakes.** Land cover class 80 marks lakes the lidar flattened. They start at that
  surface level, on a bed shelving 5% from shore to at most 3 m (assumed bathymetry). Interior
  lakes hold finite water: rain, dams, canals and drought can change their levels. Only lake
  cells on the map boundary retain a fixed level, representing the reservoir outside the map.

## Water in and out

* **Inflows** at the map edge, from each stream's catchment beyond the map (area x 0.01 m3/s per
  km2), and **lateral inflow** along each stream from the land inside the map that drains to it.
  Seasonal flow varies smoothly between the middle of each season: spring 3x the mean,
  summer 0.3x, autumn 0.8x and winter 0.1x. Source edits invalidate the cached rates even if
  the number of sources stays the same. The total source rate is cached outside physics substeps.
* **Edges** are open: water leaves with the slope of the ground at the edge. The edge cells
  beside each inflow are closed so a stream cannot leave where it came in.
* **Weather.** Seasonal mode has a repeatable rainy day in each six-day cycle with smooth
  onset/end and a three-hour delay in upstream catchment response. Sandbox overrides select
  clear, rain (12 mm/day), storm (60 mm/day) or drought, before seasonal scaling. Storms raise
  upstream flow; drought reduces it to 12% of the season's normal flow.
* **Rain reaches dry land as well as water.** Turf infiltrates four times faster than mineral
  beds; rain exceeding infiltration and evaporation builds surface runoff, using the same
  conservative solver as rivers. Rain, losses and riverbed exposure are accumulated in
  five-minute physics batches, independent of render-frame duration.
* **Ice** grows in winter and melts outside winter. Rendering uses the simulated thickness
  at each vertex; a change of season does not instantly paint every river white.
* **Books:** every source and loss is counted; total stored water always equals what came in
  minus what left.

## Start state

A new map starts with fed hollows and lakes full, then runs 2.5 hours of physics so streams
reach their steady flow before play begins.

The wet footprint after spin-up seeds persistent mineral soil. Flooding exposes new bed over
half a day; dry beds recover over 180 days. Excavated canals remain bare. This also runs in
sandbox, without depending on the colony's ecology update.

## Known limits

* No momentum advection: water does not carry its speed round a bend or form hydraulic jumps; a
  breach flood front is smeared over a few metres.
* Pools are flat to within 1.5 cm; the slight tilt of water flowing slowly through a pond is lost.
* Cells are 1 m, so a stream narrower than a cell is represented as a thin sheet across the cell.
* Dams are watertight (real beaver dams leak).
* Weather is a deterministic gameplay cycle, not a meteorological forecast. Snow accumulation,
  sediment transport and erosion of the terrain itself are not simulated.

## Drawing

The water surface is a mesh over the visibly wet cells (3 mm or more). Each vertex takes the
surface level of the water around it, so the waterline falls where that level meets the ground
and follows the banks. `water.wesl` colours it by depth (the bed shows through shallow water),
carries ripples and streaks along the flow velocity, lays lace along the waterline and white
water where the Froude number is high (rapids, the face of a dam), and freezes over in winter.
Water and terrain keep 1 m resolution beside wet or excavated cells at every camera distance;
coarse bank geometry cannot erase narrow streams. Water polygons are clipped against the real
bed and dam crest. Flow, ice and dam changes invalidate water meshes even at constant depth.
Terrain updates use a rotating cursor so moving upstream water cannot starve downstream chunks.
Excavated canal chunks refine both terrain and water to quarter-metre render vertices, with a
flat floor around each excavated cell. This keeps a narrow, deep trench from appearing as a
hairline of water at the bottom of a V. The reconstruction is visual; physics stays on the 1 m
grid and no extra ground is excavated. Metre-scale terrain colours are reused during refinement.

Shallow water shows mineral bed, with a muddy bank fringe and persistent exposed silt after
drainage. Ground plants are suppressed in water and on bare riverbed. Trees no longer carry
opaque horizontal ground discs, which used to float across sloping riverbanks and hide water;
the scene shadow map supplies their shadows. Their canopies still naturally occlude the view.
The stylized teal surface carries current streaks and rapids; screen derivatives compute ripple
normals using one third of the previous noise evaluations.

## Tests

`cargo test -p sim water::` checks the solver against physics, not against itself:

* still water in a bowl stays level and still;
* a closed box keeps its water while it sloshes;
* a small wave on still water 1 m deep reaches a gauge 120 m away at sqrt(g h);
* steady flow down a uniform slope settles at Manning's normal depth (q n / sqrt(S))^(3/5);
* a closed pond's volume rises by exactly inflow x time;
* the head over a broad-crested sill matches critical flow, 1.5 (q^2 / g)^(1/3), within 15%.
* rainfall on dry ground balances infiltration and is independent of outer frame slices;
* source edits, diagonal canal flow, reservoir breaches, and excavation into pooled water
  conserve the expected volume;
* interior lakes hold finite water and exposed beds persist after drainage.

`cargo test --release -p game water_render::tests` checks shallow one-cell streams at distant
LOD, water triangles above the bed, and flow/ice mesh invalidation. Sandbox scenarios and images
are kept outside the repository under the workspace's `tmp/river-polish` directory.

## Sandbox

`beaver_colony --sandbox --map ID` opens a map with no colony: only water runs, the dam and canal
tools build at once, and Delete breaches the dam under the cursor (or the selected dam).
Connected scripted dams longer than the normal placement limit are removed completely.
F6 cycles weather, F7 advances
to the next season, Space pauses and 1/2/3 select speed. The status panel shows rainfall and
catchment flow. Sandbox starts directly on a river and uses short simulation slices to
keep storm and breach updates responsive. `--sandbox-script FILE` runs a
script first and prints its report:

    map telemark                  # first line; 'fixture' is the small test valley
    dam 1.5 X,Y X,Y ...           # dam this high along the line through the points
    dam-to 23.0 X,Y X,Y           # dam whose crest stands at this level
    canal 2.0 X,Y X,Y             # dig this deep
    canal-to 7.4 X,Y X,Y          # excavate to a common bed elevation, never raise ground
    breach X,Y X,Y                # breach every dam cell on the line
    weather storm                # seasonal | clear | rain | storm | drought
    season summer                # advance to the midpoint of the named season
    run DAYS
    probe X,Y [label]             # ground, dam, depth, surface, flow, speed
    check depth X,Y > 0.3         # also surface ground dam width discharge speed bed ice
    check rain > 20              # mm/day; also inflow (multiplier), volume, balance, day
    dump FILE                     # grids for tools/sandbox_overview.py and tools/sandbox_cells.py

`cargo run --release -p sim --example sandbox -- FILE` runs a script headless.
`tools/sandbox_shot.sh FILE|MAP X,Y[,DIST,YAW,PITCH] OUT.png [notrees]` photographs the result in
the game.

On Windows, `./tools/sandbox.ps1 -Map telemark` builds and launches the current checkout.
Use `-Script PATH`, `-Camera 'X,Y,DIST,YAW,PITCH'`, or `-NoTrees` for repeatable visual inspection.

Canal lines add face-connected corner cells, so diagonal and bent canals carry water instead of
forming isolated pits. Crossing cells within one excavation command are dug only once. Dams,
breaches and excavation dissolve optimized pools against their old beds before changing
geometry, preserving stored volume.
