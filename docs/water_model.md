# Water model

Maps are 512 m windows of 1 m cells from 1 m lidar, heights in metres.

## Physics

Water is simulated with the two-dimensional shallow-water (Saint-Venant) equations on the 1 m
grid: conservation of mass and of momentum in x and y, with gravity, the pressure of the water
column, advection of momentum and Manning bed friction.

* **Scheme.** First-order finite volumes. Fluxes across each cell face come from the HLL
  approximate Riemann solver. The bed under each face is the higher of the two cells' beds, and
  the water depth on each side is reconstructed against it (hydrostatic reconstruction, Audusse et
  al. 2004). Still water over any bed therefore stays exactly still, a step in the bed (a dam
  face, a bank) acts as a weir, and a cell's depth never goes negative.
* **Steps in the bed.** When water rises onto a higher face, the reconstruction keeps the flow
  per metre (velocity rises as the water thins) rather than the deep-water velocity, capped at
  the shallow-water wave speed. Without this a first-order scheme loses about a third of the head
  over a sharp-crested sill.
* **Friction.** Manning, n = 0.04 (natural stream), applied semi-implicitly so thin films on steep
  ground stay stable.
* **Time step.** Explicit, CFL 0.45 on the fastest wave (|u| + sqrt(g h)), at most 2 s. Typically
  0.1 to 0.4 s of physical time per step.
* **Cost.** Only 16 x 16 m tiles holding water (and their neighbours) are computed, in parallel.
  On Telemark the solver runs about 1000 s of physics per real second with streams only, less
  with deep ponds. The game at 1x needs 288 s per second; faster game speeds run as fast as the
  physics allows.

Nothing about where water goes, how wide or deep it is, or how fast it moves is prescribed: it
follows from the terrain, the dams and canals, and the inflows.

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
* **Rain, evaporation and seepage** on wet cells, by season.
* **Books:** every source and loss is counted; total stored water always equals what came in
  minus what left.

## Start state

A new map starts with fed hollows and lakes full, then runs 2.5 hours of physics so streams
reach their steady flow before play begins.

## Known limits

* First-order accuracy: sharp fronts (a breach flood, a hydraulic jump) are smeared over a few
  metres.
* Cells are 1 m, so a stream narrower than a cell is represented as a thin sheet across the cell.
* Dams are watertight (real beaver dams leak).

## Tests

`cargo test -p sim water::` checks the solver against physics, not against itself:

* still water in a bowl stays level and still;
* a closed box keeps its water while it sloshes;
* a small wave on still water 1 m deep reaches a gauge 120 m away at sqrt(g h);
* steady flow down a uniform slope settles at Manning's normal depth (q n / sqrt(S))^(3/5);
* a closed pond's volume rises by exactly inflow x time;
* the head over a broad-crested sill matches critical flow, 1.5 (q^2 / g)^(1/3), within the
  discharge coefficient of a real square-edged crest.

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
