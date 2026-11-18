# Beaver Colony first-year audit

*Interface and early-gameplay feasibility, first 360 days. 18 Nov 2026, branch `interface-v2`.*

## Summary

**The first year is not survivable as the game is tuned today.** In the final round of 16 scripted runs (4 maps × 4 ways of playing), 14 colonies collapsed before day 360. The two survivors lived only because strangers kept joining (Bavarian Forest, attentive play: 12 beavers but 136 days with no food) or because one beaver hung on (Algonquin, attentive play: 6 of 7 starved). Every death but a handful was starvation.

The new start works as designed where the ground allows it. The family arrives with nothing built, the player picks the spot, the bundle is dropped as a pile, and the first burrow, store and dam are built from it. On Algonquin and Bavarian Forest the burrow stood by day 6–11, the warehouse by day 23 and the dam by day 73–84. On Telemark and Voyageurs the designed camp is walled in by forest or sits on the map's edge, and the family can't build anything at all.

The interface held up. The text budget, alerts with fixes, inspectors and Colony screen all did their jobs in the captures. The worst interface problems are alerts that offer no useful fix exactly when the colony is dying, misleading "runs out" countdowns on building materials, and raw debug text in the Log tab.

## How this was tested

- **Scripted players** (`sim::autoplay`, run by `cargo run --release -p sim --example first_year`). They act only through player commands and the alerts' own fix buttons, never through internals:
  - **Attentive:** builds burrow, warehouse and dam at once, adds each producer as research unlocks it, sets the workshop to rope, and acts on the top alert every day.
  - **Ignores alerts:** builds the same way, never touches an alert.
  - **Hands-off:** settles and leaves everything to auto-staffing.
  - **Attentive, other site:** settles about 12 cells from the designed camp.
- **Maps:** Algonquin, Voyageurs, Bavarian Forest, Telemark. 360 days each, about a minute for all 16 runs.
- **HUD captures:** the real game, played by the attentive script to day 12, 45, 120, 200 and 300, then screenshotted on the strip, inspector, Jobs, Goods, Log and beaver views (`BC_HUD_DEMO=play`).
- **Not covered:** mouse feel. Desktop control wasn't available, so hover timing, click targets, drag-box selection and tooltip freezing were not tried by hand.

## Results

| Map | Attentive | Ignores alerts | Hands-off | Attentive, other site |
|---|---|---|---|---|
| Algonquin | alive, 1 beaver, 6 starved | collapsed day 82 | collapsed day 82 | collapsed day 336 (cold, injury: never housed) |
| Voyageurs | collapsed day 286, 17 starved | collapsed day 183 | collapsed day 286 | collapsed day 182 |
| Bavarian Forest | alive, 12 beavers, 1 starved | collapsed day 96 | collapsed day 96 | collapsed |
| Telemark | collapsed day 101 | collapsed day 101 | collapsed day 101 | collapsed |

When things were built (attentive play):

| Map | Burrow | Warehouse | Dam | Forager hut | Research by day 360 |
|---|---|---|---|---|---|
| Algonquin | day 6 | day 179 | day 223 | day 12 | Foraging d6, Felling d223 |
| Bavarian Forest | day 11 | day 280 | day 84 | never (stalled) | Foraging d11, Felling d84, Lodge building, Stonework, Weaving d224 |
| Telemark | never | never | never | never ("unreachable" ×9) | none |
| Voyageurs | never ("out of bounds") | never | never | never | none |

## Fixed during the audit

These were found by the runs and fixed straight away, because without them the new start could not work at all:

1. **Homeless exhaustion deadlock.** With no home, a resting beaver recovered rest at about 0.01 a day, so the adults stopped building the burrow after nine days and sat out about 25 days. Sleeping rough now rests at 40% of a lodge (`OPEN_REST`); cold still needs shelter. Telemark homeless-days went from 66 to 11.
2. **Errands that never give up.** A builder or fetcher with no route to its target stood still for ever. It now drops the errand.
3. **Building over the supply pile.** A warehouse could be placed on the pile. That's now refused.
4. **No way to make rope.** There was no command or button to choose a workshop recipe, so rope (needed by the warehouse, water wheel, dock, barge, mill and storehouse) could never be made. There's now a `SetRecipe` command and a rope/baskets/tools picker in the workshop inspector.

## Gameplay findings

Most serious first. Each has the evidence and a suggestion.

### G1. The colony cannot feed itself in year one

- **Evidence:** food sat at zero for 54 to 269 days across the runs. Starvation was the main cause of death in 15 of 16 runs.
- **Causes:**
  - The 30-day larder runs out around day 30, and the first forager hut is only unlocked by the first burrow (day 6–11).
  - A staffed forager hut made about 1.1–1.3 food a day at best. Four beavers need about 0.7 a day at the start, but demand rises as kits grow and strangers join.
  - Plants grow only on wet cells. Each holds 3 food and regrows 0.3 a day, and on forested starts most are out of reach.
  - **Auto-staffing ranks Builder above Forager, even at zero food.** The Jobs board on day 45 showed builders 2/4 and foragers 0/2 with food at 0.
