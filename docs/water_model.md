# Water model

Maps are 512 m windows of 1 m cells from 1 m lidar, heights in metres.

## Physics

Water is simulated with the local-inertial form of the two-dimensional shallow-water equations
(Bates et al. 2010, the scheme of LISFLOOD-FP and similar flood models) on the 1 m grid: mass is
conserved exactly, and each face's discharge is driven by the water-surface slope against Manning
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

`SimState::step_timings` and `Water::timings` record where time goes. Set `BC_PERF=5` to log
every 5 seconds the frame rate, the game speed reached, the sim's milliseconds per second by
system and the water solver's breakdown; `BC_SPEED=1|2|3` and `BC_PERF_EXIT=S` make it a
benchmark. Measured on a 24-core desktop (1x is 288 s of game time per second):

| map | water alone reaches | game at 6x | game at 24x asked |
|---|---|---|---|
| Telemark (streams) | x30 | x6, 108 fps | x18, 45 fps |
| Algonquin (pond chain and lake) | x9 | x5.6, 51 fps | x7, 35 fps |

At 1x water costs about 40 ms of CPU per second of play.

## Terrain

The bed is the lidar terrain as measured (no carved channels, no filled hollows), plus dam
heights. Two map corrections are applied when a map is built:

* **Culverts.** Bare-earth lidar keeps road and rail embankments but not the culverts under them.
  A hollow on a stream's path is cut through when ground lower than its floor lies at most 25 m
  beyond its rim.
* **Open-water lakes.** Land cover class 80 (open water) marks lakes the lidar flattened. Each
  is held at its flattened surface level, on a bed shelving 5% from the shore to at most 3 m (the
  lidar has no bathymetry, so this depth is an assumption). Lake cells keep their level; water
  reaching them leaves the books there.

## Water in and out

* **Inflows** at the map edge, from each stream's catchment beyond the map (area x 0.01 m3/s per
  km2), and **lateral inflow** along each stream from the land inside the map that drains to it.
  Spring runs 3x the mean, late summer 0.3x.
* **Edges** are open: water leaves with the slope of the ground at the edge. The edge cells
  beside each inflow are closed so a stream cannot leave where it came in.
* **Rain** on open water, **evaporation and seepage** from wet ground, by season.
* **Books:** every source and loss is counted; total stored water always equals what came in
  minus what left.

## Start state

A new map starts with fed hollows and lakes full, then runs 2.5 hours of physics so streams
reach their steady flow before play begins.

## Known limits

* No momentum advection: water does not carry its speed round a bend or form hydraulic jumps; a
  breach flood front is smeared over a few metres.
* Pools are flat to within 1.5 cm; the slight tilt of water flowing slowly through a pond is lost.
* Cells are 1 m, so a stream narrower than a cell is represented as a thin sheet across the cell.
* Dams are watertight (real beaver dams leak).

## Drawing

The water surface is a mesh over the visibly wet cells (3 cm or more). Each vertex takes the
surface level of the water around it, so the waterline falls where that level meets the ground
and follows the banks. `water.wesl` colours it by depth (the bed shows through shallow water),
carries ripples and streaks along the flow velocity, lays lace along the waterline and white
water where the Froude number is high (rapids, the face of a dam), and freezes over in winter.
Trees do not start within 6 m of streams and lakes, so the water shows under the canopy.

## Tests

`cargo test -p sim water::` checks the solver against physics, not against itself:

* still water in a bowl stays level and still;
* a closed box keeps its water while it sloshes;
* a small wave on still water 1 m deep reaches a gauge 120 m away at sqrt(g h);
* steady flow down a uniform slope settles at Manning's normal depth (q n / sqrt(S))^(3/5);
* a closed pond's volume rises by exactly inflow x time;
* the head over a broad-crested sill matches critical flow, 1.5 (q^2 / g)^(1/3), within 15%.

## Sandbox

`beaver_colony --sandbox --map ID` opens a map with no colony: only water runs, the dam and canal
tools build at once, and Delete breaches the dam under the cursor. `--sandbox-script FILE` runs a
script first and prints its report:

    map telemark                  # first line; 'fixture' is the small test valley
    dam 1.5 X,Y X,Y ...           # dam this high along the line through the points
    dam-to 23.0 X,Y X,Y           # dam whose crest stands at this level
    canal 2.0 X,Y X,Y             # dig this deep
    breach X,Y X,Y                # breach every dam cell on the line
    run DAYS
    probe X,Y [label]             # ground, dam, depth, surface, flow, speed
    check depth X,Y > 0.3         # also surface ground dam width discharge; 'balance' and 'day'
    dump FILE                     # grids for tools/sandbox_overview.py and tools/sandbox_cells.py

`cargo run --release -p sim --example sandbox -- FILE` runs a script headless.
`tools/sandbox_shot.sh FILE|MAP X,Y[,DIST,YAW,PITCH] OUT.png [notrees]` photographs the result in
the game.
