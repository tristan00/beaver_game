# Beaver Colony industries and the forestry plan

*How resources, jobs, buildings and research fit together today, and a plan for a forest planting and upkeep industry. 18 Nov 2026, branch `interface-v2`.*

## Update: the water network, deposits and growing dams

What changed after this plan was written, and what now holds:

- **Water transport is core.** Docks are stores. A barge runs a standing route between two docks: it loads the chosen goods at one, sails to the other over water deep enough to float it, unloads, and returns. A barge sails 18 cells a day with 40 goods aboard. A beaver walks 40 cells a day, half that with a load of 3, and swims faster than it walks. Barges are slower but carry far more. Ice stops barges in winter, and shallow reaches stop them in any season.
- **Locks are how barges pass dams.** A dam stops a barge. A lock built on a dam lets it through, up or down, at the cost of about ten cells' sailing time; the dam keeps holding its water. A route refused for a dam names the cell to put the lock on.
- **Stone and clay lie in deposits.** Each map has three stone deposits and four clay beds, kept far apart. Other banks hold a little stray mud, enough for a family's first buildings. A growing colony quarries far from home and has to haul the stone and clay back.
- **Goods stay in the store they were put in.** Every store keeps its own stock. Builders fetch from a store that holds the good, and workshops craft from stores within 12 cells. Distance, hauling and barges therefore matter.
- **Dams grow.** Ordering a dam over existing dam cells raises, extends or repairs it. Only the height it adds costs, and every unit above the first needs mud. A dam can be built to a chosen crest level, up to 3 above the ground. The preview finds where rising water would walk round the dam's ends (the spill point) and offers to extend the dam over it. The Elevation view (F5) shows height contours for planning water levels.
- **Water comes from the catchment.** Rain soaks into the ground and comes back to the water network in proportion to the land that drains to it: a stream head gets the water of all the ground above it, every river cell and river-fed lake gets the water of the dry ground draining straight into it, and a river entering across the map border is fed at the cell just inside the border with the water of all the land it drains beyond the map (on algonquin the western river carries 5,545 cells of catchment, more than any stream on the map). Each catchment cell gives 0.0025 a day, so a river carries water in proportion to its catchment and the start rivers run about as deep as before. Rivers leave the map where they flow off its edge; evaporation and seepage take the rest. Nothing else adds water: the start pond fills from its own river, and the designed dam is put where the river brings more than the full pond loses.
- **Dams hold water as they rise.** Builders build as soon as some materials are on site and go for more when the work catches up with them, so a dam grows in step with the work done and its pond starts backing up while it is built. A cancelled dam keeps what was built.
- **Ponds feed the colony.** Food plants sprout on shores and in shallow water, regrow only while the water is near, and drown in deep water. Damming a stream is how a colony grows its food.
- **No elders.** Every tech now comes from colony size, branch experience and earlier research. Canals, docks, barges and locks open within the first year or so.
- **Buildings have prerequisites:**
  - a lodge needs a burrow
  - a workshop, dock, quarry camp or storehouse needs a warehouse
  - a food cache needs a forager hut
  - a water wheel needs a workshop
  - a lock needs a dock
  - a mill needs a workshop and a water wheel
  - a tree nursery needs a forester's lodge
  - a guard post needs a scent mound
- **Trees grow over years:** young at 1 year, mature at 3, old at 15 (360-day years).
- **A beaver colony lives on the water.** Water is how the colony grows, hauls and (once predators come) stays safe, so nearly every building stands at the water's edge: at least one footprint cell in water.
  - At the water: lodge, burrow, food cache, scent mound, forager hut, mud pit, logging camp, workshop, warehouse, storehouse, mill, plus the water works (dam, canal, lock, dock, water wheel, barge).
  - On dry ground: quarry camp (stone lies inland), forester's lodge and tree nursery (they tend the forest), guard post (it watches the land approaches).
  - A finished water building whose water dries up stands **high and dry** until the water comes back. Water still lying in its footprint or the ring of cells around it keeps it working; the waterline may waver that much without effect.
    - Its workplace slots let their workers go and take none.
    - A store takes no new goods and adds no room, but what it holds can still be fetched or hauled out.
    - A lodge or burrow loses 0.03 condition a day on top of its usual wear, so the family grows cold there.
  - Draining a pond (a breached or lowered dam, a dry summer) can strand a whole quarter of the colony. Keeping the water up is the colony's first job.