- **Suggestions:**
  - Raise Forager to the top of the role order automatically while food is under about 10 days. Alternatively, have the food alert's fix move workers into foraging.
  - Make the forager hut a base building, or unlock it on settling.
  - Raise the larder to 45–60 days.
  - Add a second early food source. Bark from felled trees exists but is small; fish near ponds would tie food to dams.
  - Check that a family of four can break even on each map's designed start before shipping.

### G2. Two of the four designed camps are traps

- **Evidence:**
  - **Telemark:** "could not place Forager hut: unreachable" nine times, and nothing else was built either. The settle cell is itself a tree, inside forest.
  - **Voyageurs:** the camp is at x = 124 on a 128-cell map. Burrow and warehouse were "out of bounds", and nothing was ever built.
- **Suggestions:**
  - Settling should require room to build: a few dry, reachable cells for a 3×3 store and a burrow.
  - The settle tag should show that room as a fourth bar, beside water, trees and safety.
  - Families should never swim in at the map's edge.
  - Move the designed camps on these two maps, or clear a landing.

### G3. Construction sites wall beavers in

- **Evidence (Bavarian Forest, first round):**
  - The 3×3 warehouse site was placed beside the family where they had landed at the map's edge.
  - Sites block movement from the moment they're placed, so the family was sealed in.
  - The supply pile ended up inside the warehouse footprint.
  - Nothing was built all year.
- **Fixed partly:** no building on the pile, and errands give up.
- **Suggestions:**
  - Let unfinished sites be walked across.
  - Refuse a placement that would cut beavers off from the pile, their home or each other.
  - Show "Unreachable" in the site inspector and as a marker.

### G4. The bundle runs short of mud

- **Evidence:**
  - Mud ran out on day 12 (the warehouse takes 10 of the 12, the forager hut 4).
  - The forager hut, mud pit, workshop, lodge and quarry then stalled 70–107 days "waiting for a material the colony has none of".
  - Mud pits need the Felling tech, which needs the first dam.
- **Suggestions:**
  - Bring 20 mud, or drop mud from the forager hut's cost.
  - Allow gathering mud by order before the mud pit.
  - Add a "site waiting for mud, none in stock" alert whose fix sends free beavers to dig.

### G5. Strangers join a starving colony, and their alert never clears

- **Evidence:**
  - The "asks to join" alert was up on 318 of 360 days in one run, because the top alert slot was always something else.
  - On Voyageurs strangers joined and then starved: 17 deaths from a family of four.
- **Suggestions:**
  - A request should expire after a few days.
  - Strangers shouldn't arrive when food is under about 15 days.
  - The fix should show the food cost of accepting.

### G6. No births in any run

- Kits from the founding pair grew up, but nobody was born in year one.
- That may be intended (breeding needs housing and food), but the reason is invisible. The Population tab should say why: no free home, too little food, or the season.

### G7. Wood is a resource that runs out for good

- Felled trees never regrow; tree resources are removed when used up.
- The ecology forest-density grid regrows by itself, but felling never lowers it, so the two forest models disagree.
- See the forestry plan for the fix.

### G8. Exploring

- Fog of war is now drawn, and only beavers reveal ground.
- The only way to explore is the walk order (right-click open ground with beavers selected).
- Reaching a valley's edge opens the valley, but nothing on screen says where the edges are.
- **Suggestion:** show locked-valley borders as a dashed line on the map and minimap.

## Interface findings

| # | Finding | Seen | Suggestion |
|---|---|---|---|
| U1 | The strip shows red "runs out in 4 d" countdowns on logs, sticks and mud while they're only going into construction sites | day 12 strip | Count down only for consumption (eating, crafting, repairs); show what sites still need as a separate number in the tooltip |
| U2 | The Log tab shows raw debug text: `Died { id: BeaverId(9), cause: Starvation, at: (70.5, 84.5) }` | day 200 Log | Format events as icon, name and one word ("Iris · starved"), with ⌖ to the place |
| U3 | The urgent food alert often has no fix button, only ⌖. The Place fix can point at a building that fits nowhere reachable | Telemark day 40 | Alerts should check their fix is possible. When it isn't, offer the next one (gather order, move foragers) |
| U4 | "Sites without builders" says "Show" when everyone is busy | all runs, all year | Offer "+builders" taken from a lower-priority role, with the source in the tooltip |
| U5 | Building inspector header has five small icon buttons (priority, pause, show, pin, close); a dot for "normal priority" reads as nothing | forager hut day 12 | Show priority as ▲/–/▼ text-free stepper; group pin and close; move show to the name |
| U6 | Goods tab: logs In 0 / Out 0 while logs fall; food "lasts 158 d" beside Net −0.1 | day 120 Goods | Count site deliveries as Out; use one food-days formula everywhere |
| U7 | Store rows in the Goods tab are empty: stock is colony-wide, not per store | day 120 Goods | Show colony totals only, or track per-store stock properly |
| U8 | Jobs board column header icon floats out of place; "Filled" is the only word | day 45 Jobs | Align the header row; use icons for both columns |
| U9 | The settle tag rates water, trees and safety but not room to build, so the traps in G2 look like good spots | settle view | Add a room bar; refuse spots without room |
| U10 | The minimap is almost all cloud early on; the explored disc is tiny | every capture | Zoom the minimap to the explored area plus a margin |
| U11 | At the map's edge the camera shows the world floating in blue | Bavarian day 300 | Clamp the camera to the map; tint beyond the edge |
| U12 | The supply-pile marker (logs + food icons) can be read as "out of logs and food" | day 12 | Use a single pile icon; tooltip lists what's left |

