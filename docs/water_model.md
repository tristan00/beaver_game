# Water model

The playable game now uses routed hydrology: streams carry water down a drainage graph, and
reservoirs retain a finite volume below their outlets. The one-metre terrain, dams, and canals
remain the source of the drainage geometry. The previous shallow-water solver is available
only for diagnostics with `BC_WATER_DETAILED=1`.

## Storage and flow

A priority flood finds each depression's spill elevation and the order in which water can
leave the map. Connected depressions at the same spill elevation become reservoirs. Flat
stream cross-sections share a water level; sloping stream reaches remain separate. A reservoir's
level comes from its stored volume and the actual sorted bed elevations, not a prescribed depth.

Water is routed through this graph in intervals of at most fifteen simulated minutes. Each reach
solves its remaining storage and outgoing flow together. Channel conveyance uses Manning's
law; dam crests and reservoir outlets use a weir law. A junction can send water down multiple
outlets. Internal transfers remove exactly the amount added downstream, and a finite reach
cannot send more water than it has. Map-edge exchange, external lakes, sources, rain,
evaporation, and seepage are booked separately.

Building or breaching a dam and digging a canal invalidate the graph. The next step rebuilds
it from the changed terrain while retaining the water already present. Multiple dams can hold
separate reservoirs, and lower canals can divert their inflow and drain their storage.
Construction raises dams in 5% layers, so a builder's minute-by-minute progress does not
rebuild the drainage graph each minute. Integrity repairs leave the geometry cache intact.
Reservoir volumes and weather changes use double precision; the published depth grid is a
rendering/navigation view and is not repeatedly summed back into the stored water volume.

## Time and weather

The colony and water use the same calendar: one simulated day includes 86,400 seconds of
water forcing. Changing game speed changes how many fixed steps are requested, not the length
of a water day. CPU overload slows the whole simulation and drops wall-clock catch-up debt.
It does not advance food, aging, or weather ahead of the water.

Seasonal source flow, rainstorms, drought, evaporation, seepage, freezing, and thawing remain
active. Catchment rainfall affects source flow. Direct rain is applied to existing surface
water; microscopic rain sheets across otherwise dry terrain are not simulated. Water budgets
and climate are updated every fifteen simulated minutes.

## Deliberate approximations

This is a gameplay hydrology model, not a wave simulation. It omits momentum, hydraulic jumps,
and resolved flood-wave travel. Water can travel along a connected reach within one update.
Flow follows the drainage graph; transient flow reversals and channel backwater are not solved.
A reservoir uses one level across its depression, so small internal ridges can equalize earlier
than they would in a full flood model. A high secondary spill route that is outside the drainage
graph is not a general two-dimensional overland flood route.

Ripples and foam are rendering effects driven by the published flow. They do not determine
water volume or depth. F8 hides tree crowns so waterworks remain visible without removing trees
or resources from the simulation.

## Running

Build with `cargo build --offline --release -p game`. `Play Beaver Colony.cmd` opens the normal
game; `River Sandbox.cmd` opens Telemark with instant construction. In the sandbox, F6 changes
weather, F7 advances the season, Delete breaches a dam under the cursor, Space pauses, and 1/2/3
select speed. F8 toggles tree crowns in both modes.

The "Dam weakening" warning offers Repair. This queues ordinary materials and builder work,
preserves each dam cell's height, and restores integrity when the work finishes. Repeated
clicks reuse the pending order. Cancelling a repair leaves the existing dam standing.

## Verification of this change

Manual simulation checks on 2026-11-19 covered a closed reservoir receiving 360 cubic metres,
a sloping four-metre-wide channel with a calculated depth of 0.0631 m, a one-metre dam, and a
breach. They produced the expected storage, normal depth, upstream rise, and drainage.

A scenario with two dams, a bypass canal, branching canals, a breach, storm, drought, freezing,
and thawing completed with its water-budget checks passing. All four real maps completed a
storm run and maintained their water budgets. All four maps completed settlement checks.
The earlier one-minute loop took about 59 seconds for a 30-day Telemark opening. The faster
quarter-hour loop and coarser ecology updates are checked against rendered month timings as well as headless runs.

Focused manual checks covered paid dam repairs, cancellation, duplicate clicks, variable
dam heights, home access during dense construction, and moving an occupant out before a
site becomes a finished building. No automated test suites were added to the repository.

## Scripted colony reports

`beaver_colony --script FILE --report-dir DIR` executes the existing colony-script language,
opens that actual colony state, saves nine native views, exports household/building/event
CSVs and a summary, and exits. `--script FILE --screenshot FILE.png` captures a single view.
Set `BC_HUD=1` for the HUD in single screenshots. No desktop input is required.
`BC_REPORT_BENCH=1 BC_PERF=1 RUST_LOG=info` additionally benchmarks the rendered colony for
15 seconds at maximum speed after the report images have been written.

The maximum speed requests six game days per real second; the target is a month in 5–10
seconds. All speeds use the same 15-minute simulation decisions. Whole-map vegetation
and wildlife refresh every six game hours. Water, food, aging, and construction still advance
on the same calendar. Beavers may pass one another; solid terrain, buildings, and lodge
entrances continue to constrain their routes.

## Wet settlements and families

Lodges, food caches, logging/foraging camps, workshops, warehouses, forester lodges, mills,
storehouses, and water wheels require water at least 0.10 m deep beneath a strict majority
of the footprint. They become stranded if that majority loses water. Burrows, mud pits,
docks, scent mounds, and waterworks remain shoreline structures. Quarry camps, nurseries,
and guard posts are dry-land exceptions. Canal research now needs ten Water experience
and a population of six, after Lodge building, so wet expansion is available earlier.

Each occupied lodge belongs to one family. Newcomers require an empty home or a compatible
unpaired adult; regional arrivals follow the same rule. The founding family moves from its
burrow when its first lodge is ready. Parent records survive death and household changes;
close relatives cannot pair, widowed parents retain their children, and orphans can join an
adoptive household without losing their recorded biological parents.

The 360-day calendar models one annual litter of 2–4 kits after 107 days of gestation.
Mating occurs in midwinter, births in spring, yearlings begin helping at one year, and adult
offspring can establish a new family at two years when a home and an unrelated mate are
available. The arriving mother is already pregnant from the prior winter; the two existing
young are the prior spring's offspring. Three-year isolated checks cover gestation, annual
litter size, family-only housing, and unrelated reciprocal mates. These abstractions follow
[Indiana DNR's beaver life history](https://www.in.gov/dnr/fish-and-wildlife/wildlife-resources/animals/beaver/).
