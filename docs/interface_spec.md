# Beaver Colony interface spec, v2

*Replaces the design page in `D:\harness_work\beaver_ui_design\index.html` wherever the two disagree. Written 2026-11-17 from a review of the running HUD (`crates/game/src/hud.rs`, `crates/sim/src/hud.rs`, `work.rs`, `agents.rs`, `colony.rs`, `tech.rs`) against the 30-game report in `D:\harness_work\reports\City builder and strategy UI design.md`.*

Priorities: **P0** blocks everything else, **P1** is the core of this pass, **P2** follows once P1 is in.

---

## 1. What this spec fixes

The HUD has the right parts (goods strip, alerts with fixes, lenses, dam preview, river profile, flow ledger) but fails in five ways. Each one is a direct breach of the research report.

| # | Problem | Evidence in the build | Report principle broken |
|---|---|---|---|
| 1 | **Text does the job of icons.** Goods, categories, lenses, roles and speed controls are all words. | Bevy's built-in font is ASCII only, and the game has no icon images. Strip reads "Food 6 -0.8/d lasts 8d Logs 12 +0.4/d…"; build tabs read "Basics Z". | Every modelled game uses icon + number for goods and icon tabs for building categories (AoE2, Timberborn, Anno, Banished, Against the Storm). |
| 2 | **Instructions are printed in panels.** | "Esc closes", "Enter builds - RMB or Esc to stop", "0.5-3 - [ ] or Ctrl+wheel", "North up - click or drag to look", "side view along the river, upstream on the left", "No dam yet: dam the river to store water", map credits on the start menu. | Hints belong in tooltips and a How/Why journal you can reopen (Old World, Victoria 3; the Timberborn 1.0 complaint). |
| 3 | **Panels cover the place you are building.** | At 1280×720 with the dam tool on: river profile 600×165 px at screen centre (top edge at y≈405, the camera looks at y=360), building list, build bar, rail, minimap and a 360 px tool card. About 35–40% of the screen stays clear, and not where you are working. The building list stays open while placing; pop-up messages and the building list share the same anchor (bottom 70 px). The placement verdict sits in the right-hand card, not at the cursor. | Keep the world central; give feedback at the cursor (CS2, ISLANDERS, Banished); Farthest Frontier 1.1's "hide widgets during placement". |
| 4 | **No adequate way to assign workers.** | The only control is "+ idle beaver" / "−" per slot inside one building's inspector. The sim's `Command::Assign` (direct gather orders), `SetWorkArea` and `SetHaul` are never sent by the game. `work.auto_organise` exists but is off for players and only covers building sites. An uncommitted change in the working copy removed the worker fixes from the alerts and left nothing in their place. | Idle counter + cycle (AoE2), professions board (Banished, Farthest Frontier), per-building worker steppers (Timberborn, Frostpunk, Manor Lords), job priorities (Kingdoms and Castles). |
| 5 | **The numbers behind the game can't be reached.** | Only the goods strip has a breakdown. Beavers, trees, water cells and lodges can't be selected: `Selection::Beaver / Cell / Tree / Lodge` exist in `flow.rs:77` but nothing sets them. No research screen, no event log, no per-store stock, no ecology data, no population detail. | The explanation stack: icon → number with change → itemised breakdown → nested tooltip → encyclopedia (CK3, Victoria 3, Old World, Civ VI). |

**How much text there is today.** The game can show about **920 words across about 350 strings** (counted from the source; numbers, beaver names and key letters excluded). The biggest sources: menus and settings 178, tool cards 125, inspector 119, alerts 99, goods strip and calendar 91, flow ledger 74. Words on screen at once, counted from the captured views:

| View | Words now | After this spec |
|---|---|---|
| No panel open, one alert | 58 | 0 |
| Start menu | 58 | 5 |
| Flow ledger open | 86 | about 6 (column headers) |
| Inspector open (logging camp) | 84 | 2 (`Working`, `Max`) |
| Dam tool | 113 | 5 (`Rise`, `Flood`, `Trees`, `Push`, `Fill`) |

In the no-panel view, the calendar alone carries 15 words and the minimap caption 7, more than the whole budget for that view.

Two engine-level causes sit underneath:

- **The HUD is rebuilt 4 times a second** (`REBUILD_SECS = 0.25`), and on every cursor move while a tool is active. Every panel's children are despawned and respawned. Hover state resets, a button can be replaced between press and release, and no tooltip can stay open long enough to be hovered. That is why things flicker, why clicks get lost, and why nested tooltips are impossible.
- **There are two separate job systems.** `colony::Beaver.job` / `current_job` (Forager, Logger, Builder, Digger, Hauler, Guard, Crafter, Scout, with skills and colony-wide effects) runs alongside the work model's slot roles (`work::Role`). A Jobs board needs a single source of truth. See W0.

---

## 2. Principles for every element

1. **Icon first, number second, word last, and the fewest words possible** (§F5). This applies to one- and two-word labels as much as to sentences: if a word can go, it goes.
2. **No instructions in panels.** A control's key goes in its tooltip, or in the key-hint bar shown only while a tool is active (§5.2).
3. **Every number can be hovered, and the hover explains it.**
4. **Every thing in the world can be hovered and clicked.**
5. **The work area is sacred.** In any tool mode the central rectangle stays free of panels (§3.3).
6. **One colour, one meaning** (§8). Shape or sign always backs up colour.
7. **Calm by default.** At rest the HUD covers 15% of the screen or less.