## What worked

- **The start as described:**
  - Nothing is built on the map.
  - The settle chip shows the family and what it carries.
  - The cursor tag rates each spot.
  - The game never settles for you.
  - Settling drops the bundle as a pile, and builders fetch from it.
- **Shelter:** a finished burrow houses the family the same day.
- **The dam:** built from the bundle on two maps, with the pond forming behind it (Bavarian Forest, day 300 capture).
- **Research:** progressed through Foraging, Felling, Lodge building, Stonework and Weaving on the surviving map.
- **Auto-staffing:** staffed sites and huts without the player touching anything. Beavers freed from sites moved to the next job.
- **Fog of war:** reads clearly. The minimap matches it.
- **Inspectors:**
  - The forager hut showed "No workers", its empty slots, the cap, and the single limiting factor.
  - The beaver panel showed needs, mood and health, with the low rest bar in red.
- **Text:** no panel showed more than five words; tooltips carry the rest.

## Running the game programmatically

You asked how feasible it is to drive every player action from code. It's very feasible, because most of it already exists.

### What exists now

1. **The sim command set.** Every state-changing player action that touches the world is a `sim::Command`, applied by `SimState::apply`:
   - Settle, PlaceSite, OrderDam, DigCanal, BreachDam
   - Assign (gather a resource, gather in an area, walk somewhere)
   - FillSlot, EmptySlot, AddWorker, RemoveWorker
   - AdjustRole, SetRoleOrder, SetPriority, SetPaused, SetAutoStaff, Unlock
   - SetWorkArea, SetHaul, SetRecipe, AnswerJoin

   The sim is deterministic and headless. A year runs in a few seconds in release.
2. **Scripted players** (`sim::autoplay`) and the audit runner (`examples/first_year.rs`), built for this audit.
3. **A screenshot driver in the real game:** `BC_HUD=1 BC_HUD_DEMO=play BC_HUD_DAYS=N BC_HUD_VIEW=jobs --screenshot out.png --map telemark` plays N days, then captures a view.
4. **HUD trees as data.** Every panel is built as a tree before it's drawn, so a test can read the exact words, icons and buttons on screen without rendering. The text-budget test already does this.

### What's missing for "everything a player can do"

| Gap | Where it lives now | What to do | Effort |
|---|---|---|---|
| Settling rules and fog of war | game crate (`flow.rs`) | Move fog, "explored" and `can_settle` into the sim, so a scripted player sees what a player sees | 1 day |
| Placement checks, costs, dam and canal line rules, canal orders | game crate (`build_logic.rs`) | Move the checks into the sim; canals become a sim command like dams | 1–2 days |
| Undo | split between sim (`cancel_site`) and game (pending canals) | One `Undo` command in the sim | ½ day |
| Interface actions (open a panel, select, pin, sort, search, speed, camera, menus) | game crate `HudAction`, menu actions | One `PlayerAction` enum covering sim commands and interface actions; the HUD's buttons and keys send it; a script can send it too | 1–2 days |
| Record and replay | — | Log every `PlayerAction` with its tick. The sim is deterministic, so a log replays a whole game exactly; bug reports become files | 1 day |
| Headless game runs | — | Run the Bevy app without a window, inject `PlayerAction`s, and assert on HUD trees and alerts each tick. Screenshots stay for visual checks | 2–3 days |
| Real mouse input | — | Write synthetic cursor and button events into Bevy's input for end-to-end tests of hover, drag and click | 1–2 days |

About a week and a half of work gives a harness that can do anything a player can, replay any game, and check the interface's words and state without a screen. The sim side (first three rows) is the most valuable part and the cheapest.

## Recommended next steps, in order

1. Make the first year survivable:
   - food emergency staffing (G1)
   - forager hut from the start
   - a longer larder
   - 20 mud in the bundle (G4)
2. Fix the two trap camps, and add a "room to build" check to settling (G2).
3. Make unfinished sites walkable (G3).
4. Fix the alerts' fixes (U3, U4), the material countdowns (U1) and the Log text (U2).
5. Expire stranger requests and stop arrivals during famine (G5).
6. Build the forestry chain (separate plan).
7. Build the `PlayerAction` layer, and move fog, settling and placement into the sim, so the next audit can be fully scripted and replayable.
