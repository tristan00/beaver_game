# Progression: founding, research, colony growth, time

Contract for the implementation tasks in section 9. The colony grows only through what the player
does and what the family earns: the player places every building, orders every job, and answers
every arrival. Nothing is built, worked or born by itself except spring litters under the
conditions in section 5. Current code breaks this in four places; section 8 lists them with
`file:line`.

Numbers marked *(tune)* are starting values; the walk-through in section 7 uses them.

## 1. Time (`sim::calendar`, new)

| | Value | Where it lives |
|---|---|---|
| Year | 360 days, four seasons of 90 days: spring 0-89, summer 90-179, autumn 180-269, winter 270-359 | `calendar::YEAR_DAYS`, `SEASON_DAYS`, `calendar::season_of(day)` |
| Real time at 1x | **2.5 minutes per season** (150 s), 10 minutes per year; so **0.6 days per real second** | `game::sim_driver::SimClock::advance` adds `real_dt x BASE_DAYS_PER_SECOND x Speed::multiplier` days; `BASE_DAYS_PER_SECOND` = `SEASON_DAYS / SEASON_SECONDS` (today 0.4, `sim_driver.rs:11`) |
| Speed steps | Pause, 1x, 3x, 10x (a season takes 150 s, 50 s, 15 s) | `sim_driver::Speed::{Paused, X1, X3, X10}`, `multiplier` |
| Clock stands still | in founding phases Swim, Scout and PlaceBurrow (day 0 does not start until the burrow is placed), and while a decision card is open | `founding::clock_running`, read by `sim_driver::SimClock::advance` |
| Auto-slow | an unlock, a birth or a new arrival drops 10x/3x back to 1x once | `sim_driver::on_milestone` |

One calendar everywhere. Today four disagree: 365 days (`colony.rs:17`), 30-day seasons
(`tuning.rs:297`, `water.rs:39`), 10-day seasons (`tuning.rs:479`) and 0.4 days/s
(`sim_driver.rs:11`). Retune with the new year: `KIT_DAYS` 360, `ADULT_DAYS` 720, `ELDER_DAYS` 2160
(six years), `MIDSUMMER_DAY` 135, births window day 30-89 (`BIRTH_FROM_DAY` 30, `SPRING_END` 90).
Ecology and events keep their durations in days (wetland forms in 60 days) so only the season
length changes, not the ecology.

## 2. Founding as a state machine (`sim::founding`, new)

`SimState::founding: Founding { phase, day_entered }`; `founding::step(sim, dt)` is called first in
`SimState::step` and is the only code that changes `phase`. After `Established` it does nothing.

| # | Phase | The player sees and does | Leaves when (function checks it) | Next |
|---|---|---|---|---|
| 1 | **Swim** | The pair and two kits swim in down the river to the landing bank. Camera follows. Nothing to click. | Agents reach the landing cell (`founding::arrived`), at most 6 s real | Scout |
| 2 | **Scout** | Free camera. Dry sites near water, trees and mud light up around the landing (`founding::burrow_candidates`: dry, 2-8 cells from water, a tree and a mud bank within 12). A one-line hint: "Pick a burrow site." | Player hovers a candidate and picks it | PlaceBurrow |
| 3 | **PlaceBurrow** | Ghost burrow with its reasons (water near, wood near, ground dry). Confirm or move. | `Command::PlaceBurrow { pos }` accepted by `founding::place_burrow` (a site with `BuildingKind::Burrow`, no materials, dig labour 6) | Dig |
| 4 | **Dig** | The two adults walk to the site and dig; a progress ring on the site. Kits wait on the bank (kits never work). | Site `fraction() == 1.0` (`agents::finish_sites` calls `founding::on_site_done`) | **Settled** |
| 5 | **Settled** = burrow dug | Everyone moves in. Research toast: "Foraging learned: Forager hut". Dam tool lights up. | Player orders a dam: `Command::OrderDam` (existing `SimState::order_dam`) | PlaceDam is the wait here; the order moves to 6 |
| 6 | **PlaceDam** | Dam placement with the existing preview (`dam_preview::preview`: rise, flood area, cost). | `OrderDam` accepted | FellAndHaul |
| 7 | **FellAndHaul** | Trees marked, loads moving to the stockpile and the dam site, site rising by stages. The player assigns who fells, hauls, builds (map orders; idle adults stand still). | Dam site complete: `finish_sites` calls `founding::on_site_done`; records achievement `FirstDam` | PondFills |
| 8 | **PondFills** | Water climbs behind the dam; flooded area and level shown. | Pond reaches 90% of `DamPlan` fill height (`founding::pond_full` reads `water.depth`) or 30 days pass | **Established** |
| 9 | **Established** | Research panel, build menu and seasons are the whole game from here. | (terminal) | |