---

## 3. Foundations (P0)

### F1. Retained UI instead of rebuild

- Panels are spawned once. Systems update text, values and visibility in place when the underlying data changes.
- A panel's children are rebuilt only when its *structure* changes (a row added or removed), never on a timer.
- Hover and press state survive data updates.
- **Accept:** a button held down across 2 seconds of running sim still registers its click; a tooltip stays open while the cursor sits on it; frame time with every panel open is no worse than today.

### F2. Icon atlas and symbol font

Bundle one icon sheet (24 px and 16 px) and a UI font with ▲ ▼ ● ◆ ■ ✓ ✕ ⏸ ▶.

| Set | Icons |
|---|---|
| Goods (8) | food, logs, sticks, mud, stone, baskets, rope, tools |
| Build categories (6) | basics, water, forest, building, craft, transport |
| Buildings (20) | one per `BuildingKind`, also used as the build-item thumbnail |
| Roles (8) | forager, logger, mud digger, quarrier, builder, hauler, crafter, elder |
| Map views | world, water, goods, plus the P2 views in §4.5 |
| Status | idle, unstaffed, blocked, full, no store, no power, low strength, locked |
| Controls | pause, play ×1/×2/×3, close, undo, search, pin, priority up/down |
| Beaver | adult, yearling, kit, elder; mood and health faces |

**Accept:** no button in the HUD carries a word where an icon exists, and no panel uses "!" or "!!" as an icon.

### F3. Screen zones and layout rules

1280×720 reference; anchors scale with `UiScale`.

| Zone | Contents | Rule |
|---|---|---|
| Top-left strip | goods, beavers, idle | Always. One row, about 40 px. |
| Top-right | calendar, speed, pond gauge | Always. About 300×80 px. |
| Right column, below calendar | alerts (at rest), inspector (on selection), tool card (with a tool) | One at a time. Max width 320 px. Max height: to 140 px above the bottom. Scrolls past that. |
| Bottom centre | build bar (at rest), key-hint bar + profile (dam tool) | Bar is about 56 px tall. Profile, when shown, *replaces* the bar (§5.4). |
| Bottom-left | minimap | Collapsible to a 32 px button. |
| Left edge | view rail | Icon column, about 44 px wide. |
| Centre, large windows | Colony screen, flow ledger | Docked overlay. Always leaves at least 25% of the screen width showing live map. |
| Cursor | placement tag, hover tooltip | Follows the cursor; flips sides near screen edges. |

### F3.3 Build mode

When any placement tool is active:

- **Hidden:** view rail, minimap, alerts, building list. The build bar collapses to one chip: the chosen building's icon with ✕.
- **Work rect:** the rectangle from 15% to 72% of screen width and 8% to 78% of screen height has no panels in it.
- **Camera:** when a dam/canal line is drawn or a building is placed, the camera eases so the line's midpoint sits in the work rect. It never moves while you are still dragging.
- **Messages** appear in a lane at the top-centre under the strip and fade after 3 seconds. They never share an anchor with another panel.
- **Option** "Hide HUD while placing": also hides the strip and calendar.

**Accept:** a script places each tool at 1280×720 and 1920×1080 and checks that no panel rectangle intersects the work rect.

### F4. Tooltip system

