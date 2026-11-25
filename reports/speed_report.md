# Speed report: where a game day's time goes

Measured 22 November 2026 in the full game (window, rendering, session log), at the top speed.

## Result

The top speed asks for 6 game days a second, which is 5 seconds a month. It now gets that on
every case measured; before, the simulation thread could not keep up.

| Full game, top speed | Before | After |
| --- | --- | --- |
| Your autosave (Algonquin, day 53, 8 beavers, 1 dam being built) | x87 to x132 of x180: **6.9 to 10.4 s a month** | x168 to x180: **5.0 to 5.4 s a month** |
| New Algonquin colony | x156 to x170: 5.3 to 5.8 s a month | x180: **5.0 s a month** |
| New Telemark colony | not measured | x180: **5.0 s a month** |

The slower end of the autosave range (5.4 s) is while the dam is still being built: each load
packed onto it redoes the map's drainage (see "One-off costs").

How busy the simulation thread is at top speed, steps plus snapshots (100% means it cannot go
faster):

| | Before | After |
| --- | --- | --- |
| Autosave | 100% (saturated) | about 79% |
| New Algonquin colony | 100% (saturated) | about 71% |
| New Telemark colony | not measured | about 49% |

Simulation alone, with no window (how fast a month could go if the speed asked for it):

| Seconds a month, headless | Before | After |
| --- | --- | --- |
| Algonquin, new colony | 5.4 | 3.4 |
| Telemark, new colony | 3.4 | 2.5 |
| Voyageurs, new colony | 2.8 | 2.3 |
| Bavarian, new colony | 3.4 | 2.3 |
| Autosave, first month | 8.0 | 4.1 |

A month is 30 days and a day is 96 steps of fifteen minutes, so 5 seconds a month leaves the
simulation thread 167 ms for each game day.

The tables in this report up to "Change lists" were measured before that last piece of work.
Reporting what changed costs the simulation thread about 4% more (146 ms a day on the autosave
instead of 140); top speed still holds x180 on every case.

## Where a day goes now

Full game, your autosave, 283 game days at top speed. Milliseconds of the simulation thread
for each game day; 141 ms in all (it was 279 ms). "Before" is the same save in the full game
before any change.

| Part of a day | ms a day | Before | Calls a day | Each |
| --- | ---: | ---: | ---: | ---: |
| **Water** | **81.5** | **145.3** | 96 | 849 us |
| - Publishing pond depths (36,000 pond cells every step) | 15.0 | 17.3 | 96 | 157 us |
| - Routing flow down the rivers (1,500 cells a step) | 13.9 | 48.8 | 96 | 145 us |
| - Rain, evaporation and seepage on ponds | 12.4 | 15.3 | 96 | 129 us |
| - Copying depth, flow and speed to the grids the game reads | 11.6 | 16.1 | 96 | 120 us |
| - Finding each row's runs of wet cells | 11.1 | 11.3 | 96 | 116 us |
| - Rebuilding drainage after a dam cell is raised (not made faster: fewer cells were raised in this run) | 6.9 | 11.0 | 0.1 | 68 ms |
| - Clearing last step's depths, rim flows, bookkeeping | 7.3 | 8.6 | 96 | 76 us |
| - Ice | 1.4 | 0.0 (no winter in that run) | 96 | 15 us |
| - Streams and small pools: depths and flows | 1.4 | 1.4 | 96 | 14 us |
| - Rain and seepage on streams, riverbed turning to silt | 0.4 | 15.4 | 96 | 4 us |
| - Dam wear, loading the bed | 0.1 | 0.1 | 96 | 1 us |
| **Snapshot for the interface** | **18.1** | **42.9** | 4.2 | 4.3 ms |
| **Beavers** (see next table) | **8.1** | **9.4** | 96 | 84 us |
| **Dams: measuring each pond** | **7.8** | **35.5** | 1.2 | 6.6 ms |
| **Session log** (journal) | **6.1** | **6.7** | 288 | 21 us |
| **Trees** (daily sweep) | **4.5** | **23.1** | 1 | 4.5 ms |
| - Forest cover for the ecology | 1.2 | 1.6 | 1 | 1.2 ms |
| - Ground kept moist by water | 1.1 | 4.3 | 1 | 1.1 ms |
| - Cells the dam will flood (nothing sown there) | 1.5 | 16.6 | 1 | 1.5 ms |
| - Growing, dying and seeding each tree | 0.5 | 0.6 | 1 | 0.5 ms |
| **Food plants and mud beds** (17,000 sources each step) | **4.4** | **4.5** | 96 | 45 us |
| **Ecology** | **3.1** | **2.9** | 96 | 33 us |
| **Colony events** | **3.1** | **3.2** | 96 | 32 us |
| **Wetland** (daily sweep) | **3.1** | **3.9** | 1 | 3.1 ms |
| **Colony** (needs, health, births, jobs) | **1.1** | **1.1** | 96 | 12 us |
| Fog, buildings, upkeep, canals, wolves, mood, ledger, stores | 0.2 | 0.2 | 96 | 2 us |