The founding stockpile is the one free building: `founding::land` adds a complete `Warehouse`
("Stockpile") on the landing bank at the start of phase 1, 60 food and 20 wood in it. The founders
are *not* given a lodge (today `Colony::found` does, `colony.rs:532`); they have no home until
phase 5, and `agents::route_home` already leaves them outside while no lodge exists.

Skipping: `--start 1` begins at phase 1. Starts 2 and 3 begin `Established`.

## 3. Research (`sim::tech`, rewritten in place; UI in `ui/screens.rs::tech_screen`)

Three ways to learn a building:

1. **Known at the start:** Burrow, Dam, Stockpile (`BuildingKind::Warehouse`). Nothing else: today
   `BASE_BUILDINGS` also holds Lodge, Canal and four camps (`tech.rs:98-101`).
2. **Achievement research:** the first time something is done, the research is granted at once, no
   cost. Triggers: `Tech::record(Achievement)` called from the place named in the table.
3. **Experience research:** all of its *needs* met, then it unlocks the same tick. A node has up to
   four needs: **branch experience** (work units done, `Colony::experience` summed by `Branch::jobs`),
   **colony size** (living beavers, `Colony::population`), **prerequisite** research, and for
   late nodes **study** at an Elders' den (section 4).

Experience is credited by the agent that did the work: `agents::finish_work` adds the work units to
`colony.experience[job]` (the abstract `do_work` no longer does, section 8).
Branches: Water = digging, Forest = felling and foraging, Building = building, Craft = workshop
batches, Transport = hauling. Defence stays hidden with its system (`features`).

### The tree

| Building(s) | Research | Route | Branch XP | Colony size | Needs also | Study (lore) |
|---|---|---|---|---|---|---|
| Burrow, Dam, Stockpile | Founders' knowledge | start | - | - | - | - |
| Forager hut | Foraging | **Achievement** FirstBurrow (phase 5) | - | - | - | - |
| Logging camp, Mud pit | Felling | **Achievement** FirstDam (phase 7) | - | - | - | - |
| Food cache | Winter stores | **Achievement** FirstWinter (day 360, nobody lost to hunger or cold, food >= 1 per head) | - | - | - | - |
| Lodge | Lodge building | Experience | Building 16 | 5 | Felling | - |
| Quarry camp | Stonework | Experience | Forest 40 | 6 | Felling | - |
| Workshop | Weaving | Experience | Forest 60 | 8 | Felling | - |
| Elders' den | Elders' lore | Experience | - | 8 | Lodge building, an Elder alive (age >= 6 y) | - |
| Canal | Canal digging | Experience | Water 60 | 12 | Stonework, Lodge building | 30 |
| Dock | Docks | Experience | Transport 40 | 14 | Canal digging | 40 |
| Lock, Water wheel | Flow control | Experience | Water 120 | 18 | Canal digging | 60 |
| Barge | Barges | Experience | Transport 80 | 20 | Docks | 60 |
| Storehouse | Grand storage | Experience | Building 100 | 20 | Lodge building | 60 |
| Mill | Industrial milling | Experience | Craft 100 | 24 | Weaving, Flow control | 80 |
| (bonus) build work x0.75 | Master builders | Experience | Building 60 | 10 | Lodge building | 40 |
| (bonus) storage x1.25 | Forest lore | Experience | Forest 100 | 10 | Stonework | 30 |
| (bonus) craft speed x1.5 | Fine tools | Experience | Craft 50 | 10 | Weaving | 40 |

