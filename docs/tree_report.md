# Trees

Every number here comes from `crates/sim/content/game.sqlite3` (`tree_species`, `tree_flora`, `gameplay_constant`) and from the worlds the game generates. Species counts are a census of each map as generated, before founding.

## The eight species

| Species | Kind | Food (bark) | Where it grows best |
|---|---|---|---|
| Aspen | Broadleaf | Best (2× bark) | Dry and open ground |
| Willow | Broadleaf | Very good (1.75×) | Wet banks, wetland |
| Birch | Broadleaf | Fair (0.75×) | Anywhere, open ground |
| Alder | Broadleaf | Poor (0.5×) | Wet banks |
| Maple | Broadleaf | Good (1×) | Dry closed forest |
| Beech | Broadleaf | Very poor (0.25×) | Dry closed forest |
| Pine | Conifer | None | Dry ground and ridges |
| Spruce | Conifer | None | Wet ground and slopes |

Beavers eat bark. Conifers give sticks only.

## How common each species is

Share of all trees on the map at the start.

| Species | Algonquin (Ontario) | Voyageurs (Minnesota) | Bavarian Forest | Telemark (Norway) | Generated maps |
|---|---|---|---|---|---|
| Aspen | 14.3% | **35.1%** | 2.7% | 7.1% | 20.4% |
| Willow | 3.4% | 2.3% | 2.3% | 1.9% | 4.1% |
| Birch | 11.9% | 18.7% | 4.1% | 20.9% | 24.2% |
| Alder | 4.6% | 3.7% | 3.3% | 3.1% | 10.7% |
| Pine | 18.4% | 27.4% | 4.1% | **36.5%** | **40.6%** |
| Maple | **32.1%** | 5.7% | 9.5% | — | — |
| Spruce | 5.5% | 7.1% | **41.9%** | 30.6% | — |
| Beech | 9.8% | — | 32.2% | — | — |
| **Trees** | 13,277 | 12,999 | 12,774 | 12,360 | ~770 (128×128 map) |

- **Voyageurs** is the beaver-friendly map: aspen and birch make up more than half the forest.
- **Bavarian Forest** is hard: three quarters of the forest is spruce and beech, so almost no food.
- **Telemark** is two-thirds conifer.
- Every map is about 5% tree cover on its 512 × 512 m area.
- Starting ages on every map: 15% young, 75% mature, 10% old, no saplings.

### What decides the species on a cell

Each map gives every species four weights: **dry**, **wet**, **upland** and **open**. The generator blends them:
- Moist ground moves the odds from the dry weight towards the wet weight. ESA wetland cover counts as at least 85% wet.
- Steep or high ground adds the upland weight.
- Shrubland, grassland and other non-forest cover adds the open weight.

Wet banks therefore lean to willow, alder and spruce, and ridges lean to pine and spruce, but every habitat keeps a mix. No tree is placed on river or lake cells, within 6 m of them, or in water deeper than its species stands (see below).

## Growth

Trees age only outside winter: 270 growing days a year. Stages:

| Stage | Age (days of growth) |
|---|---|
| Sapling | 0–360 |
| Young | 360–1,080 |
| Mature | 1,080–5,400 |
| Old | 5,400+ |

The species growth rate multiplies the speed, and so does the ground:
- ×1.4 within 15 m of water.
- ×0.7 on dry ground (moisture below 0.32).
- ×1.8 extra while a tree is tended.

Years from seed to each stage on ordinary ground, with the mature figure near water and on dry ground:

| Species | Growth | Young | Mature | Old | Mature near water | Mature on dry ground |
|---|---|---|---|---|---|---|
| Willow | ×2.0 | 0.7 y | 2.0 y | 10.0 y | 1.4 y | 2.9 y |
| Aspen | ×1.5 | 0.9 y | 2.7 y | 13.3 y | 1.9 y | 3.8 y |
| Birch | ×1.3 | 1.0 y | 3.1 y | 15.4 y | 2.2 y | 4.4 y |
| Alder | ×1.2 | 1.1 y | 3.3 y | 16.7 y | 2.4 y | 4.8 y |
| Maple | ×0.9 | 1.5 y | 4.4 y | 22.2 y | 3.2 y | 6.3 y |
| Pine | ×0.75 | 1.8 y | 5.3 y | 26.7 y | 3.8 y | 7.6 y |
| Spruce | ×0.65 | 2.1 y | 6.2 y | 30.8 y | 4.4 y | 8.8 y |
| Beech | ×0.55 | 2.4 y | 7.3 y | 36.4 y | 5.2 y | 10.4 y |

