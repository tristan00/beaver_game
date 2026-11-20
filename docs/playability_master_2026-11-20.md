# Gameplay and interface review — master, 20 November 2026

This pass started from `master` at `a2054ca`. The earlier audit and repairs on
the obsolete branch were not used as evidence or cherry-picked. Findings below
come from this checkout, native Windows playtests, and the simulation's real
command API. Work stays on `master`.

## First-play experience

The title screen leads to a settlement view explaining where to click and how
to navigate. Settlement pauses the clock and frames the family. The HUD names
Food, Sticks and Clay, labels the construction categories and individual
buildings, and explicitly says when time is paused. A starting guide offers
shelter, food production and storage, with prerequisite feedback.

Family members have named, clickable markers. Selecting one shows explicit
Move, Gather, Work and Stop / auto controls, its current activity, needs and
carried load. Shift-clicking names extends the selection; Family selects adults.
Tree cutaways clear both the immediate area and foreground crowns that obstruct
the camera's view of a beaver or selected building. F8 still toggles the full
cutaway view. Cutaway trees keep short trunks so their location remains visible.

## Action contract

| Action | Input | Result and feedback |
| --- | --- | --- |
| Select | Left-click a name, beaver, building or resource | Inspector identifies the selection; selected names turn orange. |
| Group selection | Shift-click names, or drag a box | Orders use the selected group; inspector reports its size. |
| Find workers | Family, Select adults, or I for the next free worker | Family frames the adults; I cycles available workers. |
| Move | Right-click ground, or M / Move then left-click | Replaces the previous order. Each group member receives a distinct reachable destination. The inspector shows queued, moving, arrived or blocked. |
| Gather | G / Gather then click a tree, food plant or clay deposit | Assigns collection in the surrounding area. Shift targets that one resource. Invalid targets get an explanation. Selection remains on the workers after issuing the order. |
| Work | R / Work then click a workplace | Fills available worker slots and reports the assigned count or lack of open slots. |
| Release direct orders | Stop / auto | Cancels the selection's assignments and returns workers to colony staffing. Carried goods remain accounted for. |
| Build | Choose a labeled category and building; click a valid site | Green footprint means valid; invalid placement shows a reason. Placement selects the queued site and exits the tool. Shift allows repeated placement. |
| Construction | Resume time after placing | Available adults are recruited by existing automatic staffing. Inspector shows worker counts, material delivery and construction labour. |
| Pause | Pause button | Always pauses; clicking an already selected Pause button does not unexpectedly resume. |
| Resume and speed | Play / fast buttons, or 1 / 2 / 3 | Resumes at the selected speed. Space toggles pause. |
| Camera | WASD / arrows, Q / E, wheel, middle drag | Pan, rotate, zoom and orbit. Right-click has one order meaning. Focusing a worker preserves the player's zoom after the initial arrival. |
| Help | Help / H | In-game control reference and opening guidance. Long panels scroll. |
| Cancel | Right-click or Escape while using a tool | Cancels that tool; Escape otherwise closes the current panel/selection before opening the pause menu. |
| Urgent problem | Persistent warning near the top | Names the problem and offers its relevant action even while a beaver or building inspector is open. |
| Colony loss | Automatic pause menu | Explains the loss and offers restart, settings, title and quit. The simulation stays stopped. |

## Defects reproduced and repaired

1. **Moves were effectively instantaneous.** The automatic-work travel budget
   was also used for direct orders. Direct movement now traverses land at four
   cells per real second at normal speed, with the existing walking/swimming
   animation and interpolation. Fast-forward accelerates it. Automatic labour
   and hauling keep their existing travel rate.
2. **Replacement orders retained obsolete routes.** Assigning an order now
   clears the old path, interpolation fraction and trail, while preserving loads.
   Direct movement takes priority over resting and delivery; kits can move but
   still cannot accept work assignments.
3. **An accepted move could vanish silently.** Commands validate reachable
   destinations before changing assignments. A later obstruction leaves an
   explicit blocked state. Invalid group orders do not partially overwrite the
   group's previous orders.
4. **Picking and placement disagreed with rendered cell centres.** Terrain
   picking now uses the cell containing the hit; footprints and area outlines
   use the same cell boundaries as resources and buildings.
5. **Finishing a gathering click changed the selection.** Its release event
   is consumed, keeping the workers selected after their order is issued.
6. **Trees concealed beavers and construction.** Named markers, shorter
   cutaway meshes and camera-to-target occlusion checks make the active subjects
   visible. This is presentation only; trees remain in the simulation.
   A close-up capture also reproduced a tree surrounding the camera itself;
   automatic cutaway now includes nearby camera geometry and both ends of the
   view segment. The failed close-up is retained as `close-view-before.png`.
7. **Essential controls and messages were only icons or unexplained numbers.**
   Added labels, activity messages, placement confirmations, worker counts,
   persistent urgent warnings and explicit pause feedback. Unknown command
   errors retain their explanation instead of becoming only “Refused”.
