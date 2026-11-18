# Beaver interface spec: wireframes

**Player goal:** found a lodge, wall a river with dams, and tell the family what to gather and build.
Look at section 1 first; every later section is one screen state.

Sources: conventions **C1–C12** in `research/city-builder-interfaces.md` (finding IDs like `[2-RW]` are in that file and in `research/city-builder-interfaces-trace.md`). The research reaches *medium* confidence at best, so a "Trace" cell says which convention backs an element and its confidence. **"Own"** means the research does not cover it; it is a decision of this spec for this game, not a convention.
Nothing here is taken from the earlier (deleted) interface or its specs; I did not open them. Facts about the sim (commands, goods, dam preview fields) come from `crates/sim/src`. Colours come from `crates/game/src/palette.rs` (world palette), not from any UI code.

---

## 1. Full screen (the glance)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ [A] RESOURCE BAR                                                             │
│ 🪵Logs 42  Sticks 30  Mud 18  Stone 6  Food 55 (+3/d)   ♥ Mood 78%  Pop 7    │
│ ───────────────────────────────────  Autumn · Day 34 · ‖ ▶ ▶▶ ▶▶▶  [🔔 3]    │
├────────────────────────────────────────────────────────────┬─────────────────┤
│                                                            │ [D] ALERTS      │
│                                                            │ ⚠ Food low (1)  │
│                                                            │ ⚠ Flooded (2)   │
│              [ WORLD VIEW ]                                │ ★ New arrival   │
│                                                            │ ┈ Log ▾        │
│                                                            ├─────────────────┤
│                                                            │ [E] MINIMAP     │
│                                                            │  ┌───────────┐  │
│                                                            │  │ ~~river   │  │
│                                                            │  │  ▭ view   │  │
│                                                            │  └───────────┘  │
│                                                            │ ⊕ ⊖  ⟳  ⌖ Focus │
├─────────────────────────────┬──────────────────────────────┴─────────────────┤
│ [C] SELECTION PANEL         │ [B] BUILD MENU                                 │
│ (empty until something is   │ [Home][Work][Water][Store][Clear]   B = open   │
│  selected)                  │ ┌────┐┌────┐┌────┐┌────┐┌────┐                 │
│                             │ │tool││tool││tool││tool││tool│                 │
│                             │ └────┘└────┘└────┘└────┘└────┘                 │
└─────────────────────────────┴────────────────────────────────────────────────┘
```

| Region | Place | Trace |
|---|---|---|
| A Resource bar | top, full width; goods left, time and speed right | C1 (figures on main screen, med.); time in the same bar as in Anno [1-AN] |
| B Build menu | bottom, right of the selection panel | C2 (categories, med.); position: Own (research says positions differ) |
| C Selection panel | bottom-left | RimWorld uses bottom-left [4-RW]; C7 says position is *not* a convention, so this is Own |
| D Alerts | top-right, under the bar | C11 (alerts on main screen, med.); position Own (RimWorld top-right [8-RW]) |
| E Minimap and camera buttons | right, under alerts | C12 (overview map, low; focus to selection, med.) |

---

## 2. Resource bar

```
 🪵 Logs 42   ⌇ Sticks 30   ◍ Mud 18   ◆ Stone 6   ❀ Food 55 +3/d   ♥ 78%   🦫 7
 ──────────────────────────────────────────────────────────────────────────────
                                    Autumn · Day 34     ‖  ▶  ▶▶  ▶▶▶     🔔 3
```

- One chip per good in the sim: Logs, Sticks, Mud, Stone, Food (the goods list in `buildings.rs`). Baskets, Rope and Tools show only once the colony owns any (keeps the bar short early).
- Chip hover: per-good detail (stored / capacity, per-day change). Food shows a per-day delta; this reuses the idea of income per minute [1-AN].
- Needs coloured: the Mood chip is leaf green (`LILY` #4E9A52) when the family's needs are met, straw (`STRAW` #D9C27A) when mixed, rust (`AUTUMN_RUST` #D9814E) when poor. Research: green for satisfied needs [1-TB], low confidence (C1). Colour is never the only cue: the chip also shows the number.
- Time: season name and day; speed buttons with keys `Space` pause, `1`/`2`/`3` speed (Anno uses P and 1/2/3 [1-AN]; weak).
- Own: population count and the beaver glyph.

| Element | Trace |
|---|---|
| Goods chips on screen | C1 [1-TB][1-RW][1-AN] medium |
| Needs-state colour | C1 [1-TB] low |
| Time and speed in the bar | [1-AN] weak |
| Season label | [1-TB] (calendar) |

## 3. Build menu

```
 [Home][Work][Water][Store][Clear]                 Ctrl+wheel: group · Shift+wheel: tool
 ┌────────┐┌────────┐┌────────┐
 │ Lodge  ││ Burrow ││ Scent  │      Water group shown:
 │ 12 🪵  ││ 6 🪵   ││ mound  │      ┌────────┐┌────────┐┌────────┐┌────────┐
 └────────┘└────────┘└────────┘      │  Dam   ││ Canal  ││  Lock  ││ Wheel  │
                                      └────────┘└────────┘└────────┘└────────┘