Canals and the industrial tier (Lock, Water wheel, Storehouse, Mill, Barge) sit at colony sizes
12-24 and 60-120 branch XP: years 3-6 at a normal pace, never in the opening year.

Costs follow: Stockpile loses its Rope cost (no rope exists at the start): `Logs 20, Sticks 10,
Mud 6`; Burrow becomes dig labour 6 with no materials. Both in `tuning::buildings::cost_and_work`.

What the player sees: the Tech screen lists every node, locked ones too, each needing row as
`have / need` with a bar (Building XP 14/16, Colony 5/5, Felling done). The one cheapest unmet
need is shown on the node itself; hover opens the full list (the existing `Explained` parts).
A node's own page says what it unlocks and which achievement or study gives it.

## 4. Elders' den

A building (new `BuildingKind::EldersDen`, 2 study slots, `Role::Study`, no materials beyond
`Logs 12, Mud 8`, labour 6; footprint 3x3 dry) unlocked by the Elders' lore research above.

* Only `Stage::Elder` beavers may fill a study slot (`work::Work::assign` rejects others).
* Each studying elder yields **1 lore per day** *(tune)*, so a 60-lore node takes 30 days with
  one elder, 15 with two (22 s at 1x with two).
* The player picks one node to study; lore goes to it (`tech::Tech::study_target`, set by
  `Command::SetStudy { tech }`). A node with a study cost is not unlocked until lore is full
  **and** its other needs are met.
* An elder away from the den teaches kits as before (`colony::teach_young`).
* Elders arrive six years after birth. The founding pair are three years old, so the first elders
  and the den are a year-4 event; a colony that grows by births alone does not wait longer.

## 5. Colony growth (`colony::births`, `colony::home_beds_free`, `society::request_join`)

A pair is a mated adult male and female sharing a home. Growth has exactly three sources.

1. **Spring litters.** Once per year per pair, the first day in the window day 30-89 on which all
   hold (`colony::births`):
   * the mother is a healthy adult (health >= 0.5, not past 10 years) with a living mate in her home;
   * **stored food** (`colony.food`) >= `BIRTH_MIN_FOOD_DAYS x HUNGER_PER_DAY` = 7.5 per beaver
     **after** the litter, counting the whole colony;
   * **home room:** free beds in her home >= 1. Beds: Burrow 6, Lodge 12 (`catalogue::Def::beds`,
     new; the room check is `colony::home_beds_free`);
   * litter size is a random 1-4 (`MIN_LITTER` 1, `LITTER_SPREAD` 4), then cut to the free beds and to
     the food-affordable count. The cut litter is simply smaller; no birth is queued.
   The births panel says which condition is short ("Needs 12 more stored food", "Burrow full").
2. **Arrivals the player admits.** Wanderers and neighbours do not appear on their own. Each
   becomes a `JoinRequest` decision at a map cell (existing `society::JoinRequest`); only
   `Society::answer(Welcome)` adds the beaver (`colony::add_immigrants`), and only if beds are free.
   Request chance per day rises with stored food per head and free beds; none while hungry.
3. **Young pairs ask to leave a home.** A grown unpaired pair that has no free bed raises a
   `JoinRequest`-style card, "A young pair needs a lodge". If the player builds a lodge, they
   move in. Nothing is built or founded for them (`colony::pair_and_disperse` is deleted).

Ages: kit < 1 y (never works, eats 0.4), yearling 1-2 y (works at half), adult 2-6 y, elder 6 y+
(works at 0.6, studies, teaches).

## 6. Who works (the rule the whole game rests on)

A beaver works only if (a) an adult or yearling and (b) the player gave it something: a slot at a
building (`FillSlot`), a resource or area order (`Assign`), or a site to build. Otherwise it
stands, rests in its home, eats. Nothing assigns work: no automatic organiser, no family
"priority" fallback, no "hungry colony forages" fallback. The Work screen shows idle adults first,
so an idle adult is a visible choice, not a hidden loss.

