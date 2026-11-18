# Beaver interface spec: wireframes

**Player goal:** found a lodge, wall a river with dams, and tell the family what to gather and build.
Look at section 1 first; every later section is one screen state.

**Rule for this spec:** every element has a **Trace** to a convention **C1–C12** in `research/city-builder-interfaces.md` (finding IDs like `[2-RW]` are in that file and in `research/city-builder-interfaces-trace.md`). Where the research gives no convention for something (see section 11), it is **not designed here**; it is listed as a gap. Confidence is the research's: *medium* at best, mostly *low*.
Data shown in the wireframes (goods, costs, flood numbers) comes from the sim (`crates/sim/src`); the sim supplies *what* is shown, the conventions decide *where and how*. Colours come from `crates/game/src/palette.rs`. Nothing is taken from the earlier (deleted) interface or its specs; I did not open them.

---

## 1. Full screen (the glance)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ [A] Logs 42  Sticks 30  Mud 18  Stone 6  Food 55 (+3/d)  ♥78%  Pop 7         │
│     Autumn · Day 34              ‖ ▶ ▶▶ ▶▶▶                        🔔 3     │
├──────────────────────────────────────────────────────────────┬───────────────┤
│                                                              │ [D] ALERTS    │
│                                                              │ ⚠ Food low (1)│
│                                                              │ ⚠ Flooded (2) │
│                    [ WORLD VIEW ]                            │ ★ Wanderer (1)│
│                                                              │ Journal ▸     │
│                                                              │               │
│                                                              │               │
│ ┌──────────────────────────────┐                             ├───────────────┤
│ │ [C] SELECTION PANEL          │                             │ [E] MINIMAP   │
│ │ (shown only when selected)   │                             │ ┌───────────┐ │
│ ├──────────────────────────────┤                             │ │ ~river ▭  │ │
│ │ [B] Home Work Water Store    │                             │ └───────────┘ │
│ │     Orders        (B opens)  │                             │ ⊕ ⊖           │
│ └──────────────────────────────┘                             │               │
└──────────────────────────────────────────────────────────────┴───────────────┘
```

| Region | Place | Trace |
|---|---|---|
| A Resource bar | top-left goods; time and speed in the same bar | C1: resource list top-left [1-RW]; time and speed in the same top bar [1-AN] |
| B Build menu | bottom-left dock | C2: RimWorld Architect bottom-left [2-RW]; Anno hammer bar at the left of the bottom bar [2-AN] |
| C Selection panel | bottom-left, stacked above the build tabs and sharing their corner | C7: RimWorld inspect pane shares the bottom-left corner with the Architect [4-RW]; Ludo puts Anno's panel bottom-left [4-AN, sources disagree] |
| D Alerts | top-right | C11: RimWorld alerts top-right [8-RW]; list form from Timberborn [8-TB] |
| E Minimap and zoom | bottom-right | C12: Anno minimap bottom-right [7-AN] |
| Bell with count in bar | top bar, right | C11: Anno bell on the right of the top bar [8-AN, Ludo only] |

Note: positions are *not* themselves conventions (the research says so for C2 and C7). They are chosen to follow where the sourced games put the same element, and each row names which.

---

## 2. Resource bar

```
 Logs 42   Sticks 30   Mud 18   Stone 6   Food 55 +3/d   ♥ 78%   Pop 7
 Autumn · Day 34                          ‖  ▶  ▶▶  ▶▶▶                 🔔 3
