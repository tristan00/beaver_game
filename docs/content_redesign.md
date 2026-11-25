# Content redesign: the database is the game's content, the code is the rules

## Why

Renaming a job took 1,500 lines and hours of work. That shouldn't be possible. Today every
piece of content exists three times:

1. a row in `crates/sim/content/game.sqlite3`,
2. a hand-written Rust enum that `build.rs` checks the rows against, in the same order,
3. dozens of `match` arms and `== Kind::X` checks across both crates that decide behaviour,
   text, icons, models and colours by naming the item.

| Content | Entries | Places that name one in code | Files |
|---|---|---|---|
| Goods | 5 | 406 | 44 |
| Resources | 7 | 261 | 27 |
| Buildings | 10 | 236 | 30 |
| Jobs (`Job`, `RoleKind`, `Role`, `Assignment`) | 9 | ~260 | ~20 |
| Trees (sim `Tree` + game `Species`, in a different order) | 8 | ~140 | ~10 |
| Food plants | 11 | ~50 | ~6 |
| Content icons | ~35 of 120 | 539 icon refs | 24 |

Variant order is also baked into save files (postcard writes enums and `[T; N]` arrays by
index), so reordering a table silently corrupts saves.

The hardcoding has already caused bugs:
- the winter-food alert offers to build a stick pile to make room for food
  (`sim/src/hud.rs:330,336`), but stick piles don't take food;
- the store-full alert always offers a stick pile, whatever good is full (`sim/src/hud.rs:602`).

## The end state

**Renaming anything is one database cell. Adding a building, good, job, tree or plant that
uses existing behaviour is only database rows, with no code changes.**

### Rules (for every agent working on this codebase)

1. **Code never names a content item.** No `BuildingKind::StickPile`, no `Good::Sticks`, no
   `"Forager"`. Code asks about traits instead: "does this building store good g", "is this
   good edible", "which resource group does this job gather".
2. **If code needs to tell items apart, that's a missing column.** Add the column and fill it
   in the database. Don't add a `match`.
3. **Truly unique mechanics are *behaviours*, not items.** A short, closed code enum per content
   type lists the mechanics that need their own code (dam water physics, lodge housing). Each
   row names its behaviour, and code may `match` on behaviours, never on items. Most rows use
   the plain behaviour.
4. **Display text, icons, models and colours all come from the row.** The game crate keeps
   *registries* of drawing functions keyed by a short id (`"lodge"`, `"feed_pile"`,
   `"broadleaf"`), and rows name the function they use. A registry key is the name of a
   drawing function, not of an item: renaming the food cache doesn't touch it.
5. **Each row has a `key` (stable, used in saves, never shown) and a `name` (shown, rename
   freely).**

### What stays in code

These are rules, not content, and keep their enums: the beaver `Task` state machine,
`Command`, `HudAction`, water and terrain, dam/canal physics, wolf and wildlife rigs, UI skins,
non-content icons, and seasons. Tuning numbers (`gameplay_constant`, 449 rows) stay compile-time
constants for now. They're numbers, not names, so they don't cause the rename problem.

## Architecture

### Loading

- New `sim/src/content.rs` reads the database **at startup**, validates it, and builds one
  `Content` value: a `Vec` of rows per table plus key→id lookup maps. It's held in a
  `static CONTENT: OnceLock<Content>` behind `content()`. There's one database per process, so
  nothing has to be threaded through every function signature. Tests load the same file
  (or a fixture) once.
- `build.rs` stops generating `catalogue.rs`, `SPECS`, `FLORA` and `DEFAULT_ROLE_ORDER`. It
  keeps the tuning constants only.
- Validation errors name the table, row and column, and stop the game at startup instead of
  failing a build: a missing icon or model key, a cost naming an unknown good, a map without
  flora.