```

| Group | Tools (from `BuildingKind`) |
|---|---|
| Home | Lodge, Burrow, Elders' den |
| Work | Logging camp, Forager hut, Mud pit, Quarry camp, Mill, Workshop, Water wheel |
| Water | Dam, Canal, Lock, Dock, Barge |
| Store | Food cache, Storehouse, Warehouse |
| Clear (orders, see section 8) | Gather, Cancel gather, Demolish |

- Open with `B` [2-AN], groups by Ctrl+wheel and tools by Shift+wheel [2-TB] (low confidence, one game each). A hover card shows cost and footprint, and whether the site must be dry or touch water (`Site` in `buildings.rs`). Guard post and scent mound sit under Home.
- Placement stays in the tool until `Esc` or right-click (right-click cancels [3-CS]).
- Trace: categories C2 (medium); hotkeys and scroll cycling C2 (low). Exact group membership is Own.

## 4. Selection panel

```
┌─────────────────────────────────────────────┐
│ Logging camp                       ⊙Focus ✕ │
│ Workers 2/3   [🦫 Hazel][🦫 Moss][ + ]      │
│ Gathers: Trees   Area: 8×6  [Edit area]     │
│ Priority  ○ ○ ● ○ ○   (3 of 5)              │
│ Stock: Logs 14                              │
│ [Demolish]                                  │
└─────────────────────────────────────────────┘
 Several beavers selected:  "4 beavers"  [Gather ▾] [Stand down]
```

- Shows name, worker slots with who fills them (`FillSlot`/`EmptySlot`), the work area (`SetWorkArea`), stock. Header actions: Focus and close, plus demolish [4-TB] (low).
- Selecting several of one kind shows a count [4-RW] (low).
- Priority: 5-level dot row per workplace, as in Timberborn [6-TB]; C10 medium for "a small set of levels". **Sim note:** the sim has no workplace priority today. The control is shown disabled with the tooltip "not yet" until the sim supports it; do not build the dots before then.
- Site under construction: progress bar (`SiteProgress`) in place of workers.

## 5. Notifications

```
 [🔔 3] ──►  ⚠ Food low (1)      click: jump to the store, click again: next
             ⚠ Flooded (2)      click: first flooded building, again: next
             ★ A wanderer asks to join   [Accept][Turn away]
             Journal ▸ (recent events)
```

- Alert list with a count in brackets; clicking cycles through targets [8-TB] (low, one game). Whether a click moves the camera is not established by the research; this spec says it does (Own).
- Wanderer requests map to `AnswerJoin` and carry the two buttons inline (Own).
- A Journal lists the last events [8-TB]. Alerts fade by type like Factorio's timed alerts (5–30 s) only for the information kind; warnings stay until fixed [8-FA] (low).
- Colour: warning = rust, info = shallow water cyan (`WATER_SHALLOW`), good news = leaf green. Always with a glyph.
- Trace: C11 medium for being on-screen; the rest low.

## 6. Camera controls

```
 Minimap (right):  drag the ▭ to move the view
 ⊕ ⊖  zoom      ⟳ rotate 90°      ⌖ Focus on selection
 Wheel: zoom · Middle-drag or WASD: pan · Q/E: rotate · F: focus · Tab: toggle full map
```

| Control | Trace |
|---|---|
| Focus on selection (button and `F`) | C12 [7-TB][7-AN][7-RW] medium |
| Minimap | C12 [7-AN] low |
| Wheel zoom, +/- buttons | [7-AN][7-FA] weak |
| Tab toggles map | [7-FA] single |
| Pan and rotate keys | **Own**: the research did not source defaults |

## 7. Settling (start of game)

The sim opens with no colony; `Command::Settle { cell }` founds it, rejected for an unusable cell.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  Choose where your family settles                                            │
│                                                                              │
│          [ WORLD VIEW, river visible, no resource bar yet ]                  │
│                         ╭────╮                                               │
│                         │ ⌂  │  ← ghost lodge follows the cursor             │
│                         ╰────╯                                               │
│                                                                              │
│  ┌ Site check ─────────────────────────────┐                                 │
│  │ ✔ Dry ground   ✔ Water within 6 cells   │   [Settle here]   [Cancel]      │
│  │ ✔ Trees nearby ✖ Too steep              │                                 │
│  └─────────────────────────────────────────┘                                 │
└──────────────────────────────────────────────────────────────────────────────┘
```

- Ghost is green when `Settle` would be accepted and red when not, with the failing reason written out [3-TB] (low); the reason list is Own.
- Clicking confirms in one step: the button and a second click on the same cell both settle. After settling, regions A–E fade in.
- Settling is a choice of place, not a menu; research has no source for settling screens. **Own.**

## 8. Placing a dam with flood preview