## 7. Paper walk-through: the opening year

Start 1 River, 1x. Pair A (adult), B (adult), two kits aged 150 days, 60 food, 20 wood in the
stockpile. Adults work 0.6 units/day (`WORK_FRACTION`); a tree takes 2 felling units and gives
3 logs, 4 sticks; a load is 3 units; the dam is 15 logs + 20 sticks + 18 labour *(tune; today 8)*;
the Forager hut 8 sticks + 4 mud + 2 labour; hunger is 0.7 food/day for the family (kits 0.4).
`t` = day / 0.6, seconds from the burrow being placed.

| Day | t | State | What happens | Result |
|---|---|---|---|---|
| - | -5 s | Swim, Scout | Swim in (clock frozen), pick a site from the lit candidates (clock frozen) | |
| 0 | 0 s | PlaceBurrow -> Dig | Burrow placed; A and B dig 6 units at 1.2/day | |
| 5 | 8 s | **Settled** | Burrow done; family moves in. FirstBurrow -> **Foraging: Forager hut unlocked (first unlock)** | Dam tool lit |
| 5 | 8 s | PlaceDam | Player orders the dam and a Forager hut; A and B fell 2 trees, B digs 4 mud | |
| 11 | 18 s | FellAndHaul | Hut done; B takes its slot, forages about +3 food/day; A alone takes the dam | food ~60 |
| 30 | 50 s | spring window | Pair + 2 kits = 4; stored food ~85 >= 7.5 x 6; free beds 6 - 4 = 2; litter rolls 3, cut to **2**. Population 6, burrow full | Lodge not yet researched |
| 38 | 63 s | | Forest XP 40 (felling and foraging, 1.2/day from day 5), size 6, Felling not yet learned: waits | |
| 67 | 112 s | FellAndHaul -> PondFills | A finished the dam (5 trees, 12 loads, 18 labour). FirstDam -> **Felling: Logging camp, Mud pit.** Lodge building: Building XP 20 >= 16, size 6 >= 5, Felling done -> **Lodge unlocked**. Stonework: Forest XP ~100, size 6 -> **Quarry camp** | Three research toasts at once |
| 87 | 145 s | **Established** | Pond at 90%; food ~110 | |
| 180 | 300 s | autumn | Player has lodge, logging camp, mud pit placed (their orders, their jobs) | |
| 210 | 350 s | | The two kits turn 1 y: yearlings, half work | 2 more hands |
| 270-359 | 450-600 s | winter | Food use ~1.0/day x 90 = 90; stockpile holds ~150 | |
| 360 | 600 s | year end | FirstWinter -> **Winter stores: Food cache.** Weaving waits on size 8; Elders' den waits on size 8 and an elder | Year 2 |

The opening year reaches new building unlocks at day 5 (8 s, Forager hut), day 67 (112 s, Logging
camp, Mud pit, Lodge, Quarry camp) and day 360 (Food cache): the first within **8 seconds** of
placing the burrow and three separate unlock moments inside the 600-second year.
If the player forages first and fells later, the dam slips to about day 110 and Felling with it;
if nobody forages, stored food falls below 45 before day 30 and the first litter is cut to 0-1.
That tension is intended; the card for the litter says why.

## 8. Violations in current code (what the tasks must remove)

Auto-building:
* `crates/sim/src/agents.rs:656-736` `organise` staffs open sites with builders, sends gatherers for missing goods, with the switch at `agents.rs:856`, set true at `starts.rs:234` and `lakes.rs:379` (field `work.rs:125`).
* `crates/sim/src/colony.rs:1162-1205` `pair_and_disperse` spends wood (`colony.rs:1195`) and creates a lodge (`colony.rs:1197`) with no order.
* `crates/sim/src/colony.rs:746-748` forces a Builder job when a lodge is failing, and `colony.rs:855` repairs it from the wood store.
* `crates/sim/src/colony.rs:532` (`found`) hands the founders a finished lodge; `starts.rs:226-228` adds a finished stockpile (the stockpile is kept, the lodge is not).