- Editing content no longer needs a rebuild (today that's about 1 minute), only a restart.

### Ids

- Newtype indices: `GoodId(u8)`, `ResourceId(u8)`, `JobId(u8)`, `BuildingId(u16)`,
  `TreeId(u8)`, `PlantId(u8)`. Each is an index into its `Vec` in `Content`.
- Per-item arrays (`Stores`, skills, experience, ledgers, `RunStats.built`, deeds) become
  `IdVec<GoodId, T>` (a `Vec` sized from `Content` at creation), not `[T; N]`.
- `BTreeMap`s keyed by derived `Ord` (shore limits, deeds) are keyed by id.

### Saves

- A save writes a **key table**: the `key` of every item in each content table, in id order.
  On load, ids are remapped through it. Reordering or adding rows then never breaks a save,
  and only removing a row that the save uses does (it fails with a clear message).
- The `Extras` … `Extras12` chain in `save.rs` folds into the main state, and the save-only
  legacy variants (`Role::Plant`, `Role::Upkeep`, `Assignment::Slot`) are deleted.
- **This means old saves stop loading once (`FORMAT` 5 → 6).** See *Open decision*.

### Tables (new or extended)

Columns in **bold** are new. Existing columns stay.

**`good`** (new): `key`, `name`, **`plural`**, **`icon`**, **`colour`**, **`load_mesh`**,
**`held_in_paws`**, **`edible`**, **`spoils`**, **`load_size`**, **`water_pace`**,
**`land_pace`**, **`carries_species`**, **`reserve_base`**, **`reserve_per_building`**,
**`reserve_rule`** (`Fixed` | `Appetite`), `order`.
*Replaces* `Good` and the matches in `agents.rs`, `jobs.rs:48`, `hud/words.rs`, and
`models.rs:537,570,632`.

**`resource`** (new): `key`, `name`, **`icon`**, **`behaviour`** (`Deposit` | `Tree` |
`Felled` | `FoodPlant` | `FlowingDeposit`), **`group`** (what a job gathers: WaterPlant and
Plant share one), **`yields`** (rows in `resource_yield`), **`skill`**, **`freezes`**,
**`min_take`**, **`max_gatherers`**, **`needs_building`**, **`solid`**, **`pose`**
(`Gnaw` | `Dig` | `Forage`), **`lens_colour`**, **`mesh`**.

**`job`** (merges `Job` + `RoleKind`; `role_policy` folds in): `key`, `name`, **`plural`**,
**`doing`** ("Felling"), **`icon`**, **`path_colour`**, `priority`, **`duty`** (`Gather` |
`Build` | `Haul` | `Forest` | `Watch` | `Mend`), **`gathers`** (resource group, for `Gather`),
**`shore_limit`**, **`shows_speed`**. Epithet pools are already keyed by job in `name_pool`
and switch to the job `key`.

**`building`** (extended): + **`behaviour`** (`Plain` | `Home` | `Dam` | `Canal` | `Balancer` |
`Watchpost` | `RaftDock` | `Nursery`), **`stores`** (rows in `building_store`: building, good),
**`unlocks`** (feature this building enables: `pebbles`, `gates`, `locks`), **`icon`**,
**`model`**, **`footing`** + **`footing_size`**, **`hotkey`**, **`menu_order`**, **`menu_group`**,
**`outline`** (`Rect` | `Round`), **`can_reinforce`**, **`can_face`**, **`reinforce_factor`**,
**`wear_rate`**, **`alert_rank`**. `requires` stays.
Dam and canal stick costs, gate/lock costs and repair materials move into `cost` rows
instead of constants that name goods.

**`tree_species`** (extended): + `name`, **`archetype`** (`broadleaf` | `pine` | `spruce`), the
12 shape numbers and 8 seasonal colours now in `entities_render/trees.rs:28,88`, and feature
flags for the per-species code branches (**`bark_dashes`**, **`bark_eyes`**, **`weeping`**,
**`keeps_leaves`**).

**`plant_species`** (extended): + `name`, **`model`** (registry id) + **`model_colour`**,
**`model_height`**, **`flower_white`**.

## Phases

Each phase is one commit on master, removes one enum **completely from both crates**, and
leaves the game playable and the tests green. Each phase also adds a test that loads a
fixture database with **one extra row** of that type (a made-up good, job, building…) and
runs the sim for a season. That proves adding content needs no code.

### Phase 1: Loader, ids, saves
- `content.rs`, `OnceLock`, startup validation, the `IdVec` type.
- Save key table, `FORMAT` 6, fold the `Extras` chain, delete legacy variants.
- Move buildings/trees/plants from generated code to the loader **unchanged** (the enums
  still exist this phase; `SPECS[kind as usize]` becomes `content().building(id)`).
- *Done:* `build.rs` only emits tuning constants; tests succeed; a save round-trips.

### Phase 2: Goods
- `good` table; delete `Good` and `Good::ALL`; every `[T; 5]` becomes an `IdVec<GoodId, T>`.
- `accepts` → `building_store` rows. Both alerts pick "a building that stores g", which
  **fixes the two bugs above**.
- The legacy mirrors `colony.food` / `colony.wood` go.
- The game crate's `good_name`, `good_icon`, `good_color`, load and heap meshes and the
  "Waiting for X" tables become row lookups plus one sentence template each.

### Phase 3: Resources
- `resource` + `resource_yield` tables; delete `ResourceKind`. `Role::Collect` takes a resource
  group id. The 5 behaviours keep their code in `forestry.rs`, `wetland.rs` and `resources.rs`.
- Game: icon, name, lens colour, pose, deposit mesh and transform all come from the row.

### Phase 4: Jobs
- One `job` table; delete `colony::Job` and `RoleKind`. `Role` becomes the `Duty` code enum,
  and the dead `Assignment::Slot` goes. Rename the private gather `Job` struct in `agents.rs`
  so the word means one thing.
- Skills, experience and deed days become `IdVec<JobId, _>`. Epithets are keyed by the job `key`.
- Game: `job_icon`, `job_name`, `role_presentation`, `role_color` and the "No work for X" text
  become one row lookup.

### Phase 5: Buildings
- Delete `BuildingKind` and the game's duplicate `StructKind`. Code branches on `behaviour`,
  `stores`, `unlocks` and the maintenance columns.
- Game: a model registry (`HashMap<&str, fn(&mut Mb, f32)>`) plus `footing` replaces
  `model_kind`, `struct_mesh` and `footing_of`. The build menu and hotkeys are read from
  `menu_order` / `hotkey`, which replaces the two hand-written lists
  (`hud/panels.rs:620`, `hud/input.rs:74`). The inspect panel is assembled from what the
  building has (family, stores, balance toggle, seats), not from a `match` on kind. `Tool::Dam`
  and `Tool::Canal` become `Tool::Build(id)` with placement chosen by behaviour.
- Fallback kinds (`map_or(BuildingKind::Canal, …)` in `agents.rs`) become `Option`.

### Phase 6: Trees and plants
- Delete sim `Tree` and `Plant` and game `Species`. Tree meshes come from
  `archetype` + shape numbers + flags, and plant meshes from the `model` registry id + its
  parameters.
- Model caches are indexed by id, not by `Species::index()` or `Plant as usize`.

### Phase 7: Sentry and clean-up
- Add a test that reads every content `key` and `name` from the database and fails if any of
  them appears as a string literal in `crates/*/src`. A deleted enum can't come back
  (it won't compile), and this test stops names creeping back in as strings.
- Remove the now-unused content icons (`FellingCamp`, `Logs`). Icons become a registry keyed
  by id like models.
- Rewrite `crates/sim/content/README.md` to describe the tables and the rules above.

## How we'll know it worked

- Rename "Forager" to "Gatherer": **1 cell, 0 lines of code.**
- Add a "Mud store" that holds mud and sticks with the stick pile's model:
  **one `building` row plus its `cost`, `slot` and `building_store` rows, 0 lines of code.**
- Add a new tree species that uses the broadleaf archetype: **rows only.**
- No `match` anywhere on a content item: `grep` for the deleted enum names returns nothing,
  and the Phase 7 sentry test succeeds.

## Open decision

**Saves.** Phase 1 changes the save format once, and saves made before it won't load. The
alternative is a one-off converter from format 5, about 300 lines that read the old index
order and write keys. It would only matter if there are saves worth keeping.