Dam is placed by drawing a line (`OrderDam { cells, height }`), the way roads are drawn in Cities: Skylines: click start, click end, right-click cancels [3-CS] (low). Plans are ghosts until confirmed [C4 low].

```
 ── Step 1: draw the line ───────────────────────────────────────────────────
     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░         ghost dam: green = buildable,
     ░░░  river  ▓▓▓▓▓▓▓▓▓▓  ░░░░░░         red segments = blocked
     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░

 ── Step 2: preview (updates live while you drag or change height) ─────────
     ┌ Dam · 6 cells · height 1.0 ─────────────┐
     │ Pond rises  +0.8     Fills in ~4 days   │
     │ Cost  🪵 12  ⌇ 18  ◍ 9      [−][+] height │
     │ Floods:  🌲 14 trees   ⌂ 1 building ⚠   │
     │ River pushes on dam: strong             │
     │      [Order dam]        [Cancel]        │
     └─────────────────────────────────────────┘
```

Map overlay while the preview is open:

| Overlay | Look |
|---|---|
| Cells that would flood | translucent `WATER_SHALLOW` tint with a dashed `FOAM` outline |
| Trees that would drown | marked with a small rust ring |
| Buildings that would flood | outlined rust with ⚠, also listed in the card |
| Ghost dam | `DAM_MUD` fill, green or red outline |

- Every number on the card is a field of `dam_preview::DamPreview`: `rise`, `fill_days`, `cost` (scaled by length), `flooded_trees`, `flooded_buildings`, `pressure`, and `flooded` for the overlay. `[−][+]` changes `height` (default `DEFAULT_HEIGHT` = 1.0).
- Colours say valid/blocked, and every state also has a word or glyph.
- Submitting creates a construction site the family fells, hauls and builds; the pond appears when it finishes. Building a finished dam can be reversed with Demolish (`BreachDam`).
- Trace: green/red ghost C3 (low); separate plan C4 (low); drawing and right-click cancel C5/C6 (low). **The flood overlay and card are Own**: no researched game documents a flood preview.

## 9. Drag gather and job assignment

```
 Gather tool (Clear group, key G).  Pick what: [🌲Trees][❀Plants][◍Mud][◆Stone]
 ┌ drag a rectangle on the map ┐        beavers nearby take the job:
 │ ┌┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┐          │        marked cells get a pale-yellow tint
 │ ┆ 🌲 🌲  🌲   🌲  ┆   12 🌲  │        and a count; "6 beavers will help"
 │ └┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┘          │
 └─────────────────────────────┘
 Cancel gather tool: drag over the same area to clear it
```

**Orders (map):**
- Drag a rectangle for the chosen kind [5-DF][5-CS] (low). Sim: `Assign { who, to: Area(kind, area) }`.
- Matching removal tool for every area order (C8 medium) [5-DF][5-RW][5-TB].
- Right-click a single tree, plant or bank assigns that one (`Assignment::Resource`). Own, from the sim.
- Area drags are separate from building work areas: they are one-off orders, while a camp's area (section 4) is standing. This is the exclusive-zone vs overlapping-area idea from RimWorld in spirit [5-RW] (low); the exact rules are Own.

**Job assignment (who does it):**

```
 Select beavers → [Gather ▾] chooses a job   or   open a workplace → [ + ] fills a slot
 Beaver card:  🦫 Hazel   Doing: Collect Trees   [Stand down]
```

- Pick beavers by clicking or dragging a box, then assign a job or stand them down (`Assign` with `None`); or fill a slot on a workplace (`FillSlot`). Roles come from the sim: Collect (by kind), Build, Haul, Craft, Study.
- This is the **workplace/slot priority model** (C10 (b)), not the per-worker grid (RimWorld, DF). A work grid is not drawn; the sim has no per-worker priorities.
- "Never" level and tie-break rule (C10, low): not shown until the sim defines them. Open question for the sim owner.

---

## 10. Look (this game, no copied art)

| Part | Rule |
|---|---|
| Surfaces | Soft cream panels (`CUT_WOOD` #F2D9A8 at about 92% opacity) with `INK` #4D3321 text, rounded corners, `BARK` #7A4A2B outlines |
| Accents | Active tab and selected tool in `FUR` #B9783F; valid in `LILY` green; warning in `AUTUMN_RUST`; info in `WATER_SHALLOW` |
| Shadows | Plum shadow (`PLUM` mix, never black), as the world does |
| Icons | Drawn fresh in this game's soft, rounded style (sticks, logs, mud ball, pebbles, berry). The emoji in the wireframes are placeholders only. |
| Type | One rounded sans for all text; numbers in tabular figures |

No art from the researched games is used, and nothing is copied; their conventions are cited only for behaviour.

## 11. Open items

| Item | Needed from |
|---|---|
| Workplace priority levels, "never", tie-break | sim (not present) |
| Pan/rotate defaults, camera click-through on alert | playtest (research gap) |
| Settling site-check rules (the reasons list) | sim: expose why `Settle` rejects a cell |