## Per action: what each beaver's step costs

A beaver is stepped 96 times a day; the cost depends on what it was doing when the step
began. Eight beavers, same run.

| Action | ms a day | Beaver-steps a day | Each |
| --- | ---: | ---: | ---: |
| Collecting (walking to a source, gathering, choosing the next) | 4.62 | 240 | 19 us |
| Idle, choosing work | 0.14 | 447 | 0.3 us |
| Resting | 0.03 | 43 | 0.7 us |
| Working on the dam | 0.02 | 16 | 0.9 us |
| Carrying a load to a store | 0.02 | 6 | 2.5 us |
| Fetching material | 0.01 | 2 | 4.7 us |
| Digging canals, gates, tending, planting, building, hauling | 0 | none in this run | |

Around the beavers, each step:

| | ms a day | Calls a day | Each |
| --- | ---: | ---: | ---: |
| Sharing out jobs (organise) | 1.79 | 96 | 19 us |
| Updating where beavers can walk and swim | 1.08 | 96 | 11 us |
| Clearing finished work, new arrivals, finished sites | 0.14 | 96 | 1.4 us |

Inside those steps:

| | Calls a day | Each |
| --- | ---: | ---: |
| Choosing a source to collect from (looks at every source) | 1.0 | 69 us |
| Choosing building work | 20 | 0.7 us |
| Finding a path | 1.4 | 19 us |

## One-off costs (hitches, not averages)

| What | When | Cost | Before |
| --- | --- | ---: | ---: |
| Rebuilding the map's drainage | every time a dam cell is raised | 68 ms | 68 ms |
| Measuring a dam's pond | once a day for each dam | 6.6 ms, about 16 ms on the first day after the ground changes | 30 ms |
| Snapshot for the interface | about 25 times a second | 4.3 ms | 8.3 ms |
| Trees, daily sweep | once a day | 4.5 ms | 21 to 23 ms |
| Wetland, daily sweep | once a day | 3.1 ms | 3.9 ms |

## What was changed

In order of what they saved on the autosave:

1. **Small stranded pools** (about 250 on this map) were each solved with 60 halvings and a
   power function every step. They now use a bracketed secant solve that settles in a few
   tries. Routing went from 49 to 14 ms a day.
2. **Snapshots** copied the whole 65 MB world each time. Grids that have not changed since
   the last copy are now skipped, the water engine's drainage tables are copied only after a
   rebuild, and the forestry's daily tables are shared. A copy went from 8.3 to 4.3 ms.
3. **A dam's pond estimate** worked out the still pond twice and used hash sets for 85,000
   cells. It now works it out once, and a dam nobody is building on reuses the ground's part
   until terrain or a dam changes. 36 to 8 ms a day.
4. **The forestry's flood plan** ran the whole dam estimate to get one list of cells, then
   built an ordered set cell by cell. It now takes just the still pond, builds the set in one
   sweep and keeps it while nothing changes. 17 to 1.5 ms a day.
5. **Riverbed turning to silt or turf** visited all 262,144 cells every step. Only cells still
   changing are visited now. 15 to 0.2 ms a day.
6. **Moist ground and forest cover** used nested scans; both are single sweeps now. 6 to 2.3 ms
   a day.
7. **Pond sweeps** (depths, weather) run along the map's rows instead of in order of bed
   height, and still water skips the square roots when published. About 10 ms a day.
8. The water router sorts the step's known work once instead of pushing it through a heap.

## Are results the same?

- Every change except two was checked to leave the world **identical, bit for bit**: the old
  and new code were run side by side (Algonquin 3 months, Telemark 2 months, the autosave for
  1 month and for 13) and the whole world compared at the end.
- The two exceptions are change 1 and the weather half of change 7. They move water by
  amounts in the last decimal places (a solve to 13 digits instead of 60 halvings; the same
  sums added in another order). The colonies came out the same in the comparison runs: same
  population and buildings at each month's end. For the side-by-side runs these two were put
  into the old code as well; with them in both, the worlds matched bit for bit, so they are
  the only difference.