8. **Panels clipped content.** Help, inspectors, colony panels and settings
   support scrolling; settings have a bounded height. The construction rail is
   removed during placement instead of leaving stale controls on screen.
9. **Gathered materials could disappear.** A regression test first reproduced
   four clay becoming zero when no warehouse existed. Non-food loads now return
   to the existing camp supplies. Full storage and unreachable delivery paths
   preserve the load; the inspector explains storage waits. No free stock was
   introduced and resource yields were not increased.
10. **One high-speed setting targeted thirty seconds per month.** Speed 2 now
    targets 7.5 seconds per 30 days; speed 3 targets 5. Both use the fast-forward
    frame budget. Real simulation steps and water time remain unchanged.
11. **Changing maps blocked the interface during valley preparation.** New maps
    and restarts are prepared on a worker thread. A loading message remains
    visible while the existing scene continues to render; failures return an
    explanation. Menu clicks are consumed so resuming does not also select or
    order something in the world behind the button.

## Verification and evidence

Native tests exercised settlement, named selection, a normal-speed move,
arrival, queued feedback while paused, basic building selection, invalid-site
feedback, burrow placement, automatic recruitment and completion, direct tree
gathering, urgent food warnings, help and scrolling. The manual Algonquin burrow
accepted normal placement, recruited two adults and housed the four family
members after completion. Commands were recorded, rather than inferred from
screenshots alone.

The final gathering retest kept both workers selected, displayed the queued
order, and Stop / auto returned the free-worker count to two. A live Voyageurs
food-warning action sent two adults to gather through normal commands: food
rose from about 3 on day 39 to 6 on day 49, and the displayed net rate became
positive. This was earned food, with no inventory edits. Settings and their
scrolling were also checked at 130% text size.
By day 61 that unattended opening needed food intervention again (about 2 food
remaining); the emergency action is temporary relief, not permanent food
security. Its renewed warning and available gathering action remained visible.

The simulation tests cover replacement routes, cancellation without losing a
load, group destinations including kits, invalid targets, visible multi-step
movement, blocked routes and camp delivery/full storage. Existing economy,
construction, repair, food-alert and shipping tests run alongside them.

Four 90-day managed runs use normal commands, paid construction, real water,
identical map seeds and starting supplies. The pre-storage-fix runs are retained
as `*-before-storage-fix.csv`; final runs use `*-90days.csv`. These are short
playability checks, not a claim that long-term economy balance is complete.

| Map, day 90 | Population | Completed buildings | Food | Sticks | Clay |
| --- | ---: | ---: | ---: | ---: | ---: |
| Telemark | 6 | 9 | 30.205 | 47.000 | 0.400 |
| Algonquin | 6 | 4 | 22.921 | 44.500 | 0.000 |
| Bavarian | 6 | 8 | 5.186 | 24.554 | 0.000 |
| Voyageurs | 6 | 4 | 24.608 | 25.500 | 0.000 |

Telemark and Bavarian matched their pre-storage-fix daily population,
inventory and building counts exactly. Algonquin retained more materials and
food; Voyageurs retained more sticks while consuming its clay. Those outcomes
were allowed to change because collected goods now remain in circulation.
Bavarian's small food reserve and the material shortages are still meaningful
constraints; these runs are not proof of effortless or permanent survival.

The native Voyageurs speed-3 run measured 8.0–9.2 seconds per 30 days in its
five-second measurement windows, with 25–27 rendered frames per second. A
live Algonquin speed-2 sample measured about 8.6 seconds per 30 days and 57 fps.
Startup and paused windows are excluded from throughput claims.
The final Voyageurs speed-2 test measured about 8.7–9.3 seconds per 30 days
and 28–30 fps. Switching from Algonquin to Voyageurs displayed the loading
message, continued rendering at approximately 90 fps during preparation, then
opened the correct settlement view and successfully founded a new family.

Final checks: 33 simulation tests passed; the workspace all-target check and
release build passed; `git diff --check` passed. The source-budget failure is
listed below, separately from those passing checks.

All temporary runners, command logs, CSVs, performance logs and capture output
are under `D:/harness_work/workspaces/beaver/tmp/playability-master/`. The runner
is outside the repository, as required for integration experiments.

## Limits

The inherited 20,000-line source-budget check already failed at the starting
revision (29,120 lines). This playability work does not solve that separate
size reduction; the check remains failing and its counting rules are unchanged.
The final count is 29,580 lines, 9,580 above the cap.

Initial application startup still prepares the first map before its window
appears; that can take several seconds. Subsequent map changes use the loading
flow described above. Initial rendering and hydraulic topology changes can cause longer individual frames;
the measured fast-forward throughput is not a claim of zero frame spikes or a
guarantee for arbitrarily large future colonies. The UI remains available during
ordinary tested play. Multi-year balance and every advanced shipping/waterworks
combination were not re-certified by these 90-day runs.