- Opens after 300 ms hover, or immediately with Alt held.
- Underlined terms in a tooltip open a nested tooltip.
- Middle-click or holding still for 1.5 s **freezes** a tooltip. The tooltip itself shows a small pin icon when frozen (the printed freeze hint is Old World's cheapest discoverability fix).
- Every tooltip on a number uses the **breakdown format**: total on top, then one row per source with its sign, sorted by size, and the hover of each row jumps the camera to the source where it has a place.
- Every interactive control's tooltip ends with its key in a keycap glyph.

### F5. Text budget (P0)

Text is a cost. Every word on screen has to earn its place, including one- and two-word labels. **Appendix A lists every player-facing string in the game today with its replacement; it is part of this spec.**

**Order of preference for any piece of information:**

1. Nothing: the information is already visible (a filled dot, a disabled button, the ghost on the grid, the thing appearing in the world).
2. Position or shape: a column, a bar, a gauge, a colour backed by a shape.
3. Icon.
4. Icon + number.
5. Number with a one-token unit (`8d`, `40%`).
6. One word.
7. Two words, only when one word would be ambiguous.
8. Anything longer goes in a tooltip, the encyclopedia or the journal, never a panel.

**Cut rules**, applied to every string:

| Cut | Example now | After |
|---|---|---|
| Labels the icon, column or position already gives | "Here", "Check", "Measured", "Crest", "Cost", "Materials" | gone |
| Section headers over a single obvious section | "WORKERS", "OUTPUT", "CONSTRUCTION", "VIEW", "TOOLS" | divider or icon |
| Units repeated on every value | "/d", "/day", "cells", "days" on each row | stated once in the tooltip; `d` only on durations |
| Parentheticals | "(to level 7.85)", "(83 under water)", "(still settling)", "(Ctrl+Z undoes)" | moved to tooltip or cut |
| Filler words and articles | "A beaver took the place", "Something is already built here" | cut, or "Occupied" |
| Repeats of what another element shows | status "1 of 2 places empty" + Workers row "1 of 2" + dots | the dots only |
| Confirmations of something visible | "Logging camp ordered at (34, 50)", "Welcomed", "Place the forager hut" | no message |
| Messages for impossible actions | "Nothing unfinished to undo", "Drag across the water first" | disable the control instead |
| Instructions | "Click to place - keeps the tool - RMB or Esc to stop" | key-hint bar glyphs, journal |
| Text close/arrow buttons | "x", "Show >", "seed -" | ✕, ⌖, ‹ › |
| Abbreviations | "stk", "stn", "bsk", "^Z" | icons (an abbreviation is worse than either the word or the icon) |
| Code formatting leaking into the UI | `{g:?}` Debug names, raw sim error strings | mapped to the short text in Appendix A |
| Excess precision | "+0.79", "crest 1.50", "22.5 log", "12.0 logs/day" | "+0.8", "1.5", "23", "12" |

**Allowed text:** the game title; names of beavers, maps and buildings (in an inspector or tooltip header); numbers; one-word labels from Appendix A where no icon is clear; tooltips (lines ≤ 6 words, ≤ 5 lines before an expander).

**Budgets** (words per element; numbers and names don't count):

| Element | Max words |
|---|---|
| Button | 0 (icon); 1 where Appendix A allows it |
| Status line | 2 |
| Alert title | 1, plus icon and number |
| Row label | 1 |
| Cursor tag | 2 |
| Message lane | 2 |
| Panel at rest (whole panel) | 6 |
| Tooltip line | 6 |

Example, dam tool card: today about 52 words ("Dam - 9 cells - crest 1.50", "Water rises +0.79 (to level 7.85)", "Enter builds - RMB or Esc to stop"…). After: 5 words (Rise, Flood, Trees, Push, Fill), everything else icons and numbers.

**Accept:** a test walks every HUD panel in each state of the scripted demo, collects the text nodes and fails if any panel exceeds its budget, any string ends in a full stop, or any string is not in Appendix A's "after" column.

---

## 4. Information access (P1)

### 4.1 Hover tooltips in the world

| Hovered | Tooltip |
|---|---|
| Beaver | name, life stage icon, role icon, current task ("carrying logs to warehouse"), mood and health faces |
| Tree / plant / mud bank / stone | amount left, regrowth per day, which workplace's area it is in |
| Ground cell | elevation, water depth, flow speed and direction, vegetation, flooded days |
| Building | icon, status line, worker dots, output per day |
| Site | progress %, what it is waiting for |
| Dam | crest, water level each side, strength %, flow over |

### 4.2 Inspectors (click)

Wire `Selection::Beaver`, `Cell`, `Tree`, `Lodge` and `Building` in `world_input`. One inspector at a time in the right column; pin (📌) keeps it open while selecting something else (pinned ones stack, max 3).

**Beaver**
- Header: name, stage icon, age, sex, family (link), partner (link), home (link).
- Job row: role icon + workplace (link) + a **change job** dropdown listing roles with open slots (§W3).
- Needs: food, rest, warmth, safety, social as bars with the threshold mark; hover shows what fills each one.
- Mood and health with breakdown tooltips.
- Skills: 8 bars with role icons.
- Follow (eye icon: camera tracks the beaver).

**Building (complete)**
- Header: icon, name, pause ⏸, priority (▲ ▼, 3 levels), pin, ✕.
- Status line: icon + 2–4 words, coloured and shaped by §8.
- **Workers** (§W2).
- **Production**: recipe line in icons (tree icon → logs icon) with measured rate per day. Only the **limiting factor** shows inline ("Walk to store: 9 cells ◆"). Every other factor and the ceiling move into a "▸ details" expander that remembers its state.
- **Work area** (gatherers): area icon button draws the rectangle and lets you drag its corners (`SetWorkArea`).
- **Storage** (stores): stock and capacity per good, haul orders in and out with add/remove (`SetHaul`).

**Site**: progress bar, materials as icon + "got / need", builders as worker dots with stepper, cancel (refunds).

**Dam**: crest, level upstream and downstream, strength with breakdown, flow over the crest, pond volume, days of summer cover, "Raise / lower crest" (P2), breach.

**Water cell / tree / lodge**: the hover data plus history (depth over 60 days for a cell; residents and capacity for a lodge).

### 4.3 Numbers that must get breakdowns

| Number | Breakdown rows |
|---|---|
| Each good's net per day | each producing building and each consuming use (eating by life stage, building sites, crafting inputs), with buildings linked |
| Food days left | stock, eaten per day by stage, gathered per day, the result |
| Beaver count | adults, yearlings, kits, elders; housed / unhoused; arrivals and deaths this season |
| Idle count | each idle beaver with why: no open slot, slot unreachable, no tool |
| Pond cover | each pond's volume against summer evaporation and seepage |
| Dam preview rows | river pushing = inflow at the line × …; fill time = volume ÷ inflow; trees drowned list; flooded buildings list |
| Mood, health | each contributing need and event |
| Output per day | each limit with its measured value (the current `building_report` factors) |
| Research xp | where each xp came from |

### 4.4 Colony screen (Tab)

One docked window with tabs; each tab's table is sortable, filterable, and clicking a row selects and shows the thing.

| Tab | Contents |
|---|---|
| **Population** | roster: name, stage, age, role, workplace, home, mood, health, top skill. Multi-select rows to reassign (§W4). Graph: population by stage over time. |
| **Jobs** | the Jobs board (§W3). |
| **Goods** | the current flow ledger, plus stock and capacity per store, and haul orders. Graph per good. |
| **Water** | one row per pond: dam, crest, level, volume, inflow, outflow, evaporation, days of cover. Graph: total stored water. |
| **Land** | forest, meadow, wetland, fish, waterfowl totals and trend; regrowth per day. |
| **Research** | the tech tree by branch: unlocked, in progress with xp bar and source breakdown, achievement unlocks with the achievement that triggers them, what each unlocks. |
| **Log** | event history, filterable by kind, each linked to its place. |

### 4.5 Map views

Each view greys the world and colours one measure, with a legend that says what the colour means. A view opens itself when a tool needs it, and closes when the tool closes.

| View | Colours | Opens itself with |
|---|---|---|
| Water (exists) | depth; flow arrows | dam, canal, lock, water wheel |
| Goods (exists) | workplace status; delivery lines | stores |
| **Vegetation** (P2) | plant food density | forager hut |
| **Forest** (P2) | tree density, regrowth | logging camp |
| **Mud & stone** (P2) | mud banks, outcrops | mud pit, quarry camp |
| **Work areas** (P2) | every gatherer's rectangle, coloured by role, overlaps hatched | any gathering building; "Edit area" |
| **Housing** (P2) | lodge reach, unhoused beavers | burrow, lodge |

### 4.6 Event log and How/Why journal

- Log: §4.4. The newest entry also shows in the message lane for 3 s.
- Journal: a lesson fires the first time a system appears (first dam, first dry summer, first idle beaver, first full store). Each lesson has a short "How" and "Why". All lessons stay in the journal (J), searchable.

---

## 5. Building and placement (P0 for layout, P1 for content)

### 5.1 Build bar

- Six icon tabs (§F2) with the key shown in the tooltip, not on the tab.
- No coloured top edges per category (breaks §8).
- **Dam moves to the Water category.** It is currently listed under Basics while the Water tab is highlighted.
- Items: thumbnail, cost as icon + number rows. Locked items are greyed with a padlock; the tooltip names the tech and how to get it (xp source or achievement, e.g. "Build your first burrow").
- Search (/) keeps synonyms. Results show unlocked first; locked results show the padlock and the route to unlock.

### 5.2 Placement feedback at the cursor

- **Cursor tag** next to the ghost: cost icons, then ✓, or ✕ with a reason of at most two words from Appendix A ("✕ Wet", "✕ Occupied").
- **No tool card for ordinary buildings.** Name, cost and verdict are on the chip and cursor tag; the footprint is the ghost. Only line tools (dam, canal) get a card.
- Ghost footprint: white (valid) or red with a ✕ pattern (invalid).
- **Key-hint bar** (tool mode only, bottom-centre, about 28 px): keycap glyphs for place, cancel, undo, crest +/−, confirm. Nothing else.

### 5.3 Dam tool card

Right column, max 320 px, no prose:

| Row | Shown as | Tooltip |
|---|---|---|
| (header) | dam icon, `9` | "9 cells" |
| crest icon ↕ | `− 1.5 +` | "Crest 0.5–3", keys |
| Rise | `+0.8` | "Level 7.9" |
| Flood | `68` | "83 under water" |
| Trees | `50` ◆ when > 0 | list |
| (no label) | log icon `23` stick icon `30` | — |
| Push | `3.5` | breakdown |
| Fill | `6d`, `∞` with no inflow | breakdown |
| building icon ■ | `2`, only when > 0 | the buildings |
| Buttons | ✓ (accent), clear icon, ✕ | keys |

### 5.4 River profile

- Docks to the bottom edge and takes the build bar's place while the dam tool is on. Full width minus the minimap, about 140 px high.
- Collapses to a 24 px strip with P.
- Legend as coloured swatches only; the title is the word "Profile" and an upstream ← icon.
- P2: hover a point to highlight that cell on the map; drag the crest line to edit it.

---

## 6. Worker model (P1)

### W0. One job model (decision needed before W1)

**Recommendation:** work slots are the truth. A beaver's role is the role of the slot it fills, or "free" when it fills none. `colony::Beaver.job` becomes derived from the slot role so skills keep growing. Guard and Scout either become slots on Guard Post / Scent Mound or are removed from the player-facing model. The abstract colony effects of Digger, Crafter and Builder must either be driven by slot work or retired.

### W1. Auto-staffing (default on)

- Free working-age beavers fill empty slots by **priority** (building priority first, then role priority from the Jobs board), then by distance.
- Building sites are slots like any other (builders), using the existing `organise` logic for material gathering.
- A beaver placed by hand is **locked** (🔒 on their dot). Auto-staffing never moves a locked beaver.
- Setting in Settings and on the Jobs board: auto-staffing on/off.

### W2. Per-building worker control

In the inspector and in the building's hover tooltip:

```
[logger icon]  ● ● ○   −  +    ▲▼  ⏸
```

- Dots = slots. Filled dots show the worker's face; click opens the beaver.
- **+** fills the next slot with the nearest free beaver. If none is free, it takes one from the lowest-priority unlocked job, and the tooltip on + says which job it will take from *before* you click.
- **−** frees the last-filled slot (that beaver becomes free).
- Right-click a dot: pick a specific beaver from a list sorted by distance and skill.
- Priority ▲▼ and pause ⏸ as above.

### W3. Jobs board (Colony screen → Jobs, or click the beaver count)

| Role | Filled / slots | − + | Priority | Output per day | Free nearby |
|---|---|---|---|---|---|

- One row per role, with its icon.
- **−** and **+** move a beaver between this role and the free pool, choosing the building by priority then distance.
- Header shows: working, free, kits, elders.
- Drag a role row up or down to set role priority (Kingdoms and Castles).

### W4. Direct orders

- Click a beaver to select; drag a box to multi-select; Shift adds.
- With beavers selected: right-click a resource → gather it (`Command::Assign` with `Assignment::Resource`); right-drag an area → gather that kind there (`Assignment::Area`); right-click a site or building → fill its free slots.
- Selected beavers show a ring; nothing else uses that ring (today white rings appear on beavers with no explanation).
- A direct order locks the beaver (W1) until the lock icon on their inspector is clicked.

### W5. Idle counter

- In the strip: idle icon + count, amber, only when > 0.
- Click: selects the next idle beaver and moves the camera to it. Shift-click: opens the Jobs board. Tooltip lists the idle beavers with why (§4.3).

### W6. In-world markers

- Unstaffed building: role icon with a ✕ badge over the roof.
- Idle beaver: idle icon over the head.
- Blocked building (no store, full, no inputs): matching status icon.
- Every alert kind in §7 has a matching marker.

### W7. Alerts get their fixes back, through the work model

Fix buttons are icons (role or building icon with + ); the tooltip says what will happen in ≤ 6 words.

| Alert | Fix button |
|---|---|
| Food runs out in N days | forager icon `+2` if a forager hut has free slots; otherwise forager-hut icon + (place) if unlocked; otherwise plant icon `+2` (free beavers gather around the lodge, `Assignment::Area`) |
| Sites without builders | builder icon `+` (fills each stalled site's slots by W2 rules) |
| Beavers idle | role icon `+N` for the highest-priority open role, or opens the Jobs board when none is open |
| Store full | warehouse icon + (place), or haul icon (opens the store's haul orders) |
| Dam weak or failed | ⌖ to the dam; builder icon `+` for repair if repair exists |
| Stranger asks to join | ✓ / ✕ |

### W8. The first ten minutes must be playable

On the working copy the start looks likely to stall: about 8 days of food, the forager hut locked until the first burrow, the idle alert offering only "Show", and no way to order gathering. **Accept:** a scripted new game on each of the four maps, using only HUD actions, reaches 30 days without starvation; the first alert's fix gets food coming in.

---

## 7. Other HUD elements (P1)

| Element | Now | Change to |
|---|---|---|
| Goods strip | "Food 6 -0.8/d lasts 8d …" with tiny colour bars | food icon 6 ▼0.8; "8 d" chip only when a good would run out within 20 days. Crafted goods join once owned. Beaver icon + count; idle chip (W5). |
| Calendar | "Spring 21 Year 1", unlabelled colour bars, "Dry summer in 70 d" | season icon + day; ribbon with season icons and today's tick; next-event chip (sun icon + "70 d", amber when it's a dry summer). Year in the tooltip. |
| Speed | "\|\| paused", "1x 2x 3x" | ⏸ ▶ ▶▶ ▶▶▶ icons. |
| Pond cover | a sentence in a box | a small gauge (fill = days of summer cover vs 90); empty with a dashed outline before the first dam. Text only in the tooltip. |
| Alerts | title + cause sentence + fix + Show | icon + number (A3), fix icon button, ⌖. Cause in the tooltip. Up to 4; more collapse into "+N". Settings: per-kind show/hide and auto-pause. |
| View rail | "VIEW / World F1 …", "TOOLS / Ledger L / Search / / Undo ^Z" | icon column: world, water, goods (P2: more). Ledger becomes the Colony screen (Tab). Search and undo move to the build bar's right end as icons. |
| Inspector | status line repeats the Workers row; "At most…" wraps; "Esc closes" | §4.2: status once, limiting factor only, details behind an expander. No footer text. |
| Minimap | caption "North up - click or drag to look" | no caption; N mark in the corner; collapse button. Markers for alerts. |
| Start menu | instruction and credit lines | Play, map picker (name and size only), Settings, Quit (A11). |

---

## 8. Visual rules

| Meaning | Colour | Backed by |
|---|---|---|
| Water | blue | only ever water |
| Commit / fix | tooth orange | button shape |
| Selected | birch white outline | outline only |
| Good / rising / working | green | ▲ or ● |
| Needs attention | amber | ◆ |
| Falling / stopped / urgent | red | ▼ or ■ |

- Each good keeps one colour everywhere, never blue.
- No other decorative colour in the data layer; category tabs are neutral.
- Every view and panel passes a greyscale check.

---

## 9. Controls (additions and changes)

| Action | Key |
|---|---|
| Colony screen | Tab |
| Jobs board | J (opens Colony → Jobs) |
| How/Why journal | H |
| Next idle beaver | . (period) |
| Collapse river profile | P |
| Freeze tooltip | middle-click, or hold still |
| Peek tooltip instantly | Alt |
| Select beavers | click, drag box, Shift adds |
| Order selected beavers | right-click resource / right-drag area / right-click building |
| Cancel tool | right-click with no beaver selected, or Esc |

Existing keys are unchanged: WASD/arrows pan, Q/E rotate, Z X C V B N categories, 1 2 3 speed, Space pause, F1–F3 views, / search, [ ] crest, Enter confirm, Ctrl+Z undo. All keys remappable (P2).

---

## 10. Build order

Each step is one task with its own acceptance check.

| Step | Pri | Task | Accept |
|---|---|---|---|
| 1 | P0 | Retained UI (F1) | §F1 |
| 2 | P0 | Icon atlas and symbol font (F2) | §F2 |
| 3 | P0 | Zones, build-mode layout, camera framing, message lane (F3) | work-rect script passes at 720p and 1080p |
| 4 | P0 | Tooltip system with breakdown format and freeze (F4) | tooltip survives 5 s of running sim; nested tooltip opens and freezes |
| 5 | P1 | One job model (W0) | Jobs board totals equal slot totals; skills still grow |
| 6 | P1 | Auto-staffing, worker stepper, Jobs board, idle counter (W1–W3, W5) | a new game staffs a placed forager hut with no clicks; + / − behave as in W2 |
| 7 | P1 | Beaver, tree, cell, lodge selection and inspectors (§4.1, §4.2) | every thing in the world opens an inspector |
| 8 | P1 | Direct orders and work-area editing (W4) | right-click and right-drag orders gather; area edits move gathering |
| 9 | P1 | Alert fixes and in-world markers (W6, W7) and the early-game check (W8) | W8 script passes on all four maps |
| 10 | P0 | Text pass: every string replaced per Appendix A, all panels (§F5, §5, §7) | the F5 budget test passes |
| 11 | P1 | Number breakdowns (§4.3) | every listed number has a breakdown tooltip |
| 12 | P1 | Colony screen tabs (§4.4) | each tab's table sorts and links to the world |
| 13 | P2 | More map views with auto-open (§4.5); journal (§4.6); profile hover/drag; remapping | — |

---

## 11. How to check it worked

- **Screen budget:** idle HUD ≤ 15% of the frame; build-mode work rect clear (step 3 script).
- **Find-the-element test** (Anno's method): time a new player on "find out why food is falling", "put two more beavers on logging", "find where the logs come from", "see how full the pond is", "find what unlocks the workshop". Target under 10 seconds each, with no text instructions on screen.
- **Greyscale check** of every view.
- **Late-game check:** 200 beavers, 60 buildings, 6 ponds, 15 alerts. Strip, alerts, Jobs board and roster stay readable.

---

## 12. Open questions for the owner

1. **W0:** retire the abstract colony jobs (Guard, Scout, Digger's dug total, Crafter's tools) or map each onto a building slot?
2. **Auto-staffing on by default?** This spec assumes yes, with hand placement locking.
3. **Repairs:** should damaged dams need builders (so the "dam weak" alert gets a "+builders" fix), or stay passive?
4. **Icon art:** commission a set, or draw a placeholder set in-house first?

---

## Appendix A. Every string, before and after

Notation: `[food]` = the food icon, `[logs]` = the logs icon, and so on. **cut** = removed with nothing in its place. **tip:** = moved into the tooltip, in the words given. Source: `crates/game/src/hud.rs`, `menu.rs`, `build_logic.rs`, `crates/sim/src/hud.rs`. Any string added later must be added here first.

### A1. Goods strip and its tooltip

| Now | After |
|---|---|
| "Food", "Logs", "Sticks", "Mud", "Stone", "Baskets", "Rope", "Tools" | `[food]` … `[tools]`; tip: the name |
| "+0.4/d", "-0.8/d" | `▲0.4`, `▼0.8` (per day stated once in the tip header) |
| "lasts 8d" | `8d` chip, only under 20 days |
| "empty" | `0` with ■ |
| "4 beavers" | `[beaver] 4` |
| "2 idle" | `[idle] 2` |
| "Food - 6 of 40 stored" | tip header: `Food 6/40` |
| "+x brought in per day (7-day average)" | tip: `Gathered +x`, one sub-row per source building |
| "-x eaten per day by N beavers" | tip: `Eaten −x`, sub-rows by life stage |
| "-x used per day (building, crafting)" | tip: `Used −x`, sub-rows per site or workshop |
| "N d until the store is empty" | tip: `Empty in 8d` |
| "ok gatherers keep up with eating" | cut (no row when nothing is wrong) |
| "To break even: +2.1 food/day" | tip: `Short 2.1` |
| "Rates appear after the first whole day." | `—` in the rate |

### A2. Calendar, speed, pond

| Now | After |
|---|---|
| "Spring 21" | `[spring] 21` |
| "Year 1" | tip: `Year 1` |
| "‖ paused", "‖", "1x", "2x", "3x" | ⏸ ▶ ▶▶ ▶▶▶ |
| "Dry summer in 70 d" | `[sun] 70d`, amber |
| "Dry summer: ponds evaporate fastest - N d left" | `[autumn] Nd` (days to the next season); tip: `Dry summer` |
| "Winter: ice, little inflow - spring in N d" | `[spring] Nd`; tip: `Winter: low inflow` |
| "Ponds hold N: X of 90 summer days" | gauge; tip: `Ponds N`, `Cover Xd / 90d` |
| "ok", "short" | gauge colour + ◆ |
| "No dam yet: dam the river to store water" | dashed empty gauge; tip: `No ponds` |

### A3. Alerts

| Now | After |
|---|---|
| "!!", "!" | ■, ◆ (shape + colour) |
| "Food lasts N days" | `[food] Nd` |
| "No food in store" | `[food] 0` ■ |
| "N beavers eat X/day, gatherers bring Y/day" | tip: `Eaten −X`, `Gathered +Y` |
| "Place a forager hut" | `[forager hut]+`; tip: `Place forager hut` |
| "Place a warehouse" | `[warehouse]+`; tip: `Place warehouse` |
| "Show >" | ⌖ |
| "A dam could not be raised" | `[dam] ✕`; tip: the cleaned sim reason (A7) |
| "Dam at N% strength" | `[dam] N%` |
| "Water runs over its crest or it is ageing; it breaks at 0%" | tip: `Overtopping −x`, `Age −y` |
| "N sites without builders" | `[site] N` |
| "Every working beaver is busy" / "N beavers are idle" | tip: `Free N` |
| "N beavers idle" | `[idle] N` |
| "No job or workplace slot" | cut |
| "Logs storage full" | `[logs]` + full icon |
| "40 of 40 stored; new loads are lost" | tip: `40/40`, `Lost x` |
| "Iris asks to join" | `[stage icon] Iris` |
| "An adult who can work at once" / "A young beaver" | cut (stage icon) |
| "Accept" | ✓, plus ✕ |
| "Paused: {t} (auto-pause, change it in Settings)" | ⏸ on the alert itself |

### A4. Build bar, items, search

| Now | After |
|---|---|
| "Basics", "Water", "Forest", "Building", "Craft", "Transport", "Defence" | category icons; tip: name + key |
| "Z" … "N" on the tabs | tip only |
| "per 6 cells: 15 log 20 stk" | `[logs]2.5 [sticks]3.3`; tip: `Per cell` |
| "log", "stk", "stn", "bsk", "food", "rope", "tool" (cost text) | icons |
| "free" | cut (no cost row) |
| "Locked: needs Foraging" | 🔒; tip: `Foraging`, `[burrow] first burrow` |
| "Locked: Stonework 3/6 xp" | 🔒 + progress bar; tip: `Stonework 3/6` |
| "Search: food_" | search icon + the typed text |
| "Enter picks the first - Esc closes" | cut |
| "No building matches" | `None` |

### A5. Tool cards, cursor, key hints

| Now | After |
|---|---|
| building name (tool card title) | the chip's icon; tip: name |
| "Materials (hauled in by builders)" + cost | cost icons on the cursor tag |
| "Footprint" "W x H cells" | cut (the ghost shows it) |
| "Here" "can be placed ok" | ✓ at the cursor |
| "Here" + reason | ✕ + A7 reason at the cursor |
| "Here" "point at the ground" | cut |
| "Click to place - keeps the tool - RMB or Esc to stop" | key-hint bar glyphs |
| "Done" | ✕ |
| "Canal - N cells" | `[canal] N` |
| "Drag from water across the land to draw the canal" | cut; journal lesson on first use |
| "Cost" | cut |
| "Check" "ready ok" | cut (✓ enabled) |
| "Dig canal", "Build dam" | ✓ (accent) |
| "Clear" | clear icon |
| "Enter digs - RMB or Esc to stop", "Enter builds - RMB or Esc to stop" | key-hint bar glyphs |
| "Dam - N cells - crest 1.50" | `[dam] N` |
| "Crest" + "0.5-3 - [ ] or Ctrl+wheel" | ↕ `− 1.5 +`; tip: `Crest 0.5–3` + keys |
| "Drag across the river to draw the dam. The pond it makes is drawn before you build." | cut; journal lesson |
| "Water rises" "+0.79 (to level 7.85)" | `Rise +0.8`; tip: `Level 7.9` |
| "Newly flooded" "68 cells (83 under water)" | `Flood 68`; tip: `83 under water` |
| "Trees drowned" "50" | `Trees 50` |
| "Materials" "22.5 log 30 stk" | `[logs]23 [sticks]30` |
| "River pushing" "3.5 / day" | `Push 3.5` |
| "Fills in" "6 days" / "never (no inflow)" | `Fill 6d` / `Fill ∞` |
| "! Floods N building(s)" | `[building] N` ■ |
| "Pond" "not a valid dam line" | ✕ `No pond` |

### A6. River profile

| Now | After |
|---|---|
| "River profile" | cut (profile icon on the collapse handle) |
| "side view along the river, upstream on the left" | flow arrow → on the axis |
| "No river near here" | `No river` |
| legend "water", "added by this dam", "dams", "your line", "nearest river" | swatches; tip: `Water`, `New`, `Dams`, `Line` |
| "height 0.4-8.1" | axis tick numbers |

### A7. Placement and command errors (cursor tag, message lane)

| Now | After |
|---|---|
| "Locked: X needs N xp (have M)" | 🔒 `X` |
| "Outside the map" | `Off map` |
| "Something is already built here" | `Occupied` |
| "Needs dry ground" | `Wet` |
| "Must touch water" | `No water` |
| "Needs a free water cell next to a finished dock" | `Needs dock` |
| "Not enough Logs: need 22, have 12" | `[logs] 12/22` |
| "Nothing drawn yet" | cut (✓ disabled) |
| "Too long (max 40 cells)" | `Max 40` |
| any raw sim error (`PlaceError::Sim`, `StoresError`, "job X is disabled", Debug names) | mapped to a ≤ 2-word entry in this table; an unmapped error is a bug |

### A8. Messages (message lane)

| Now | After |
|---|---|
| "{building} ordered at (x, y) (Ctrl+Z undoes)" | cut |
| "Dam of N cells ordered" / "Canal of N cells ordered" | cut |
| "Drag across the water first" | cut (✓ disabled) |
| "Undid the {building}, N logs back in store" | `↶ [building]`; refund floats as `+[logs]N` at the store |
| "Undid the canal, materials refunded" | `↶ [canal]` |
| "Cannot undo: {e}" | `↶ ✕` + A7 reason |
| "Nothing unfinished to undo" | cut (undo disabled) |
| "A beaver took the place" | cut (dot fills) |
| "Nobody is idle" | cut (W2: + takes from the lowest-priority job; its tip says which) |
| "Place the {building}" | cut (chip appears) |
| "Welcomed" | cut (count rises; log entry) |

### A9. Inspector

| Now | After |
|---|---|
| "x" | ✕ |
| "Gone" | cut (inspector closes) |
| "Turning at N%" | `[wheel] N%` |
| "Still: no current here" | `No current` |
| "Idle: nobody works here" | `No workers` |
| "Working, 1 of 2 places empty" | `Working` (dots show the gap) |
| "Waiting: no builders assigned" | `No builders` |
| "Building - waiting for logs, sticks" | `Needs [logs][sticks]` |
| "Working", "Building", "Planned" | keep |
| "CONSTRUCTION", "WORKERS", "OUTPUT" | cut (dividers) |
| "Labour" "1.2 / 3.0" | `[hammer] 1.2/3` |
| "Done" "40%" | progress bar with `40%` |
| per-good "got / need" with good names | `[logs] 12/23` |
| "Logger", "Forager", "Mud digger", "Quarrier", "Builder", "Hauler", "Crafter", "Scholar" | role icons; tip: the name |
| "empty" | hollow dot |
| "+ idle beaver", "-" | `+`, `−` |
| "Measured" "0.8 sticks + 0.5 logs /day" | `[sticks]0.8 [logs]0.5` |
| "nothing made yet" | `—` |
| "(still settling)" | `~` before the number; tip: `Settling` |
| "Workers" "1 of 2", "Crafters" "1 of 2" | cut (dots) |
| "Trees / Food plants / Mud banks / Stone outcrops in its area" | `[tree] 39`, `[plant] n`, `[mud bank] n`, `[outcrop] n` |
| "Walk to the nearest store" "3 cells" | `[walk] 3` |
| "Store for its goods" "none: loads go only to sites" | `[store] ✕`; tip: `No store` |
| "Recipe" "none chosen" | `No recipe` |
| "{Good:?} in store (a batch needs N)" | `[good] have/N` |
| "Room for {Good:?}" | `Room [good] N` |
| "Water wheel power" "+20%, shared by workshops" / "none" | `[wheel] +20%` / `[wheel] 0` |
| "At most" "12.0 logs/day, every slot filled, no walking" | `Max [logs]12`; tip: `All slots, no walking` |
| "Esc closes" | cut |

### A10. View rail, flow ledger, minimap

| Now | After |
|---|---|
| "VIEW", "TOOLS" | cut |
| "World  F1", "Water  F2", "Goods  F3" | view icons; tip: name + key |
| "Ledger  L", "Search  /", "Undo  ^Z" | icons; tip: name + key |
| "Flow ledger" | tab name `Goods` (Colony screen) |
| "per day, 7-day average - L or Esc closes" | cut; column-header tips |
| "GOOD" | cut (icon column) |
| "STORED", "IN", "OUT", "NET", "LASTS" | `Stock`, `In`, `Out`, `Net`, hourglass icon |
| "STOCK, LAST 60 DAYS" | `60d` |
| "MADE BY" | `Sources` |
| "No workplace has produced yet. Beavers gathering by order…" | `—` |
| "WATER", "Dam cells", "Ponds behind dams", "Summer cover", "no dam yet", "N water on M cells" | Water tab (§4.4) with icon headers; `—` when empty |
| "North up - click or drag to look" | cut; N mark |

### A11. Start menu, pause menu, settings

| Now | After |
|---|---|
| "Beaver Colony" | keep |
| "Shape the river, feed the family, build a colony." | cut |
| "New game" | `Play` |
| "Algonquin Park, Ontario, Canada" | `Algonquin Park`; tip: region |
| "Real terrain, 2.6 km across, 45.55 N 78.42 W" | `2.6 km`; tip: coordinates |
| "seed -", "seed +" | ‹ › around the seed number |
| "The valley behind this menu is the map you will play." / "New game builds this map (it takes a few seconds)." | cut; progress bar while building |
| "The game starts paused: press Space or 1, 2, 3 to let time run." | cut; ⏸ pulses on first start; journal lesson |
| "Elevation: AWS Open Data Terrain Tiles (…)" | Settings → About |
| "Paused" | cut |
| "{season} {day}, year {y} - N beavers - seed S" | cut (the HUD shows it) |
| "Resume", "Settings", "Quit" | keep |
| "Restart this map" | `Restart` |
| "Quit to title" | `Menu` |
| "Quit game" | `Quit` |
| "There is no saving yet: restarting or quitting loses this colony." | confirm on Restart, Menu, Quit: `Lose colony?` ✓ ✕ |
| "Esc resumes." | cut |
| "INTERFACE SIZE" | `UI size` |
| "ALERTS" | `Alerts` |
| "Pause when an urgent problem appears" + "On"/"Off" | `Auto-pause` + toggle |
| "CONTROLS" | `Keys` |
| control rows ("Select, place, draw a dam", "Orbit / pan / rotate / zoom", …) | one word each: `Select`, `Cancel`, `Orbit`, `Pan`, `Rotate`, `Zoom`, `Build`, `Crest`, `Views`, `Colony`, `Search`, `Undo`, `Pause`, `Speed`, `Menu` |
| "Back" | ← |
