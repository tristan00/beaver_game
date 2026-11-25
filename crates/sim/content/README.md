# Game content

The `.toml` files here are the game's content: every good, kind of resource, job, building, tree
and food plant is a row, so a change to the content shows in `git diff` like any other change.
The game reads them when it starts (`sim::content`): an edit needs a restart, not a rebuild. Set
`BEAVER_CONTENT` to another folder to load other content. The tuning numbers are in
`../tuning.toml` and are compiled in by `build.rs` (an edit there needs a rebuild).

| File | Contents |
| --- | --- |
| `goods.toml` | What a new colony starts with, and goods: load, pace, floats, edible, carries a tree species, looks |
| `buildings.toml` | Gate and lock materials, and buildings: footprint, cost, seats (jobs), stores, refits, how each is built, home, rafts, wear, looks, hotkey, menu place |
| `resources.toml` | Gather groups, the kinds the world's making puts down, and kinds of resource: behaviour, yields, the job that gathers it, rate, tools, looks |
| `jobs.toml` | Jobs: duty, names, skill, colony panel place, shore limit, icon, path colour |
| `trees.toml`, `plants.toml` | Species: growth and yield (`spec`) and drawing (`look`, colours as `0xrrggbb`) |
| `flora.toml` | Each map's tree mix and food plants (`default` for maps without their own) |
| `names.toml` | Word pools for names |

## Rules

1. **Code never names a content item.** It asks a row about its traits: what a building stores,
   which job gathers a resource, which good is edible.
2. **If code needs to tell items apart, add a column**, not a `match` on an item.
3. **A few mechanics need their own code**, and a row picks one: a resource's `behaviour` (tree,
   felled tree, food plant, deposit), a job's `duty`, a building's `built_as` (site, dam, canal).
4. **`key` is stable and goes in saves; `name` is shown and can change freely.** Rows name other
   rows by key (`cost = [["Sticks", 4.0]]`, `role = "Feller"`).
5. **Looks are rows too.** Icons, colours and models are named by id; the game keeps a drawing
   routine for each model id.

Every file in the folder is read and added together, so a mod can add rows in a file of its own.
The loader rejects unknown fields, duplicate keys and rows naming missing rows, with the file and
line.