## Part 1: the economy as it stands

### Goods

| Good | Comes from | Used for |
|---|---|---|
| Logs | felling trees (3 per tree) | almost every building; tools |
| Sticks | a felled tree's branches (4 per tree) | dams, buildings; rope and baskets |
| Food | plants (3 per plant, regrowing 0.3 a day on wet ground); bark of felled trees (2 per tree) | eating (0.25 a day per adult, less for the young) |
| Mud | mud banks (20 each) | lodges, stores, workshops, canals; rope |
| Stone | outcrops (30 each) | quarry-era buildings; tools |
| Rope | workshop: 1 stick + 0.5 mud → 1 rope | warehouse, water wheel, dock, barge, mill, storehouse |
| Baskets | workshop: 2 sticks → 1 basket | barge |
| Tools | workshop: 1 log + 1 stone → 1 tool | lock, water wheel, mill |

One ledger holds the colony's stock. Every store adds room: 100 base, then the warehouse +150, the food cache +100 (food only) and the storehouse +400.

### Resources in the world

| Resource | Where | Yield | Regrows? |
|---|---|---|---|
| Tree | every forest cell at the start | felled (2 work) into a felled tree: 3 logs, 4 sticks, 2 bark | **No.** A used-up tree is removed for good |
| Food plant | wet cells only | 3 food | Yes, 0.3 a day (full again after 10 days) |
| Mud bank | river banks | 20 mud | No |
| Stone outcrop | outcrops | 30 stone | No |

There is also an abstract forest-density map in the ecology system. It grows and spreads on its own (0.05% base, +1% from its own density and +1% from neighbours, per day), but felling never lowers it, and it never puts trees back on the map. So the colony's actual wood supply only ever shrinks, while the density map says the forest is fine.

### Jobs

Since this redesign, a beaver's job is the role of the work slot it fills. Free beavers do nothing hidden.

| Role | Works at | Does |
|---|---|---|
| Builder | any construction site (2 slots each) | fetches materials, adds labour (0.6 a day per builder) |
| Forager | forager hut (2) | gathers plants and bark in the hut's work area |
| Logger | logging camp (2) | fells trees and gathers logs and sticks in its area |
| Mud digger | mud pit (2) | digs mud banks in its area |
| Quarrier | quarry camp (2) | breaks stone in its area |
| Hauler | warehouse, food cache, dock (1); storehouse (2) | moves goods between stores on standing orders |
| Crafter | workshop (1), mill (2) | makes the chosen recipe |

- **Gathering camps:** each has a 9×9 work area that can be redrawn.
- **Auto-staffing:** fills empty slots by building priority, then role order (Builder, Forager, Logger, Forester, Hauler, Mud digger, Quarrier, Crafter). Food short: foragers first. A site waiting on a material the stores can't cover: that material's gatherers (or crafters) come before builders. Logs under 20: loggers come before builders, then distance.
- **Kits:** they don't work.

### Buildings

