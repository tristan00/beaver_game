# Gameplay data

`game.sqlite3` is the source for gameplay constants and all 20 building
definitions. `build.rs` reads and validates the database, then generates Rust
declarations in Cargo's `OUT_DIR`. Database edits require a rebuild; no database
queries run during simulation steps.

| Table | Contents |
| --- | --- |
| `gameplay_constant` | Named scalar gameplay constants, grouped by simulation module |
| `constant_reference` | Derived constants and their dependencies |
| `constant_array`, `constant_range` | Ordered values and inclusive ranges |
| `building`, `cost`, `prerequisite`, `slot` | Building footprints, terrain requirements, costs, labour, dependencies, staffing and capacity |
| `recipe` | Equipment material use and upper-dam construction costs |
| `starting_good` | Initial material supplies |
| `role_policy` | Default staffing priorities and weights |
| `name_pool` | Beaver and family names |

For example, change barge capacity using SQLite:

```sql
UPDATE gameplay_constant SET value = 48
WHERE module = 'shipping' AND name = 'BARGE_CAPACITY';
```

Then run `cargo build --offline --release -p game` from the repository root.
Keep cost and recipe `position` values in their intended delivery order.

The migration preserves the existing values. HUD, renderer, terrain generation,
and water-engine definitions remain in their existing code. Implementation
constants such as numeric tolerances, identifiers, enum membership, precision
scales, simulation timesteps and test fixtures also remain in Rust.

Validation: `cargo test --offline -p sim --lib` and
`cargo check --offline --workspace --all-targets`.