**Spreading.** On each growing day, a mature or old tree has a 0.0055% chance of seeding a neighbouring cell. The cell must be:
- free of trees and buildings, and not the supply pile;
- dry and never flooded;
- outside a pond a dam is about to make;
- a cell the world generator gave a tree.

Stumps regrow only under coppicing, which is sandbox-only for now.

## Yields

Felling gives logs and branches (both become **sticks**) and bark (**food**). Saplings cannot be felled.

| Species | Young: sticks / food | Mature: sticks / food | Old: sticks / food | Dead tree: sticks | Felling (beaver-days) |
|---|---|---|---|---|---|
| Aspen | 3 / 2 | 7 / 4 | 6 / 2 | 3 | 1.2 |
| Willow | 3.35 / 1.75 | 7.05 / 3.5 | 6.7 / 1.75 | 4.5 | 0.8 |
| Birch | 2.9 / 0.75 | 6.7 / 1.5 | 5.8 / 0.75 | 3 | 1.4 |
| Alder | 2.8 / 0.5 | 6.4 / 1 | 5.6 / 0.5 | 3 | 1.2 |
| Maple | 3.2 / 1 | 7.6 / 2 | 6.4 / 1 | 3 | 2.0 |
| Pine | 2.85 / 0 | 7.05 / 0 | 5.7 / 0 | 2.25 | 1.8 |
| Spruce | 2.9 / 0 | 7.2 / 0 | 5.8 / 0 | 2.25 | 1.8 |
| Beech | 3.6 / 0.25 | 8.8 / 0.5 | 7.2 / 0.25 | 3 | 2.5 |

Felling days assume one beaver at full effort (1.3 work a day). Mood, health, rest and felling skill change it.

Old trees give fewer sticks and less food than mature ones. Mature is the stage to cut, and that is the default felling rule.

## Survival

### Water

Each species stands water up to a set depth for good. Deeper water drowns it after enough days flooded. A cell counts as flooded while water stands over 5 cm; the count drains at twice the rate once the cell is dry.

| Species | Stands water up to | Sapling drowns after | Young drowns after | Mature or old drowns after |
|---|---|---|---|---|
| Willow | 0.6 m | 15 days | 60 days | 90 days |
| Alder | 0.5 m | 12.5 days | 50 days | 75 days |
| Birch | 0.2 m | 5 days | 20 days | 30 days |
| Aspen | 0.15 m | 5 days | 20 days | 30 days |
| Maple | 0.15 m | 4 days | 16 days | 24 days |
| Spruce | 0.15 m | 3 days | 12 days | 18 days |
| Pine | 0.1 m | 3 days | 12 days | 18 days |
| Beech | 0.1 m | 2.5 days | 10 days | 15 days |

- A drowned sapling or young tree is gone.
- A drowned mature or old tree dies standing (a snag), then falls after **10 days in water**, or **2 years** on dry ground.

So a new dam pond kills its conifers and beech within a few weeks and its aspen and birch within a month, and the dead trunks clear about ten days later. Willow and alder survive in its shallow edges.

### Age

Old trees die at a 0.05% chance a day, about 5½ years on average after turning old, and stand as dead trees until they fall.

### Winter

No growth, no spreading. Drowning and rotting continue.

## Changes made with this report

- Each species now has its own water depth limit; before, every species drowned at 15 cm.
- Mature trees drown after 30 flooded days × the species' flood tolerance; it was 60.
- Dead trees standing in water fall after 10 days; before, they stood in the pond for 2 years.
- New maps start with no tree in water deeper than its species stands. That removed about 100 trees from Algonquin's river edges.
- Clicking a tree, a felled trunk, food plants or mud now opens an inspector card:
  - species, with condition pips while the tree is drowning;
  - days to its next stage;
  - what felling it now gives;
  - the water depth here against what the species stands.
