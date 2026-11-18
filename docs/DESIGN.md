# Design: agent-based colony, three starts, map-first interface

Contract for the follow-up tasks. Every type and function named in backticks with a path exists as a
compiling stub (checked by `cargo check --workspace`). Task keys (`nav`, `collect`, …) are the
planner's local keys; the ownership table in section 11 says which files each may edit.

## 1. Request map

Every request bullet → where it lives, its public types, and who builds it.

| # | Request bullet | Module | Types / functions | Owner |
|---|---|---|---|---|
| 1 | Beavers on a navigation grid, land/water, no overlaps | `sim::nav` | `NavGrid`, `Surface`, `Gait`, `Path`, `Goal`, `NavGrid::find_path`, `reserve`, `release` | `nav` |
| 2 | Lodge underwater entrances | `sim::nav` | `Surface::LodgeEntrance`, `Surface::LodgeInside`, `Goal::Inside` | `nav` |
| 3 | Dam climbing | `sim::nav` | `Surface::Dam`, `Gait::Climb`, `CLIMB_COST` | `nav` |
| 4 | Task state machine: collect, carry, build, haul, rest | `sim::agents` | `Task`, `Agent`, `Agents`, `Load`, `agents::step` | `collect` (+`build`) |
| 5 | Resource entities: trees→logs/branches/bark, plants, mud, stone | `sim::resources` | `ResourceWorld`, `Resource`, `ResourceKind`, `ResourceId`, `take`, `nearest` | `resources` |
| 6 | Workplaces with worker slots and work areas | `sim::work` | `Work`, `Workplace`, `Slot`, `Role`, `Area`, `assign`, `set_area` | `build` |
| 7 | Construction with delivered materials, progress | `sim::work` | `SiteProgress::fraction`, `Task::Fetch`, `Task::Build` | `build` |
| 8 | Hauling between stores | `sim::work`, `sim::agents` | `HaulOrder`, `Work::hauls`, `Command::SetHaul` | `build` |
| 9 | Orders: right-click collect/build, drag area | `sim::agents`, `game::ui::orders` | `Assignment`, `Command::Assign`, `Gesture`, `gesture_command` | `collect` (sim), `commands` (UI) |
| 10 | Society: families, needs, mood, skills, elders, priorities | `sim::colony` (existing), `sim::society` | `society::mood_factors`, `work_speed`, `Factor` | `society` |
| 11 | Join requests as decisions | `sim::society` | `JoinRequest`, `Answer`, `Society::answer`, `Command::AnswerJoin` | `society` (sim), `events_ui` (UI) |
| 12 | Water network: rivers, tributaries, springs, ponds | `sim::worldgen` | `worldgen::World` (existing; `water` adds spring and pond fields) | `water` |
| 13 | Dam preview: rise, flooding, cost, pressure; pond fills | `sim::dam_preview` | `DamPreview`, `preview` | `dam` |
| 14 | The one start (swim in, scout, burrow, dam, pond) | `sim::starts` | `new_game`, `new_game_with_dam` | `start1` |
| 17 | New industrial content: canal networks, efficient production, bigger storage | `sim::catalogue`, `sim::buildings` | `Def`, `Tier`, `def`, `available`; new `BuildingKind` variants | `content` |
| 18 | One switch disabling out-of-scope systems | `sim::features` | `Features`, `System`, `Features::enabled`, `SimState::features` | `switch` |
| 19 | Navigation bar, one screen at a time | `game::ui::shell` | `NavItem`, `Shell`, `Shell::toggle`, `Shell::bar` | `ui_shell` |
| 20 | Number provenance (hover shows where it comes from) | `game::ui::explain` | `Explained`, `Part`, `Source`, `Numbers`, `audit` | `ui_shell` |
| 21 | Icons for resources, needs, jobs, actions, map modes | `game::ui::icons` | `Icon`, `Icon::file`, `From<Good>` | `icons` (PNGs), `ui_shell` (loading) |
| 22 | Entity panels: beaver, family, building, dam, tree | `game::ui::selection` | `Selection`, `Panel`, `PanelAction`, `SlotRow`, `panel` | `panels` |
| 23 | Map modes: water, forest, food, work, with actions | `game::ui::mapmodes` | `MapMode`, `ModeAction`, `MapMode::actions` | `mapmodes` |
| 24 | Build menu, placement preview, dam hover preview | `game::ui::orders` | `Placement`, `placement` (+ `dam_preview::preview`) | `buildmenu` |
| 25 | Events/alerts that point to places | `game::ui::orders` | `Alert`, `alerts` | `events_ui` |
| 26 | Beavers visibly walking/swimming/climbing/carrying; sites rising | `game::entities_render` | reads `Agent::{cell, step_frac, gait, load}`, `SiteProgress::fraction` | `render_agents` |
| 27 | World style and terrain polish (icon art is row 21, styled by `docs/UI_SPEC.md`) | `docs/STYLE.md`, `docs/style/*.png`, `terrain_render`, `water_render` | — | `artdir`, `terrain` |
| 28 | Playtest report covering every bullet | `docs/PLAYTEST.md` | — | `playtest` |