| Building | Size | Cost | Labour | Slots | Unlocked by |
|---|---|---|---|---|---|
| Burrow hideout | 1×1 | — | 6 | — | start |
| Dam | 3×1 per span | 15 logs, 20 sticks per 6 cells | 8 | — | start |
| Warehouse | 3×3 | 25 logs, 15 sticks, 10 mud, 2 rope | 10 | 1 hauler | start |
| Forager hut | 2×2 | 8 sticks, 4 mud | 2 | 2 foragers | Foraging |
| Logging camp | 2×2 | 4 logs, 6 sticks | 2 | 2 loggers | Felling |
| Mud pit | 2×2 | 3 logs, 4 sticks | 2 | 2 diggers | Felling |
| Food cache | 2×2 | 6 logs, 10 sticks, 6 mud | 4 | 1 hauler | Winter stores |
| Lodge | 3×3 | 20 logs, 25 sticks, 20 mud | 12 | — | Lodge building |
| Quarry camp | 2×2 | 5 logs, 6 sticks, 3 mud | 3 | 2 quarriers | Stonework |
| Workshop | 3×2 | 8 logs, 6 sticks, 4 mud | 6 | 1 crafter | Weaving |
| Canal | per cell | 2 mud | 1 | — | Canal digging |
| Dock | 2×1 | 10 logs, 6 sticks | 4 | 1 hauler (store, 60 per good) | Docks |
| Lock | 2×1 | 10 logs, 6 stone, 8 mud, 1 tool | 6 | — | Flow control |
| Water wheel | 2×2 | 12 logs, 2 rope, 1 tool | 7 | — | Flow control |
| Barge | 1×1 | 12 logs, 8 sticks, 2 rope | 5 | — | Barges |
| Storehouse | 4×3 | 40 logs, 20 sticks, 12 stone, 4 rope | 16 | 2 haulers | Grand storage |
| Mill | 3×3 | 30 logs, 10 stone, 4 rope, 3 tools | 14 | 2 crafters | Industrial milling |

The scent mound and guard post exist but are switched off.

### Research

Research comes three ways:
- **Achievements:** first burrow, first dam, first winter.
- **Branch experience:** work done in a branch's jobs, plus a minimum colony size.

| Tech | Branch | Needs | Unlocks |
|---|---|---|---|
| Foraging | Forest | first burrow | Forager hut |
| Felling | Forest | first dam | Logging camp, mud pit |
| Winter stores | Forest | first winter | Food cache |
| Lodge building | Building | Felling; 16 xp; 5 beavers | Lodge |
| Stonework | Forest | Felling; 40 xp; 6 beavers | Quarry camp |
| Weaving | Forest | Felling; 60 xp; 8 beavers | Workshop |
| Master builders | Building | Lodge building; 60 xp; 10 beavers | faster building |
| Fine tools | Craft | Weaving; 50 xp; 10 beavers | faster crafting |
| Dry stores | Forest | Stonework; 100 xp; 10 beavers | more storage |
| Canal digging | Water | Lodge building; 30 xp; 8 beavers | Canal |
| Docks | Building | Lodge building; 30 xp; 8 beavers | Dock |
| Flow control | Water | Docks, Stonework; 60 xp; 12 beavers | Lock, water wheel |
| Barges | Building | Docks, Weaving; 60 xp; 10 beavers | Barge |
| Grand storage | Building | Lodge building; 100 xp; 20 beavers | Storehouse |
| Industrial milling | Craft | Weaving, Flow control; 100 xp; 24 beavers | Mill |

### The chains

```
Forest ──fell──► Logs ─────────────────────► buildings, dams, tools
   │             Sticks ──► rope, baskets ──► stores, wheel, docks, barges
   │             Bark ────► Food
Wet ground ──► Plants ──► Food ──► eating, growth, strangers joining
Banks ──► Mud ──► lodges, stores, canals, rope
Outcrops ──► Stone ──► tools, quarry-era buildings
Rope + tools ──► water wheel ──► powers workshops ──► more rope, baskets, tools
Dams ──► ponds ──► water through the dry summer, wetlands (more plants), canals and boats
```

### Problems the chains have today

- **Wood is finite.** Every tree felled is gone for good, so logs and sticks, the base of nearly every chain, run down with no way back.
- **Rope is gated behind 8 beavers.** The warehouse needs rope, rope needs a workshop, and the workshop needs Weaving, which needs 8 beavers. A second store is impossible until the colony is fairly big; the founding bundle's 2 rope covers exactly one warehouse.
- **Mud is the early bottleneck.** The mud pit needs Felling, which needs the first dam (see the audit, G4).
- **The two forest models disagree:** the density map regrows by itself while the real trees don't.

## Part 2: the forestry industry

### Goals

