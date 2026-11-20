# Food, sticks and clay

The only inventory goods are **Food, Sticks and Clay**. Food comes from wetland
plants and stripped bark. Felled trunks and branches provide sticks. Clay comes
from finite deposits and a few slowly replenishing river-bank spots. There is no
pebble stock, quarry, quarrier job or stone prerequisite. Construction,
craftsmanship, repairs and clay reinforcement consume sticks and clay.

River clay supplements deposits; it cannot sustain rapid expansion by itself.
Maps seed up to eight widely spaced spots outside the finite clay beds. Each
starts with 2 clay and holds at most 8. A spot recovers 0.04 clay per game day
while adjacent water is flowing and substantially unfrozen. Recovery stops when
the reach dries, freezes or becomes still. Exhausted spots remain discoverable;
diggers wait for at least 0.5 clay before making another collection trip.
Finite clay cells hold 30 units, reduced from the previous blanket increase to 60.

The starting stock is a 45-day food larder, 100 sticks and 30 clay, subject to
normal storage capacity. Food spoils at 0.3% per day. The base storage capacity is
100 per good. Foragers collect 1.4 food per working day before worker modifiers;
this was reduced from 2.8 after the first food/sticks/clay pilots proved too easy.

## Balance acceptance

Survival alone is not a passing result. Sustained abundance without consequential
resource or labour decisions is a balance failure, just as unavoidable collapse
under competent play is. Do not give a failed run additional resources merely to
make it survive.

- Compare the same map, starting stock and command rules before and after a
  change. Preserve failed runs as evidence.
- Record food buffers, hunger, deaths, material delays, resource depletion,
  worker activity, building progress and recovery costs. Separate population
  growth from sustainability.
- Diagnose placement, access, hauling, staffing and broken behavior before
  changing resource supply. Prefer one balance lever per experiment.
- Compare routine care, infrequent care, overbuilding, neglect and worker
  reassignment. A recovery must use normal player commands, paid work and
  resources earned in the simulation.
- Explicitly flag extended easy periods as well as shortages. The current
  diagnostic counts days after day 180 with at least eight beavers, more than
  90 days of food, no queued material deficit, and under 40% worker activity.
  Inspect the daily trace too: a winter lull is different from effortless
  expansion, and moving just below a threshold is not proof of balance.
- Use short pilots first, then a longer run and a second map. Report remaining
  uncertainty; one surviving seed does not establish universal balance.

Integration experiments and command replays live in the workspace's
`tmp/food-clay-balance` folder, outside the repository. Only fast module-level
tests are committed.

## First controlled comparison

The initial two-year Telemark managed pilot survived but had 216 easy days and
52.3% worker activity. It was rejected. After reducing food harvesting speed,
the same policy had 29 easy days, 67.8% worker activity, no hunger, and 41 days
where queued clay demand exceeded stock. Overbuilding increased clay-delay days
to 90 and worker activity to 76.1%; neglect collapsed on day 590. Neither the
starting supplies nor map yields were increased between those two pilot sets.

These are diagnostic results, not a claim that every map and strategy is balanced.