## 2. One model, three layers

```
colony::Colony   who the beavers ARE: age, family, needs, mood, skills, lodge   (existing, society extends)
agents::Agents   where they ARE and what they DO: cell, task, load, path         (new, BTreeMap<BeaverId, Agent>)
work::Work       places to work: slots, areas, sites, haul orders                (new)
resources::ResourceWorld  things to gather: trees, plants, mud, stone            (new)
buildings::Buildings      what is built and the Stores ledger                    (existing)
nav::NavGrid     derived surfaces + cell reservations                            (new)
```

Rules that keep tasks from fighting over the model:

* `BeaverId` is the only join key. `Agent` never copies age/mood; read `Colony::beaver(id)`.
* A beaver exists in `Colony::beavers` first; `Agents::spawn` is called when it is born/arrives.
* Only `agents::step` moves agents, only `NavGrid::reserve/release` change reservations,
  only `Stores::deposit/withdraw` change totals. Totals therefore change only on delivery.
* The old `Colony::step_on` job simulation (abstract work orders) stays for the legacy example
  binaries but `collect` stops it from producing goods once agents exist (it keeps ageing,
  births, needs). `collect` owns that cut-over inside `colony.rs`'s `step_on` call sites only.
* Determinism: all iteration is over `BTreeMap`/`Vec` in id order; one `Pcg32` per system.

`SimState::step` order: time → water → ecology → colony (ageing, needs, births) → tech →
buildings → region → `resources.step` → `agents::step` → `society.step` → events.
New fields on `SimState`: `features`, `nav`, `resources`, `agents`, `work`, `society`.

## 3. Navigation (`sim::nav`)

* `Surface` per cell: `Land`, `Water` (depth > 0.05, from `SimState::water`/rivers/lakes), `Dam`,
  `Solid` (tree, stone, building body), `LodgeEntrance` (a water cell beside the lodge), `LodgeInside`.
* Cost per step: `WALK_COST` 1.0, `SWIM_COST` 0.6 (water is usually faster), `CLIMB_COST` 3.0 over `Dam`.
* `Gait` of a step follows the destination surface; renderers pick walk/swim/climb poses from it.
* Lodge interior is entered only from `LodgeEntrance`; `Goal::Inside` plans through it.
* `Goal::Beside` is for solid targets (a tree to fell, a site to build).
* **No overlaps:** an agent holds its cell and the cell it is stepping into (`reserve` before move,
  `release` after). A beaver that cannot reserve waits (`stuck_ticks`) and replans after a delay.
  `inside == true` agents hold no map cell.
* `NavGrid::rebuild` runs when trees fall, sites complete or dams change, not every tick.
* Budget: 300 agents' path planning < 2 ms per tick in release (cached paths, A* bounded by area).

## 4. Agents and the task state machine (`sim::agents`)

States (`Task`): `Idle`, `Collect`, `Carry`, `Fetch`, `Build`, `Rest`. Diagram is in the module doc.

* Assignment (`Assignment::{Resource, Area, Slot}`) is player intent; `Task` is what the agent is doing now.
* Collector loop: `Collect` until `Load` is full (or resource exhausted) → `Carry` to nearest
  complete store (`Buildings` with a `Stores`; swims if cheaper) → deposit → repeat.
  Area-assigned collectors only target `ResourceWorld::nearest(kind, from, Some(area))`.
* Builder loop (`build`): `Fetch` materials from a store → `Carry` to the site → deposit into
  `SiteProgress::delivered` → `Build` adds `labour_done` scaled by `society::work_speed`.
  Site completes when `fraction() == 1.0` and costs were exactly delivered.