1. Wood is the key limiting resource, and running out of it is a real threat.
2. Planting and caring for forest counters deforestation and is an investment: what you plant now is the wood you cut in a season or two.
3. Logging and forestry each work in **areas the player sets per building**, so the player decides where wood is cut and where it's grown.
4. The industry ties into water: wet, well-watered ground grows trees faster, so dams help forests as well as ponds.

### One forest model: trees that grow

Merge the two forest models. Tree resources become the truth, and each tree has an age:

| Stage | Age (days, untended) | Yield if felled | Notes |
|---|---|---|---|
| Sapling | 0–360 (1 year) | nothing (can't be felled) | trampled or flooded easily |
| Young | 360–1080 (1–3 years) | 1 log, 2 sticks | thinning gives sticks |
| Mature | 1080–5400 (3–15 years) | 3 logs, 4 sticks, 2 bark | seeds new saplings nearby |
| Old | 5400+ | 2 logs, 4 sticks | slowly dies to a snag (sticks only) |

- **Growth rate:**
  - Base 1 day of age per day.
  - ×1.5 on moist ground (within 3 cells of standing water).
  - ×0.6 on dry ground.
  - ×0 in winter.
  - ×1.5 when tended (see the forester).
  - Flooded saplings and young trees die; the ecology system already drowns forest.
- **Natural spread:** each mature tree has a small daily chance (about 0.3%) to seed an empty, dry, unshaded neighbouring cell with a sapling. Untouched forest slowly refills; a clear-cut does not come back on its own for a long time.
- **The ecology density map is derived from the trees** (share of cells holding a young-or-older tree), so felling finally shows up in wildlife, vegetation and the map views.
- **Starting forest:** mostly mature, with a mix of young and old, so the first logging is fine but a bare patch takes a year to recover.

### Buildings

**Logging camp (changed)**
- **Work area:** the 9×9 area becomes a player-set rectangle, up to 15×15, drawn with the existing area tool.
- **Cutting rule,** chosen in the inspector:
  - **Mature only** (default): fells mature and old trees.
  - **Clear-cut:** fells everything young-or-older.
  - **Thin:** fells only enough to keep at least half the cells forested, oldest first.
- **Inspector:** mature trees left in its area, wood per day, and **days until the area runs out** at the current rate.

**Forester's lodge (new)**
- **Size and cost:** 2×2; 4 logs, 8 sticks, 2 mud; 3 labour; 2 forester slots.
- **Work area:** player-set, up to 15×15, drawn the same way.
- **Plant:** puts a sapling on empty, dry, suitable cells in its area. Each planting costs half a day of work and no goods. Cells inside a building, road, pond or dam flood preview are skipped.
- **Tend:** visits young trees and saplings in its area. Tended trees grow 1.5× for 30 days. Clearing deadwood while tending gives a trickle of sticks, which keeps the job from being pure overhead early on.
- **Planting rule,** chosen in the inspector:
  - **Fill gaps** (default): plants only where a tree was.
  - **Plant all:** plants every suitable cell.
  - **Tend only.**
- **Inspector:** saplings, young and mature trees in its area; trees planted and tended per day; **logs ready in N days** (a forecast from ages and growth rate).

**Tree nursery (new, later)**
- **Size and cost:** 2×2; 6 logs, 6 sticks, 4 mud, 1 basket; 1 forester slot.
- **Saplings:** grows sapling stock at 1 a day, a new good the forester's lodge uses to plant at a quarter of the labour.
- **Head start:** a nursery sapling starts at age 30.
- Gives the industry a second step and a use for baskets.

### Jobs

- **New role: Forester** (icon: beaver with a seedling). It sits after Logger in the default role order.
- **Auto-staffing:** fills forester slots like any other.
- **Jobs board:** shows planted, tended and forecast wood per day.

### Research

| Tech | Branch | Needs | Unlocks |
|---|---|---|---|
| Forestry | Forest, tier 1 | Felling; 20 Forest xp; 5 beavers | Forester's lodge; the logging camp's cutting rules |
| Coppicing | Forest, tier 2 | Forestry; 50 xp; 8 beavers | Felled trees of a chosen camp regrow from the stump as young trees (sticks-rich), instead of leaving a gap |
| Tree nursery | Forest, tier 3 | Forestry, Weaving; 80 xp; 10 beavers | Tree nursery, sapling good |

Forestry should come early, right after Felling. In the first year a colony that fells around its camp will strip the nearby forest, and the game should offer the answer then.

### Areas: how the player manages where wood is cut and grown

- **Both areas are rectangles** set per building with the existing area tool (inspector → area button → drag on the map). Felling and planting are restricted to the building's area.
- **Overlaps are allowed.** The intended play is rotation: one logging camp cuts area A while the forester replants area B, then they swap. Overlaps show hatched in the work-area map view.
- **Forest map view (F4):**
  - Colours each tree by stage: saplings pale, young mid, mature deep, old brown.
  - Outlines every logging and forestry area in its role colour.
  - Opens automatically when you place either building or edit its area.
  - Legend headline: "Mature trees 412 · ready in 60 d: 95".
- **Alerts:**
  - "Logging area running out" when a camp's area has under 10 mature trees. The fix moves the area or places a forester's lodge.
  - "Forest falling" when the colony's mature trees drop by more than 5% in a season. The fix opens the forest view.

### Balance target

- A two-logger camp fells about 1 tree every 2 days (about 0.5 logs and 0.8 sticks a day measured in the audit).
- One forester planting and tending should replace about 1 tree a day.
- One forester's lodge per logging camp therefore keeps an area in balance, with the first planted trees maturing about two seasons later.
- The first year then reads as:
  - cut near camp
  - unlock Forestry
  - start replanting
  - feel the gap through winter
  - harvest your own trees in year two

### Implementation plan

**Sim**
1. **Tree age.** In `resources.rs`, add `age_days` and a `stage()` to trees; `step` ages trees by the growth rules and kills flooded saplings.
2. **Spread and density.** Natural spread from mature trees, seeded and deterministic. Replace the ecology forest-density growth with a value derived from the trees, so felling lowers it (`ecology.rs`).
3. **Yield by stage.** Felling yields by stage (`fell` / `strip`).
4. **New roles and buildings.** `Role::Plant`, `RoleKind::Forester`, `BuildingKind::ForesterLodge` and `TreeNursery`, with catalogue entries, a work area like a gathering camp, and costs in `tuning.rs`.
5. **Forester tasks** (`agents.rs`): Plant (walk to an empty suitable cell in the area, work, add a sapling) and Tend (walk to the youngest untended tree, work, mark it tended until a day).
6. **Rules as commands.** `Command::SetCutRule { building, rule }` and `Command::SetPlantRule { building, rule }`.
7. **Research.** Forestry, Coppicing and Tree nursery techs in `tech.rs`.
8. **Read-outs and alerts.** `hud::forest_report(area)` (stage counts, forecast) and the two alerts with fixes.
9. **Unit tests:**
   - trees age and stop in winter
   - floods kill saplings
   - a forester raises the sapling count in its area only
   - "mature only" never fells young trees
   - the density map falls with felling
   - a camp plus a lodge keep an area's mature count stable over two years

**Game**
1. **Icons:** forester's lodge, nursery, forester role, sapling and tree stages.
2. **Inspectors:**
   - logging camp: cut rule picker, days-to-empty
   - forester's lodge: plant rule picker, stage counts, "logs ready in N d"
   - both: the area button already exists
3. **Forest map view** with stage colours, area outlines and a legend; opens with the tools.
4. **Alerts and their fixes** in the right column; a Jobs board row for foresters.
5. **Text budget:** the new words ("Mature only", "Clear-cut", "Thin", "Fill gaps", "Plant all", "Tend only") are one or two words each and go into Appendix A.

**Scripted check**
- Add a "forester" style to `sim::autoplay`: it places a forester's lodge beside each logging camp.
- Add a two-year run to `examples/first_year.rs` that reports mature trees per season with and without forestry.
