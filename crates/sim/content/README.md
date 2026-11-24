# Gameplay data

`game.sqlite3` is the source for gameplay constants and all building
definitions. `build.rs` reads and validates the database, then generates Rust
declarations in Cargo's `OUT_DIR`. Database edits require a rebuild; no database
queries run during simulation steps.

| Table | Contents |
| --- | --- |
| `gameplay_constant` | Named scalar gameplay constants, grouped by simulation module |
| `constant_reference` | Derived constants and their dependencies |
| `constant_array`, `constant_range` | Ordered values and inclusive ranges |
| `building`, `cost`, `slot` | Building footprints, terrain requirements, costs, labour, staffing and capacity |
| `starting_good` | Initial material supplies |
| `role_policy` | Order of the job rows in the colony panel |
| `name_pool` | Beaver and family names |
| `tree_species` | Each tree's growth, yield and flood tolerance. `growth` is set so a tree on ordinary ground turns mature (full logs, starts seeding) at half its real seed-bearing age, per the USDA *Silvics of North America*: aspen and willow 10 years, birch 15, alder 4 (red alder), pine 20 (eastern white pine), spruce 30, maple and beech 40. A tree ages `growth` days a day outside winter; mature is 1080 days. |

| `plant_species` | Each food plant beavers eat: its `habitat` (`Upland` moist open ground, `Meadow` wet ground, `Shore` the waterline out to `max_depth`, `Water` between `min_depth` and `max_depth`), the most `food` it holds, and per day in full growing season how much it `regrow`s, the chance it `creep`s by its roots into a neighbouring cell and the chance its `seed` takes root up to `SEED_REACH` cells off. `keep` is the share of its food it holds through winter in roots, rhizomes or canes (cattail, arrowhead and lily rhizomes, raspberry canes: what beavers dig for in winter). |
| `plant_flora` | How common each food plant is on each map, from the map region's real wetlands: Joe-Pye weed only in North America, meadowsweet only in Europe, little cattail or arrowhead in Norway. Every map and `default` need one, with a plant for every habitat. |

For example, change how many sticks a gate needs using SQLite:

```sql
UPDATE gameplay_constant SET value = 8
WHERE module = 'canals' AND name = 'GATE_STICKS';
```

Then run `cargo build --offline --release -p game` from the repository root.
Keep cost `position` values in their intended delivery order.

HUD, renderer, terrain generation,
and water-engine definitions remain in their existing code. Implementation
constants such as numeric tolerances, identifiers, enum membership, precision
scales, simulation timesteps and test fixtures also remain in Rust.

Validation: `cargo test --offline -p sim --lib` and
`cargo check --offline --workspace --all-targets`.
