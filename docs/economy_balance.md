# Food, sticks and clay

The earlier zero-death validations below are survival diagnostics, not balance
approval. Good planning must still leave meaningful mortality and the need to
replace workers, recover reserves and sustain families after losses. The current
pass tests those outcomes alongside scarcity and effortless abundance.

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

Even the best planning still involves deaths over sustained play. Preparation
should reduce exposure and let a colony absorb losses; it must not make a colony
effectively immortal. Do not impose scripted death quotas. A particular short
run can be lucky; evaluate exposure, mortality and recovery over longer runs.

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

On Algonquin, the construction-first policy spent 659 of 720 days short of queued
clay and lost three beavers. An economy-first opening (burrow, forager hut,
logging camp, clay pit, then larger construction) used identical supplies and
finished with no deaths or hunger, 252 clay-delay days and no easy days. Build
order matters; adding clay to the failing run would conceal that decision.

The first five-year Telemark run is also retained as a failed diagnostic: 16
starvation deaths, despite a surviving colony. Inspection found that the test
caretaker immediately unlocked its emergency area-gathering assignments, which
cancelled those orders before workers could act. The caretaker now preserves
its own emergency food orders until a 30-day food buffer returns. Cleanup leaves
pre-existing orders and later player overrides intact. This changes decisions,
not resource supply. Repeating the long run after that correction alone produced
the same 16 deaths, so it is explicitly not counted as a successful balance fix.

A second issue was in the food alert itself: it could offer workers to a hut
whose entire work area was exhausted. The alert now checks actual harvestable
resources, and can recommend emergency gathering in the wider area instead of
staffing an empty hut. The staffing controller uses the same availability check.
The regression test covers empty local supply, food outside the hut's area,
and a productive local hut.

The low-weight staffing experiment did not produce a crisis, and raising its
weights did not outperform leaving them alone. It is not evidence of successful
crisis recovery.

## Previous validation and winter failure

With the food-response fixes, the five-year weekly-care Telemark run ended with
66 beavers, but still had **6 starvation deaths, 38 hunger days and 43 food-stockout
days**. It fails the winter-balance check. The previous 16 deaths falling to six
is an improvement in the response, not a passing balance result. There were 29
easy days, 60 clay-delay days and 212 days with almost no stored clay. Workers
collected 25.47 river clay; the map regenerated 40.47 across its limited spots.

The final two-year Algonquin economy-first run had no deaths or hunger, 249
clay-delay days and no easy days. The overbuilding control retained 90 clay-delay
days. No supplies or yields were increased to rescue these policies. The
remaining late-winter failure needs investigation across additional management
policies and maps before calling the economy balanced.

Verification: 21 simulation unit tests pass; the release game builds; native
captures verify the three-goods display and a harvested river-clay spot with its
recovery status. The workspace report retains all comparison CSVs, command logs,
failed runs and screenshots in `tmp/food-clay-balance/report.html`.

## Winter diagnosis and response changes

The remaining deaths were reproduced from the exact day-1350 state, with the
same future construction orders. At day 1400 the original colony had only 14
days of food, but 96% of its workers were free. The weekly response sent two
workers each time, taking weeks to deploy the available workforce.

The matched day-1350 to day-1530 branches produced six starvation deaths under
the original responses, fifteen when food-area orders were ignored, and none
with daily responses or larger weekly responses. This demonstrates an avoidable
response delay, rather than proving that the map needed more food. It does not
establish full balance: the daily and larger-response branches still had 29 and
41 hunger days respectively, especially after the next births. The branches
used normal commands and preserved failed or refused commands in their logs.

Food warnings now offer a quarter of the working population for emergency
gathering (at least two where available), capped by eligible workers. Kits and
locked player orders are excluded. This is an explicit player action, and it
takes workers away from other tasks; it does not create food or raise output.
The tooltip states how many workers will be sent.