- All tests succeed: 94 in the simulation, 15 in the game.
- Old saves still load. Your autosave was only read, never written.

## What still costs most, and what would cut it

1. **Ponds cost time for every cell, every step** (about 50 ms a day here, for 36,000 cells of
   lake). Depths, weather and wet-row runs are redone for the whole pond even when its level
   moved a hair. Publishing a pond only when its level has moved by a fraction of a millimetre
   would remove most of this, and matters more as dams make bigger ponds. It would change
   results slightly, so it was left alone.
2. **Raising a dam cell rebuilds the whole map's drainage** (68 ms). While a dam is being
   built this is what holds the game just under top speed.
3. **Each dam costs about 7 ms a day** for its pond measurement, more on the day after any dam
   changes. Ten dams would take 70 ms of the 167 ms a day.
4. **Snapshots** still copy about 30 MB of the water engine's per-cell working arrays.

## The game's side: how it reads the simulation

The simulation hands the game a copy of the whole world about 25 times a second, and every
renderer and panel works out what it needs from that copy. Three faults made this cost far
more than it should, and are fixed:

- **The world looked changed on every frame.** Two systems wrote to it each frame whether or
  not they had anything to write (sending commands, and collecting colony events). Everything
  that redraws "when the world changes" (buildings, dams) therefore redrew 60 times a second.
  They now leave it alone when there is nothing to do.
- **Terrain and water checked every chunk on every frame**, hashing about half a million cells a
  frame each to see whether a chunk needed rebuilding. They were first made to skip the check
  while nothing had changed; that is now replaced by the change lists below, which say which
  chunks to check.
- **The fog was redrawn four times a second for ever**: the simulation raised its "newly
  seen" flag and never lowered it. It is now raised once for each snapshot that saw something
  new.

A new Algonquin colony at top speed went from 56 frames a second (frames up to 60 ms) to a
steady 60 (frames up to 25 ms). Checked from a screenshot and the timed runs; not played by
hand.

## Change lists: the simulation says what changed

Done after everything above. The simulation now reports what changed, and the game redraws
from the reports instead of comparing the world it is handed.

What the simulation reports:

- **Tiles of each grid.** Every grid remembers, for each tile of 16 by 16 cells, when a cell
  that tile reads was last written (`Grid::tile_stamp`). Height, dams, canals and ice are
  stamped on any write. Depth, water speed, riverbed and plant cover are drawn only so finely,
  so their tiles are stamped once a cell has moved a whole step of what is drawn since the tile
  was last stamped: 2 mm of depth, 0.05 m/s, a sixteenth of riverbed, a sixty-fourth of cover.
- **The water's edge.** A new grid, `water.shore`, holds each cell's depth in whole centimetres
  up to three, which is all the ground's colour uses of depth. The ground is redrawn when the
  edge moves, not when a pond rises over ground already under water.
- **Trees, plants and deposits.** A list of those added, removed, or that came to look
  different (`resources.changes()`). `Resource::look()` is the simulation's own account of how
  one looks, and the game now draws from it. The list of sources is private to the simulation,
  so every change to one goes through a method that notes it.

What the game does with them:

| | Before | Now |
| --- | --- | --- |
| Trees, plants and deposits | walked all 17,000 on each snapshot | looks at the few on the list, and at plants where the water's edge moved |
| Ground chunks | hashed every chunk's cells on each snapshot | hashes a chunk only when one of its tiles is stamped |
| Water chunks | the same, and remade every chunk each three game days when the season stepped, though the season does not change the mesh | hashes a chunk only on a stamp; the season no longer remakes any |
| Dams | scanned the whole dam grid on each snapshot | only when a dam changes (no gain while a dam is wearing, which changes it every step) |

### What it bought

Full game, your autosave. "Before" at top speed is this morning's build. "Before" at normal
speed is the old rule switched back on in today's build (the switch is removed again), because
the earlier build was not measured at that speed.

Normal speed:

| | Before | Now |
| --- | ---: | ---: |
| Rebuilding water meshes, ms a second | 44 to 50 | 2.5 to 5 |
| Rebuilding ground meshes, ms a second | 7.3 to 7.5 (15 when the season steps) | 1.6 to 1.8 (10 when the season steps) |
| Ground and water together, each frame | about 1.2 ms | 0.2 ms |

Top speed:

| | Before | Now |
| --- | ---: | ---: |
| Trees, plants and deposits, each frame | 0.39 ms | 0.06 ms |
| Rebuilding water meshes, ms a second | 21 | 47 |
| Rebuilding ground meshes, ms a second | 45 | 45 |
| Frames a second, longest frame | 60, 24 to 26 ms | 60, 24 to 26 ms |
| Simulation thread, ms a game day | 139.7 | 145.7 |
| Speed held | x180 | x180 |

So the gain is at normal speed, where the map stops being remade for changes nobody can see.
At top speed nothing is gained on balance:

- The water is remade more, not less. Before, the sweep spent most of its 2 ms a frame hashing
  chunks and ran out before it had remade those that needed it; now it spends that time
  remaking them, so the water on screen is less far behind. It stays inside the same 2 ms.
- The ground is remade for the season, as before: every chunk each three game days, ten chunks
  a frame.
- The simulation thread does 6 ms a day more: 1.3 comparing cover with what was last told, 1.25
  doing the same for water before each snapshot, 1.1 in publishing depths, 0.6 on plants and
  mud beds, the rest scattered. It is at 87% of its 167 ms instead of 84%.

### Why so much was being remade

- **Meadow regrows by 0.07% of cover a day on every cell of the map.** The game rounded cover
  to sixty-fourths and remade a chunk when any cell's rounding changed. With hundreds of cells
  a chunk, some cell crossed a rounding step every quarter day, so at normal speed the whole
  map's ground was remade every 7.5 seconds. A tile is now remade when cover on some cell has
  moved a whole sixty-fourth since it was last drawn: about every 22 days.
- **A pond's level creeps the same way**, and every chunk of it was remade whenever one cell
  crossed a 2 mm rounding step. Now a chunk is remade when its water has moved 2 mm.
- **Under a new pond, forest cover goes down and up again every day.** The ecology drowns it a
  little each quarter day and the forestry puts it back from the standing trees each morning:
  670 cells on the autosave, a twelfth of full cover each way. Nothing of it shows, since
  ground that deep is drawn as bed alone, and the game now leaves cover out of its check there.
  The sawtooth itself is untouched. It looks unintended and is worth a look.

### Is it right?

- The world comes out identical, bit for bit, on the same three comparison runs as before.
- A new test plays a colony for eight days and after every step compares all of it with what
  was last reported: no tile may show a cell further than one drawn step from what is there
  now, and no source may look different without being on the list. Tests in all: 94 in the
  simulation, 15 in the game.
- Two pictures were taken from the running game (top speed in winter, normal speed in spring)
  and look right. It was not played by hand, and moving the camera was not tried.
- Saves are unchanged: none of the new bookkeeping is saved.

### What the game still works out for itself

| The game does this | How often | Cost |
| --- | --- | ---: |
| Tree detail and cutaway: every tree against every beaver | every frame, at any speed | 0.5 ms a frame |
| Remakes the ground for the season | every chunk each three game days | 0.8 ms a frame at top speed, next to nothing at normal speed |
| Alerts, idle beavers, headcounts and goods for the panels | every frame | 0.2 ms a frame |
| Dam walls while a dam is wearing | every snapshot | 0.06 ms a frame |

## Other changes asked for

- **Autosave** is now once a game year, when the new year begins, and never sooner than five
  minutes of play after the last one (at top speed a year goes by in a minute). Saving on the
  way out and Ctrl+S are as before.
- **Python and the database** are not part of the running game. The Python scripts are
  screenshot and map tools, and the SQLite file is read once, when the game is built, to
  generate constants. Rewriting or replacing either would not change the game's speed.

## Measuring it again

Full game, as measured here (prints the table on exit):

```powershell
$env:BC_PERF=10; $env:BC_PROFILE=1; $env:BC_SPEED=3; $env:BC_PERF_EXIT=50; .\target\release\beaver_colony.exe --load saves\autosave.sav
```

With `BC_PERF` set the game never writes a save. Add `$env:BC_SETTLE=1` and `--map telemark`
in place of `--load` for a new colony, and `$env:BC_PERF_SHOT="shot.png"` to save a picture of
the window three seconds before it quits. The table now also lists the game's own systems
(the rows under `game`), per frame in the last column.

Simulation alone, with the same table (`SPEED_PROFILE=1`), a world fingerprint for comparing
two builds, and the cost of a snapshot by part of the world:

```powershell
cargo run --offline --release -p sim --example speed -- saves/autosave.sav 3
```

```powershell
cargo run --offline --release -p sim --example speed -- copy saves/autosave.sav
```
