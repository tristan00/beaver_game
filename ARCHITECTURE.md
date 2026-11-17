# Beaver Colony – Architecture

Cargo workspace: `crates/sim` (pure Rust, deterministic, no Bevy; RNG is seeded `rand_pcg::Pcg32`)
and `crates/game` (Bevy 0.20.1, `bevy_ui` only for UI). The game reads and drives `SimState`; the
sim never depends on the game.

## Colony model (new, see `docs/DESIGN.md`)

`docs/DESIGN.md` is the contract for the agent-based colony: request-to-module map, state
machine, starts, content, the feature switch, UI architecture and per-task file ownership. It
supersedes the ownership table below for the files it lists. New sim modules: `agents`, `nav`,
`resources`, `work`, `society`, `starts`, `dam_preview`, `catalogue`, `features`; new game UI
modules: `ui/{shell,explain,icons,selection,mapmodes,orders}`. `SimState` gains `features`, `nav`,
`resources`, `agents`, `work`, `society`, and `Command` gains `Assign`, `FillSlot`, `EmptySlot`,
`SetWorkArea`, `SetHaul`, `AnswerJoin`.

## File ownership

Each later task owns only the files listed for it and must not edit other tasks' files except through
the public APIs below. `R12-2` (this scaffold) owns everything until a later task claims a file.

| File | Owner task | Purpose |
|---|---|---|
| `Cargo.toml`, `ARCHITECTURE.md`, `.gitignore` | R12-2 (scaffold) | Workspace, this document |
| `crates/sim/src/lib.rs` | R12-2 | `SimState`, `Command`, `SimError` composition only |
| `crates/sim/src/grid.rs` | R12-2 | Shared `Grid<T>`; height/water/vegetation grid aliases |
| `crates/sim/src/worldgen.rs` | worldgen task | Seeded terrain generation (currently value-noise heightmap) |
| `crates/sim/src/water.rs` | water task | Water depth field and flow |
| `crates/sim/src/ecology.rs` | ecology task | Vegetation and wildlife |
| `crates/sim/src/colony.rs` | colony task | Beavers, jobs, resources |
| `crates/sim/src/buildings.rs` | buildings task | Lodges, dams, structures |
| `crates/sim/src/tech.rs` | tech task | Research/unlocks |
| `crates/sim/src/region.rs` | region task | Wider map, neighbours |
| `crates/sim/src/events.rs` | events task | Random events |
| `crates/game/src/main.rs` | R12-2 | Parses CLI, only adds plugins |
| `crates/game/src/cli.rs`, `screenshot.rs`, `state.rs` | R12-2 | CLI flags, screenshot mode, `Sim` resource |
| `crates/game/src/camera.rs`, `input.rs` | camera/input task | Orbit camera + presets; mouse/keyboard |
| `crates/game/src/terrain_render.rs` | terrain render task | Heightmap mesh |
| `crates/game/src/water_render.rs` | water render task | Water surface (stub) |
| `crates/game/src/entities_render.rs` | entities render task | Beavers/buildings (stub) |
| `crates/game/src/ui/` (`mod`, `shell`, `screens`, `explain`, `kit`, `panels`) | UI task | Navigation bar and six on-demand screens (Colony, Work, Stores, Buildings, Tech, Events). `screens.rs` holds the pure view-models (`cargo test -p game screens`); `panels.rs` draws and handles clicks; see `docs/DESIGN.md` §11 |
| `crates/game/src/midgame.rs` | UI task | `--midgame`: seeded mid-game colony for screenshots |
| `crates/game/src/opening.rs` | UI task | Default start: founders settled at the best site (`--open NAME` opens a screen at start) |
| `crates/game/src/audio.rs` | audio task | Sound (stub) |

One system per file; one `Plugin` per game file.

## SimState API (`crates/sim/src/lib.rs`)

```rust
SimState::new(seed: u64) -> SimState
SimState::step(&mut self, dt_days: f32) -> Result<(), SimError>   // Err(InvalidDt) if NaN/inf/negative; state unchanged
SimState::apply(&mut self, cmd: Command) -> Result<(), SimError>  // Err(InvalidCommand) leaves state unchanged
enum Command { Regenerate { seed: u64 } }                         // extend per system
enum SimError { InvalidDt(f32), InvalidCommand(String) }
```

Public fields: `seed`, `time_days: f64`, `height: HeightGrid`, `water: Water` (`depth`),
`ecology: Ecology` (`vegetation`), `colony`, `buildings`, `tech`, `region`, `events` (stubs with
`step(dt_days)`). `Grid<T>`: `new(w,h,fill)` (panics on zero size), `get/get_mut(x,y) -> Option`,
`width/height/as_slice`. Same seed ⇒ identical state.

## Running

```
cargo run -p game -- [--seed S] [--cam iso|top|low]
cargo run -p game -- --screenshot out.png [--frames N] [--seed S] [--cam preset]
```
`--screenshot` waits until the terrain is actually renderable (terrain mesh instance extracted,
mesh and material prepared, no render pipelines queued/compiling, held for 3 consecutive frames)
and at least N frames have run (`--frames` is a minimum, default 10), then saves a PNG and exits 0.
It exits non-zero instead of writing a blank image if a pipeline fails to compile, the terrain is
not renderable within 180 s, or the file is not written. Bad arguments exit 2.
Controls: mouse drag orbit, wheel zoom, Q/E rotate.

Events now fire inside `SimState::step` (`step_events`, applied to the colony through `sim::ColonyHost`), and neighbour migrants join the colony there too.
`cargo run -p game -- --seed 1 --open stores --screenshot out.png` shows the Stores screen on the default start.
`cargo run -p game -- --midgame --seed 3 --screenshot docs/midgame.png` shows the ledgers, top bar and event cards on a settled, year-old colony. `BC_UI_TAB=trade|stores|families|beavers|jobs|tech|events` opens a detail tab at start (debug aid).

## Drawing the sim (R19-18)

`entities_render/sim_view.rs` maps sim state to visuals (pure functions, pose table in its header);
`sim_sync.rs` keeps one `BeaverSprite` per `sim.agents` entry (translation = sim position, exactly:
cell centre + `step_frac` towards the next path cell, plus the sub-step time since the last fixed
step, which is 0 in a stepped scene), plus entities for `sim.resources` (standing/felled trees with
the logs left, mud, stone, plants) and `sim.buildings` (sites by `work.progress` stage 0-2, finished
stage 3). `attach_rigs` adds the animated body; `animate_beavers` only eases heading and blends poses.
`--stress` still draws the fixed 5000-tree / 300-beaver stub scene; `BC_STRESS_SIM=1 --stress --midgame`
measures the live sim-driven world, filled to 300 beavers with the clock running (the log names which
scene ran and counts the beaver sprites and trees actually drawn). `--showcase poses|world` (`showcase.rs`, `BC_STATION=0..5`) stages
still scenes; `tools/contact_sheet.py` tiles the six pose shots. Test: `cargo test -p game sim_tests`.