* Hauler loop (`build`): `Fetch` from `HaulOrder::from` → `Carry` to `to` → deposit.
* Rest: needs low, night, no assignment, or kit → walk to lodge entrance → `inside`.
  Kits never work; unassigned adults rest.
* `Command::Assign { who, to }` sets or clears assignment; unknown ids are `InvalidCommand`.

## 5. Resources (`sim::resources`)

`ResourceKind`: `Tree` (solid) → felled by work into `Felled` (not solid; yields `Logs`, `Sticks`
and bark counted as branch yield for now), `Plant` (`Food`, regrows to `max_amount`, only on wet
ground), `MudBank`, `StoneOutcrop`. `ResourceWorld::seeded(seed, world)` is deterministic per seed.
`take(id, want)` returns the good and amount, depletes, and converts trees. Bark as its own
good is not added; `content` may add a bark good later; `Icon::Bark` is already reserved.

## 6. Workplaces (`sim::work`)

Every building has a `Workplace` (slots from `catalogue::def(kind).slots`); a site has
`site: Some(SiteProgress)`. Slots hold a `BeaverId`; `Work::assign` / `unassign` back
`Command::FillSlot` / `EmptySlot`. A slot worker's collection is clamped to `Workplace::area`;
`Command::SetWorkArea` moves or resizes it (`Area` is an inclusive cell rectangle). Slot filling
beats family priorities; assigning a beaver to a slot clears their old slot.

## 7. Society (`sim::society` + `colony`)

Kept in `colony.rs`: pairs, births, ageing, needs, skills, family priorities. `society` adds:
`JoinRequest` (a decision with a map cell), `Answer`, `Society::answer`, `mood_factors` (terms
that sum to `Beaver::mood`, feeding number provenance), `work_speed(mood)`. Low mood slows work
via `work_speed`. Births need food and a free lodge bed (rules already in `colony.rs`; `society`
adds tests and the lodge-founding hook for grown families).

## 8. The one start (`sim::starts`)

There is a single start. `starts::new_game(seed) -> SimState` (and `new_game_with_dam`, which also
returns the dam the world is designed around) is deterministic and does not simulate forward:
two adults and two kits in the river beside a dry landing bank, no building of any kind, no
dam. The game opens on them swimming, camera on them; there is no start menu and no `--start`.
The player places a burrow (`PlaceSite` Burrow: no materials, 6 labour) and a dam; the adults dig
and fell and haul wood straight to the site, kits never work, and the dam completes only from
delivered wood. The old lake starts survive only as test fixtures (`sim::lakes`, feature
`fixtures`).

## 9. Content (`sim::catalogue`)

`def(kind) -> Def { tier, slots, area, unlock, system }` is the single table for slots/area/
unlock; cost, footprint and site rule stay in `BuildingKind::spec`. `content` adds canal networks
that agents path along as cheap `Water`, higher-yield production buildings (more per worker than
elementary), bigger storage. `available(sim)` hides kinds whose `system` is disabled.

## 10. The switch (`sim::features`)

`Features { disable_out_of_scope }`, `Features::scoped()` (default), `everything()`,
`enabled(System)`. `System::OUT_OF_SCOPE` = predators, floods, droughts, guard, rivals, trade,
scouting. Owners of those systems wrap their firing/effect code in `features.enabled(..)`;
view-models never build rows for disabled systems. `Command` variants for them must return
`InvalidCommand`. Code stays compiled.

## 11. Interface architecture (`game::ui`)

First view is the map: only the map, one **navigation bar** (bottom centre) and the **time
controls** (top right, with the season and colony day) are on screen. Everything else opens on
demand. Detail follows interest: bar icon → screen with dense, sortable tables → hover on a
number → what adds up to it.

**Screens** (`shell::NavItem`, one open at a time via `Shell::toggle`; clicking the open item,
its Close button or Esc closes it):