```

| Element | Behaviour | Trace |
|---|---|---|
| Goods chips | stockpiled goods only (a good appears once the colony holds some), one chip each | C1: RimWorld lists stockpiled items only [1-RW]; Timberborn inventory [1-TB] |
| Per-day change on Food | `+3/d` beside the count | C1: Anno shows income per minute [1-AN] |
| Chip hover | per-good detail (stored, capacity) | C1: Timberborn per-good detail [1-TB] |
| Mood chip and Pop | mood % and population | C1: Timberborn Well-Being average and population [1-TB] |
| Mood colour | leaf green (`LILY`) satisfied, straw (`STRAW`) mixed, rust (`AUTUMN_RUST`) poor; number always shown too | C1 (low): satisfied needs in green [1-TB] |
| Season and day | text under the goods | C1: Timberborn calendar [1-TB] |
| Speed buttons | pause and three speeds; keys `P`, `1`, `2`, `3` | C1: Anno P and 1/2/3, controls in the top bar [1-AN] |

## 3. Build menu

```
 Closed:   [ Home ][ Work ][ Water ][ Store ][ Orders ]            (B)

 Open (Water):     ┌────────┐┌────────┐┌────────┐┌────────┐┌────────┐
 submenu centred   │  Dam   ││ Canal  ││  Lock  ││  Dock  ││ Barge  │
 above the tabs    └────────┘└────────┘└────────┘└────────┘└────────┘
 [ Home ][ Work ][▣Water][ Store ][ Orders ]
```

| Group | Tools (from `BuildingKind` and commands) | Category in the games |
|---|---|---|
| Home | Lodge, Burrow, Elders' den, Guard post, Scent mound | Housing [2-TB][2-BA] |
| Work | Logging camp, Forager hut, Mud pit, Quarry camp, Mill, Workshop, Water wheel | Food / Wood / Production [2-TB][2-RW] |
| Water | Dam, Canal, Lock, Dock, Barge | Water [2-TB] |
| Store | Food cache, Storehouse, Warehouse | Storage [2-TB][2-BA] |
| Orders | Gather, Cancel gather, Demolish | Orders tab [2-RW]; Demolish and removal tools [2-TB][2-BA] |

| Element | Trace |
|---|---|
| Categorised tabs | C2 medium [2-TB][2-RW][2-AN][2-BA] |
| `B` opens the menu | C2 low [2-AN] |
| Submenu shown centred above the tab row | C2 low [2-TB] |
| `Ctrl+wheel` cycles groups, `Shift+wheel` cycles tools | C2 low, one game [2-TB] |
| Tool stays active until cancelled; right-click cancels | C5 low [3-CS] |

The selection panel (section 4) and the build tabs share one corner; opening a submenu draws it above the tabs and the panel steps up, as RimWorld's panes do [4-RW].

## 4. Selection panel

```
┌─────────────────────────────────────────────┐
│ Logging camp                       ⊙Focus ✕ │
│ Workers 2/3   [🦫 Hazel][🦫 Moss][ + ]      │
│ Priority  ○ ○ ● ○ ○   (disabled: not in sim)│
│ Stock: Logs 14                [Demolish]    │
└─────────────────────────────────────────────┘
 Several beavers selected:  "4 beavers"   [Gather ▾]  [Stand down]
 Several lodges selected:   "3 Lodges"
 Site under construction:   progress bar in place of workers
 Wanderer selected:         "A wanderer asks to join"  [Accept] [Turn away]
```

| Element | Trace |
|---|---|
| Opens on selection with name and settings | C7 low [4-TB] |
| Header actions: Focus, close, demolish | C7/C12 [4-TB][7-TB] |
| Worker list and `[ + ]` to fill a slot (`FillSlot`, `EmptySlot`) | C7: Timberborn worker count and list [4-TB] |
| 5-level workplace priority dots | C10 medium, 5 levels [4-TB][6-TB]. The sim has no priority today, so the row is drawn disabled and must not be built before the sim supports it |
| Count when several same-kind are selected | C7 low [4-RW] |
| Job buttons for selected beavers | C10 model (c), profession assignment [6-BA] (weak); see section 9 |
| Join request answered in the panel | C7: a selected object's panel carries its actions [4-TB]; the request itself arrives as an alert (section 5) |

## 5. Notifications

```
 [🔔 3] top bar          ALERTS (top-right)
                          ⚠ Food low (1)     click: jump to target, click again: next
                          ⚠ Flooded (2)
                          ★ Wanderer (1)     click: jump to the wanderer; panel shows Accept / Turn away
                          Journal ▸          recent events, newest first
