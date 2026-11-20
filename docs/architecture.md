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
  orders, and cancellation/refunds.
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
  tuning/constants locations; structural refactors should not change values.

There is one paid construction path. The unused prototype work-order queue,
automatic economy switches, and `Buildings::place`/`cancel` APIs were removed.
Use commands and work sites instead. Diagnostic instant-placement APIs remain
available for their existing callers.

## Interface

- `hud.rs` owns shared HUD state, system scheduling and panel refreshes.
- `hud/input.rs` handles keyboard/pointer input, selection and camera interaction.
- `hud/actions.rs` translates UI actions into commands and feedback.
- `hud/minimap.rs` owns the minimap image and navigation.
- `hud/demo.rs` contains explicitly requested diagnostic/screenshot setups.
- `hud/words.rs` owns display names, icons and formatting. Worker-role labels and
  icons share one mapping after normalization through `sim::jobs::role_kind`.
- The existing panel, inspector, world-overlay and UI modules render views.

Keep implementation helpers scoped to their owning module. Renderer systems
that use private model types are visible only to the renderer parent module.

## Refactor checks

Run `cargo test --offline -p sim` for module tests and
`cargo check --offline --workspace --all-targets` for all callers. Build the
release game and inspect a native frame after interface changes.

For simulation refactors, compare the same commands and management policies
before and after, including daily inventories, population, events and command
logs. The current refactor's three 720-day comparisons live in the workspace's
`tmp/structure-refactor` folder. Long integration runners stay outside the
repository; committed tests are fast module tests.