| Screen | Shows | Actions |
|---|---|---|
| **Colony** | beavers, mood and the four needs (food, rest, shelter, company) as headline numbers; Families and Beavers tables | Show a beaver on the map |
| **Work** | working/idle, Jobs table (workers, skill, XP, best hand), open work orders | More / Fewer on a job, Show an order |
| **Stores** | larder, wood and every good: stock with fill bar, capacity, made/used/net per day | none (read and compare) |
| **Buildings** | lodges, finished buildings and sites, and the Build menu with costs | Build (picks the tool and closes the screen), Show |
| **Tech** | branch experience, every tech with tier, cost, state, what it gives | none |
| **Events** | decisions waiting (badge on the bar icon), join requests, event log | Choose an option, Welcome / Turn away |

View-models live in `ui/screens.rs` (`screen`, `nav_bar`, `time_bar`); `ui/panels.rs` only draws
them and runs clicks. Rules, enforced by tests in `screens.rs`:

* **Explained numbers:** every number is an `Explained` whose parts add up to it (`audit`);
  hovering shows the parts (`Tip`). No label+value appears twice on one screen.
* **Icons:** every resource, need, job and action entry carries an `Icon` whose PNG exists in
  `assets/icons`; table rows always have one. Text is a caption, never the only marker.
* **Sortable:** every table header sorts (click again to reverse).
* `--open colony|work|stores|buildings|tech|events` opens a screen at start (screenshots).

Still to come from other tasks: entity panels (`selection::panel`), map modes
(`mapmodes::MapMode`), placement and orders (`orders`), alerts with locations.

## 12. File ownership

Owner may edit only its files; shared files are changed only where noted. Any other change goes
through the public API above (or a follow-up request to the owner).

| Task | Files |
|---|---|
| `design` (this) | `docs/DESIGN.md`, `ARCHITECTURE.md`, `crates/sim/src/lib.rs` (composition: mods, fields, `Command`, step order) |
| `switch` | `sim/src/features.rs`; the `if features.enabled` guards in `events.rs`, `region.rs`, `colony.rs` (guard/scout paths); `ui/ledgers.rs` trade rows |
| `water` | `sim/src/worldgen.rs`, `sim/src/water.rs` |
| `nav` | `sim/src/nav.rs` |
| `resources` | `sim/src/resources.rs`, `sim/src/ecology.rs` |
| `collect` | `sim/src/agents.rs`; `step_on` cut-over in `sim/src/colony.rs` |
| `build` | `sim/src/work.rs`, `sim/src/buildings.rs`; Build/Fetch/Haul code in `agents.rs` after `collect` merges |
| `society` | `sim/src/society.rs`, `sim/src/colony.rs` (needs/births/skills), `sim/src/tuning.rs` colony block |
| `dam` | `sim/src/dam_preview.rs`, pond fill in `sim/src/water.rs` |
| `content` | `sim/src/catalogue.rs`, `sim/src/tech.rs`, `sim/src/tuning.rs` buildings block |
| `start1` | `sim/src/starts.rs` (River), `game/src/cli.rs` (`--start`), `game/src/flow.rs`, delete `game/src/midgame.rs` |
| `start23` | `sim/src/starts.rs` (EarlyLake, IndustrialLake; separate fns) |
| `artdir` | `docs/STYLE.md`, `docs/style/*.png`, `tools/style_sheets.py` |
| `icons` | `assets/icons/*`, `docs/icons_sheet.png`, `tools/make_icons.py` (styled by `docs/UI_SPEC.md`) |
| `terrain` | `game/src/terrain_render.rs`, `water_render.rs` |
| `ui_shell` | `game/src/ui/{mod,shell,screens,explain,icons,kit,panels}.rs`, `game/src/opening.rs`, the time controls in `flow_view.rs` |
| `render_agents` | `game/src/entities_render.rs`, `entities_render/*` |
| `panels` | `game/src/ui/selection.rs` |
| `commands` | `game/src/ui/orders.rs` (`Gesture`, `gesture_command`), `game/src/input.rs` |
| `mapmodes` | `game/src/ui/mapmodes.rs` |
| `buildmenu` | `game/src/ui/orders.rs` (`Placement`, `placement` only), `game/src/build.rs`, `build_logic.rs` |
| `events_ui` | `game/src/ui/orders.rs` (`Alert`, `alerts` only), `game/src/ui/events.rs` |
| `playtest` | `docs/PLAYTEST.md` |

`orders.rs` is shared by three tasks: each edits only its named items and appends new code below
a `// --- <task> ---` marker to avoid merge conflicts. `lib.rs` is edited by others only to add a
`Command` arm for their own variants.
