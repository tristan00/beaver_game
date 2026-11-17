# Beaver Colony – Architecture

Cargo workspace: `crates/sim` (pure Rust, deterministic, no Bevy; RNG is seeded `rand_pcg::Pcg32`)
and `crates/game` (Bevy 0.20.1, `bevy_ui` only for UI). The game reads and drives `SimState`; the
sim never depends on the game.

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
| `crates/game/src/ui.rs` | UI task | bevy_ui overlay |
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
