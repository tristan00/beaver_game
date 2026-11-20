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

Water is routed through this graph in intervals of at most one simulated minute. Each reach
solves its remaining storage and outgoing flow together. Channel conveyance uses Manning's
law; dam crests and reservoir outlets use a weir law. A junction can send water down multiple
outlets. Internal transfers remove exactly the amount added downstream, and a finite reach
cannot send more water than it has. Map-edge exchange, external lakes, sources, rain,
evaporation, and seepage are booked separately.

Building or breaching a dam and digging a canal invalidate the graph. The next step rebuilds
it from the changed terrain while retaining the water already present. Multiple dams can hold
separate reservoirs, and lower canals can divert their inflow and drain their storage.

## Time and weather

The colony and water use the same calendar: one simulated day includes 86,400 seconds of
water forcing. Changing game speed changes how many fixed steps are requested, not the length
of a water day. CPU overload slows the whole simulation and drops wall-clock catch-up debt.
It does not advance food, aging, or weather ahead of the water.

Seasonal source flow, rainstorms, drought, evaporation, seepage, freezing, and thawing remain
active. Catchment rainfall affects source flow. Direct rain is applied to existing surface
water; microscopic rain sheets across otherwise dry terrain are not simulated. Water budgets
and climate are updated every five simulated minutes.

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

## Verification of this change

Manual simulation checks on 2026-11-19 covered a closed reservoir receiving 360 cubic metres,
a sloping four-metre-wide channel with a calculated depth of 0.0631 m, a one-metre dam, and a
breach. They produced the expected storage, normal depth, upstream rise, and drainage.

A scenario with two dams, a bypass canal, branching canals, a breach, storm, drought, freezing,
and thawing completed with its water-budget checks passing. All four real maps completed a
storm run and maintained their water budgets. The normal Telemark colony also completed a
five-day startup run. These checks do not establish two-winter food balance or certify every
possible canal arrangement. No automated test suites were added to the repository.