The food forecast also accounts for proportional spoilage. It estimates time
to exhaustion under current appetite and gathering rates, rather than assuming
stored food keeps indefinitely. The warning, its breakdown and automatic food
priority use the same estimate. This remains a current-rate estimate, not a
prediction of future seasonal harvests or births.

The first continuation validation compared the unchanged expansion policy with a cautious growth
policy that defers additional household housing and immigration below a 60-day
food reserve. The first permanent lodge is still allowed. Algonquin also retains
its economy-first opening. Overbuilding and neglect remain controls; no resource
parameters are changed in this continuation.

## Autumn preparation

The scaled response and spoilage forecast resolved the Telemark regression:
1,800 days, 68 beavers, no deaths or hunger, one food-stockout day, 65 clay-delay
days and 29 easy days. The slower-growth Telemark policy also survived without
hunger, but grew to 51 rather than 68. These are different management costs,
not a reason to increase starting stock.

The longer Algonquin check exposed a separate spring failure: 20 starvation
deaths after new litters arrived. A second matched experiment copied both the
colony and caretaker at day 1260 and continued through day 1890. Ordinary care
reproduced the 20 deaths. Earlier autumn gathering alone reduced that to four.
Earlier gathering plus three paid food caches produced no deaths or hunger,
and four easy days in the 630-day comparison window. The extra caches cost 36
sticks, 15 clay and 10.5 construction work, plus hauling and maintenance. The
resource supply, harvesting rate and starting stock were identical.

The game now exposes this choice during autumn with **Prepare food for winter**.
It shows stored food against a 90-day appetite reserve goal, offers a food cache
when capacity is insufficient, and offers food-gathering assignments alongside
construction. This goal is before spoilage and winter foraging; it is a planning
target, not a promise that gathering can stop. Immediate food emergencies take
priority over the seasonal warning. The test caretaker retains its own autumn
and winter gathering orders until the seasonal goal or practical storage limit
is reached, rather than releasing them after only 30 days of food.

Simulation and caretaker snapshots now support matched-state experiments
without rerunning independent openings or replaying invalid building IDs after
different construction choices. The diagnostic runners remain outside the
repository; their baselines reproduced the original failures before comparisons
were accepted.

## Completed continuation checks

No resource yield, starting stock, harvesting rate or river recovery parameter
was increased in this continuation. The changes address forecasting and useful
player actions, and the successful policies pay for their storage and work.

| Full run to day 1890 | Final population | Deaths | Hunger days | Clay-delay days | Clay-stockout days | Food buffer below 10 days | Easy days |
|---|---:|---:|---:|---:|---:|---:|---:|
| Telemark, weekly care and expansion | 97 | 0 | 0 | 63 | 237 | 99 | 59 |
| Algonquin, economy-first opening and food-aware growth | 65 | 0 | 0 | 292 | 251 | 187 | 0 |

Both horizons include the spring after the fifth winter. The two-year
overbuilding control had 105 clay-delay days; neglect still collapsed on day
590. The rejected initial managed pilot had 216 easy days in only 720 days.
The final report also retains a broader 60-day food-buffer/low-activity check,
so a result just below the original 90-day cutoff is not silently accepted.

The release build succeeds and 24 module-level tests pass. A native capture
confirms that the autumn warning appears while food income is still positive
and exposes the gathering action. All experimental data, commands and screenshots
remain in `tmp/food-clay-balance/report.html`; only the fast tests are committed.
The evidence covers these maps and policies, rather than asserting universal
balance for every seed or indefinitely growing colony.

## Rejected accident experiment

The work-accident experiment in commit 0f710c9 was rejected by the user.
All random work injuries, including the older minor accidents, are removed.
Its runs remain in the workspace at tmp/balance-outcomes as historical evidence,
but are not evidence for the current economy. No replacement mortality mechanic
is authorized: balance uses the existing food, storage, labor, shelter and
family systems.
