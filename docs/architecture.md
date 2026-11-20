# Code ownership

The `sim` crate owns game rules and state. The `game` crate presents that state
and translates input into `sim::Command`. Rendering and HUD code should issue
commands rather than implement a second version of simulation rules.

## Simulation

- `lib.rs` exposes the public types, constructs `SimState`, and orders the fixed
  simulation step. Step order is part of behavior: preserve it when reorganizing
  systems.
- `command.rs` defines commands, dispatches them, and records successful commands
  for replay. `sim::Command` and `SimState::apply` remain the public entry points.
- `construction.rs` owns prerequisites, site validation, paid building/dam/canal
  orders, and cancellation/refunds. `SimState::check_site` returns typed errors
  and is shared by previews, commands and the scripted player.
- `catalogue.rs` supplies one `BuildingKind::spec()` definition for each building:
  its footprint, terrain, cost, labour, prerequisites, worker slots, work area
  and storage capacity. Worker roles are static slices rather than fresh vectors
  allocated during queries. The former tier and production-batch metadata is gone.
- `work.rs` owns worker assignments, delivery requirements and construction
  progress. `agents.rs` performs movement, gathering, hauling and physical work.
- `colony.rs` owns needs, skills, families, aging and population events. It reads
  actual worker assignments; it does not manufacture resources or construct
  homes in an independent abstract economy.
- `colony_bridge.rs` connects colony needs to physical inventories, homes and
  worker jobs, and applies world events. Inventory mirroring and settlement stay
  together so consumption is charged once to the correct resource.
- `resources.rs`, `forestry.rs`, `shipping.rs`, `maintenance.rs`, and the water
  modules own their respective systems. Economy tuning remains in the existing
  tuning/constants locations and the building catalogue; structural refactors
  should not change values.

There is one paid construction path. The unused prototype work-order queue,
automatic economy switches, and `Buildings::place`/`cancel` APIs were removed.
Use commands and work sites instead. Diagnostic instant-placement APIs remain
available for their existing callers.

Shipping has one implementation in `shipping.rs`. Barges carry their goods in
`hold`; the unused dispatch/cargo/destination path and its second movement loop
were removed. World exploration uses the fog grid directly; the always-open
single-valley registry and no-op unlock polling were removed.

The routed water solver is the only solver for both gameplay and the sandbox.
The opt-in shallow-water solver, pool scheduler, parallel kernels, activity masks
and their unused diagnostics were removed. `SimState::step` is the common clock.

The disabled rivals, trade, predators and disaster event implementations and
their feature switches were removed. The two active ambient arrival effects
remain in `events.rs`; player-facing immigration requests remain in `society.rs`.

## Interface

- `hud.rs` owns shared HUD state, system scheduling and panel refreshes.
- `hud/input.rs` handles keyboard/pointer input, selection and camera interaction.
- `hud/actions.rs` translates UI actions into commands and feedback.
- `hud/minimap.rs` owns the minimap image and navigation.
- `hud/words.rs` owns display names, icons and formatting. Worker-role labels and
  icons share one mapping after normalization through `sim::jobs::role_kind`.
- The existing panel, inspector, world-overlay and UI modules render views.

Keep implementation helpers scoped to their owning module. Renderer systems
that use private model types are visible only to the renderer parent module.

Entity rendering reads the live simulation through `entities_render/sim_sync.rs`.
The fake colony/stress scene, showcase plugin, HUD demonstration fixtures and
demo-pond driver were removed, along with their CLI options. Procedural artwork
remains procedural. Screenshots, colony scripts, reports and the real-map sandbox
remain available for exercising the actual game.

## Refactor checks

Run `cargo test --offline -p sim` for module tests and
`cargo check --offline --workspace --all-targets` for all callers. Build the
release game and inspect a native frame after interface changes.

For simulation refactors, compare the same commands and management policies
before and after, including daily inventories, population, events and command
logs. The current refactor's three 720-day comparisons live in the workspace's
`tmp/structure-refactor` folder. Long integration runners stay outside the
repository; committed tests are fast module tests.

The 2026-11-20 legacy-removal pass is verified in `tmp/legacy-refactor` at the
workspace root. Three 720-day runs (managed Telemark, neglected Telemark and
cautious Algonquin) exactly match the prior daily results, commands, colony
events and recorded final state. All 29 unit tests and the all-target build
check pass, as do 19 sandbox checks covering dams, branching canals, breaches,
storms, drought, freezing, thawing and water budgets. Native renders were checked
with normal crowns and the existing cutaway view.

Source count: 33,995 to 29,120 lines, a reduction of 4,875. The 20,000-line limit
is still unmet by 9,120 lines; `tools/check_size.py` continues to fail until that
remaining reduction is implemented. The counting rules and limit are unchanged.
