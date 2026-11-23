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