Kits working:
* `crates/sim/src/lib.rs:281-287` `Command::Assign` checks no life stage (only `FillSlot` does, `lib.rs:290-295`).
* `crates/sim/src/agents.rs:126-133` `Agents::assign` stores an assignment for a kit; its doc comment leaves the check to `step`.
* `crates/sim/src/work.rs:160-170` `Work::assign` puts any `BeaverId` in a slot.
Kits are only kept from working downstream (`colony.rs:737`, `agents.rs:443-445`); the rule is not at the command layer.

Idle adults working:
* `crates/sim/src/colony.rs:736-752` `effective_job` gives every non-kit a job: forager when hungry (`:744`), builder for roofs (`:748`), a family priority otherwise (`:751`).
* `crates/sim/src/colony.rs:781-815` `plan_work` plans work for all of them and `colony.rs:850-866` `do_work` turns it into food, wood, digging, tools and experience with no order behind it.
* `crates/sim/src/agents.rs:656-736` `organise` also puts unassigned adults to work (`agents.rs:680-684` takes adults with no slot and no assignment).

Self-growth:
* `crates/sim/src/colony.rs:1162-1205` `pair_and_disperse` founds new families unasked, and `colony.rs:1181-1192` conjures a wanderer (`immigrant`, `colony.rs:1208`) as a partner.
* `crates/sim/src/colony.rs:1108-1155` `births` never checks lodge beds (only `lodge.is_some()`, `:1136`), rolls 1-3 (`tuning.rs:205-206`) in a 75-150 day window of a 365-day year (`tuning.rs:18-20`).
* `crates/sim/src/lib.rs:187-190` region arrivals and `lib.rs:357-358` event migrants call `colony.add_immigrants` (`colony.rs:1400-1406`) with no join request.

## 9. Rules and who enforces them

Each rule names the module and function that enforces it, and the task that owns the change.
Where a rule also depends on a constant, the constant is listed after the function that reads it.

| Rule | Module::function | Task |
|---|---|---|
| R1 One calendar: 360-day year, 90-day seasons | `calendar::season_of`, `calendar::YEAR_DAYS`; `water::Season::at`, `events::Calendar` use it | calendar |
| R2 150 s per season at 1x (0.6 days/s) | `sim_driver::SimClock::advance` (adds `real_dt x BASE_DAYS_PER_SECOND x Speed::multiplier()` sim days, `sim_driver.rs:72`); constant `BASE_DAYS_PER_SECOND` = `SEASON_DAYS / SEASON_SECONDS` | calendar |
| R3 Speed steps pause/1x/3x/10x | `sim_driver::Speed::multiplier`, `flow_view` time bar | calendar |
| R4 Clock stands still in Swim, Scout, PlaceBurrow | `founding::clock_running` checked in `sim_driver::SimClock::advance` before it adds days | founding |
| R5 Founding phases only change in `founding::step` | `founding::step`, `founding::on_site_done` | founding |
| R6 Founders arrive without a lodge; stockpile free | `founding::land` replaces `colony::found` use in `starts::build_river` | founding |
| R7 A burrow is dug by adults, no materials | `tuning::buildings::cost_and_work(Burrow)`, `agents::build`, `founding::place_burrow` | founding |
| R8 Only the player places buildings | `SimState::place_site` via `Command::PlaceSite`; `agents::organise` deleted; `work.auto_organise` field deleted | work-rules |
| R9 Placing needs research | `tech::Tech::allows` called in `buildings::Buildings::can_place_site` | research |
| R10 No new lodges from dispersal | `colony::pair_and_disperse` deleted; `society::request_lodge` raises the card | growth |
| R11 Kits never work | `work::can_work(stage)` called by `Work::assign`, `Agents::assign`, `SimState::apply(Assign/FillSlot)` | work-rules |
| R12 Idle adults do not work | `colony::effective_job` returns the beaver's order only; `colony::do_work` produces nothing (cut-over); `agents::duty_of` is the sole source of work | work-rules |
| R13 Experience comes from work done by agents | `agents::finish_work` credits `colony.experience` | work-rules |
| R14 Everything beyond the start set needs research | `tech::Tech::allows` (true only for `BASE_BUILDINGS` or a kind unlocked by a researched tech, `tech.rs:123`), called by `buildings::Buildings::can_place_site`; constant `tech::BASE_BUILDINGS` cut to Burrow, Dam, Warehouse | research |
| R15 Achievements grant research at once | `tech::Tech::record(Achievement)`: called by `founding::on_site_done` (FirstBurrow, FirstDam), `SimState::step` at winter end (FirstWinter) | research |
| R16 Experience research needs XP, size, prerequisites | `tech::Tech::update(&SimState)` checks `TechDef::{xp, size, requires}` | research |
| R17 Late research needs study at the den | `tech::Tech::study(dt, lore)`, `TechDef::lore` | research |
| R18 Only elders study; 1 lore/day each | `work::Work::assign` (Role::Study accepts `Stage::Elder`), `tech::LORE_PER_ELDER_DAY` | den |
| R19 Needs are visible with have/need | `ui::screens::tech_screen`, `explain::audit` | research |
| R20 Births in spring window only | `colony::births` using `calendar::in_birth_window` | growth |
| R21 Litter 1-4, cut to beds and food | `colony::births` (`MIN_LITTER` 1, `LITTER_SPREAD` 4) | growth |
| R22 Births need 7.5 stored food per head after the litter | `colony::births` using `BIRTH_MIN_FOOD_DAYS x HUNGER_PER_DAY` | growth |
| R23 Births need a free bed | `colony::home_beds_free`, `catalogue::Def::beds` | growth |
| R24 Arrivals are decisions | `society::request_join`; `lib::step_events` and `ColonyHost::add_beavers` call it, not `add_immigrants`; `Society::answer` admits | growth |
| R25 Elders from age 6 y | `tuning::colony::ELDER_DAYS`, `colony::Stage::of_age_days` | calendar |