```

| Element | Trace |
|---|---|
| List on the main screen | C11 medium [8-TB][8-FA][8-RW][8-AN] |
| Count in brackets per alert | C11 low [8-TB] |
| Click cycles through targets | C11 low [8-TB] |
| Click moves the camera to the target | C12 "camera action that moves to a selected object" [7-TB][7-RW]. The research does not establish that alerts do this (C11 says so), so this applies C12 to the alert's target |
| Journal of recent events | C11 low [8-TB] |
| Information alerts fade after 5–30 s; warnings stay until fixed | C11 low [8-FA] |
| Bell with total count in the top bar | C11 low [8-AN] |
| Colour: warning rust, info `WATER_SHALLOW`, good news leaf green, plus a glyph | C1 low (colour marks state) [1-TB][1-CS] |

## 6. Camera controls

```
 Minimap (bottom-right)       ⊕ ⊖
 Wheel: zoom      Tab or M: full map      F or ⊙Focus: centre on selection
```

| Control | Trace |
|---|---|
| ⊙ Focus button in the panel header, key `F` | C12 medium [7-TB][7-AN][7-RW] |
| Minimap bottom-right | C12 low [7-AN] |
| Wheel zoom and ⊕ ⊖ buttons | [7-AN][7-FA] (weak) |
| `Tab` / `M` toggles the full map | C12 low [7-FA] |
| Clicking a beaver chip in a worker list jumps to it | C12 [7-RW] |

Pan and rotate: no convention in the research (gap). Not designed here; see section 11.

## 7. Settling (start of game)

Settling is placing the first building, so it uses the placement convention rather than a separate screen. `Command::Settle { cell }` founds the colony.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ [A] resource bar (empty)                                      [D] alerts     │
│                                                                              │
│                  [ WORLD VIEW, river visible ]                               │
│                           ╭────╮                                             │
│                           │ ⌂  │  ← translucent lodge ghost follows cursor   │
│                           ╰────╯    green = settle accepted, red = rejected  │
│                                                                              │
│ ┌────────────────────────────┐                                [E] minimap   │
│ │ [B] Settle   (only tool)   │                                              │
│ └────────────────────────────┘                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

| Element | Trace |
|---|---|
| Translucent ghost, green when valid, red when blocked | C3 low [3-TB] |
| Click places it; right-click or `Esc` cancels | C5 low [3-CS] |
| Same screen layout as play (build dock bottom-left, alerts, minimap) | C1, C2, C11, C12 as in section 1 |

The text explaining *why* a cell is rejected has no source in the research and is not drawn; see section 11.

## 8. Placing a dam with flood preview

A dam is a drawn line (`OrderDam { cells, height }`), like a road: click start, click end [3-CS].

```
 Step 1: draw        click start ──── click end         right-click: cancel
     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░        ghost dam: green = buildable,
     ░░░  river  ▓▓▓▓▓▓▓▓▓▓  ░░░░░░        red segments = blocked
     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░

 Step 2: plan card (bottom-left, in the selection panel's place); updates live
     ┌ Dam plan · 6 cells · height 1.0 ─────────┐
     │ Pond rises +0.8     Fills in ~4 days     │
     │ Cost 12 logs · 18 sticks · 9 mud         │
     │ Floods: 14 trees · 1 building ⚠          │
     │ River pushes on dam: strong              │
     │ PageUp / PageDown: height   [Order] [✕]  │
     └──────────────────────────────────────────┘
```

Map overlay while the plan is open:

| Overlay | Look |
|---|---|
| Cells that would flood | translucent `WATER_SHALLOW` tint, dashed `FOAM` edge |
| Trees that would drown | small rust ring |
| Buildings that would flood | rust outline with ⚠ |
| Ghost dam | `DAM_MUD` fill, green or red outline |

| Element | Trace |
|---|---|
| Line drawn by click start, click end | C6/C5 low [3-CS] |
| Ghost green when valid, red when blocked | C3 low [3-TB] |
| Placement is a plan; nothing is built until **Order** | C4 low [3-DF][3-RW]: plans are ignored by workers until converted |
| `PageUp` / `PageDown` change height | [3-CS] (Cities: Skylines uses them for road height) |
| Right-click cancels | C5 low [3-CS] |
| Card sits in the corner the selection panel uses | C7 [4-RW] shared corner; the card carries the plan's settings as an object panel does [4-TB] |
| Flood overlay and card numbers | The convention is the *preview*: a plan shown before commitment (C3, C4). What the preview contains (`rise`, `fill_days`, `cost`, `flooded_trees`, `flooded_buildings`, `pressure`, `flooded` of `dam_preview::DamPreview`) is sim data. The research documents no game with a flood preview, so only the preview mechanism is a convention |
| Colour marks plus a word or glyph | C1 low (colour marks state) [1-TB] |

Submitting creates a construction site the family builds; the pond appears when it finishes. A finished dam is removed with Demolish (`BreachDam`) from the panel header [4-TB].

## 9. Drag gather and job assignment

```
 Orders group → Gather (G).   Kind:  [Trees] [Plants] [Mud] [Stone]
 ┌ drag a rectangle on the map ┐
 │ ┌┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┐          │   marked cells get a pale tint
 │ ┆ 🌲 🌲  🌲   🌲  ┆          │
 │ └┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┘          │   Cancel gather: drag the same way to clear
 └─────────────────────────────┘
```

| Element | Trace |
|---|---|
| Gather sits in the Orders group; one order per kind | C2, [2-RW] Orders tab (Chop Wood, Harvest) [5-RW] |
| Area is drag-selected as a rectangle | C8 low [5-DF][5-CS] |
| Marked cells tinted until worked | [3-DF] designations are drawn on the map and carried out later |
| Each area order has a matching removal tool (Cancel gather) | C8 medium [5-DF][5-RW][5-TB] |
| Sim: `Assign { who, to: Area(kind, area) }` | data |

**Job assignment**

```
 Select beavers (click or drag a box) → panel: "4 beavers"  [Gather ▾]  [Stand down]
 Open a workplace → [ + ] fills a slot          Panel: 🦫 Hazel   Doing: Collect Trees
```

| Element | Trace |
|---|---|
| Assign beavers to a job from a panel | C10 model (c) [6-BA] (weak) |
| Fill workplace slots (`FillSlot`) | C10 model (b), per-workplace [4-TB][6-TB] |
| Selected count | C7 low [4-RW] |
| Jobs available (from sim): Collect by kind, Build, Haul, Craft, Study | data |
| No per-worker grid | C10 (a) [6-RW][6-DF] is not used: the sim has no per-worker priorities |

---

## 10. Look (this game, no copied art)

This is the task brief ("in the look of this game"), not an interface convention.

| Part | Rule |
|---|---|
| Surfaces | cream panels (`CUT_WOOD` at about 92% opacity), `INK` text, rounded corners, `BARK` outlines |
| Accents | selected tab and tool `FUR`; valid `LILY`; warning `AUTUMN_RUST`; info `WATER_SHALLOW` |
| Shadows | `PLUM`-mixed, never black, as in the world |
| Icons | drawn fresh in this game's soft style; emoji in wireframes are placeholders |
| Type | one rounded sans, tabular figures |

No art from the researched games is used; they are cited only for behaviour.

## 11. Gaps (no convention, so not designed)

| Gap | Why | Needed from |
|---|---|---|
| Pan and rotate defaults | research did not source them (C12) | research or playtest |
| Workplace priority levels, "never", tie-break | C10 low; sim has none | sim |
| Why `Settle` rejects a cell | no source for settle feedback | sim |
| Cost shown in the build menu tile | no source in research; shown only on dam plan | research |