Every rule above is covered by a unit test in its own module (one function, runs in seconds); no
integration tests are committed.

### File ownership

One task owns a file or a named function range; others change it only through the public API.
Order: calendar first; then founding, work-rules, research, growth in parallel (their file sets do
not overlap except where split by function); den after research and work-rules; ui last.

| Task | Sim and game files |
|---|---|
| calendar | `sim/src/calendar.rs` (new), `tuning.rs` calendar constants (colony, water, events blocks), `water.rs::Season::at`, `events.rs` default calendar, `game/src/sim_driver.rs`, time controls in `game/src/flow_view.rs` |
| founding | `sim/src/founding.rs` (new), `starts.rs::build_river`, `colony.rs::found` (that function only), `lib.rs` (`founding` field, `Command::PlaceBurrow`), `game/src/opening.rs`, `startflow.rs`, `flow.rs` |
| work-rules | `agents.rs` (delete `organise`, add `can_work` use, `finish_work`), `work.rs` (`assign`, `can_work`, remove `auto_organise`), `colony.rs` (`effective_job`, `plan_work`, `do_work`, `repair` only), `lib.rs` (`Assign`, `FillSlot` arms), `starts.rs:234`, `lakes.rs:379`, the three game call sites of `auto_organise` |
| research | `tech.rs`, `catalogue.rs` (`unlock`), `tuning.rs` tech and buildings cost blocks, `buildings.rs::can_place_site`, `game/src/ui/screens.rs::tech_screen` |
| growth | `colony.rs` (`births`, `home_beds_free`, `immigrant`, `add_immigrants`, delete `pair_and_disperse`), `society.rs` (`request_join`, `request_lodge`), `tuning.rs` colony growth block, `lib.rs` (`step_events`, `ColonyHost::add_beavers`) |
| den | `buildings.rs` (`BuildingKind::EldersDen`), `catalogue.rs` (slots, beds), `work.rs` (`Role::Study`), `tech.rs` (study), `agents.rs` (Study duty, after work-rules), `lib.rs` (`Command::SetStudy`) |
| ui | `game/src/ui/screens.rs` (founding hints, births panel, Work idle list), `ui/events.rs` (cards), `ui/orders.rs` placement hints |
