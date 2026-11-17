# Beaver Colony: UI spec

Short contract for the interface, written before UI code. It is derived from what a colony builder asks of its screen, not from any one game: the player watches a living place, decides where to build, then checks whether it works. Wireframe: `docs/ui_wireframe.png` (1920x1080, regenerate with `python tools/make_ui_wireframe.py`).

**Supersedes** `docs/STYLE.md` §3 (UI corner radii) and §6 (UI style): the world palette, terrain and lighting there stay; the cream card and pill look does not. `docs/DESIGN.md` §11 still defines the screens and view-models.

## 1. Fixed zones
Every element lives in one named zone and never moves. Players learn where to look once. Always-on zones cover **11%** of a 1920x1080 frame, **21%** with the inspector open (Z5).

| Zone | Where | Holds |
|---|---|---|
| Z1 Clock and needs | top-left | season icon, day, five need meters (food, shelter, rest, mood, company) |
| Z2 Stores ribbon | top-centre | the 8 goods the colony uses most, icon and count; hover for the rest |
| Z3 Speed | top-right | pause, 1x, 2x, 3x |
| Z4 Alerts | left, under Z1 | up to 4 alerts, newest first; extras collapse into "+n" |
| Z5 Inspector | right, under Z3 | the selected thing; absent when nothing is selected; height fits content |
| Z6 Build and orders | bottom-centre | Build, Assign, Move area, Haul, Demolish, Screens |
| Z7 Map modes | bottom-left | Water, Forest, Food, Work |

Screens (Colony, Work, Stores, Buildings, Tech, Events) open centred, one at a time, over the map (layer L3, §4).

## 2. Map as hero
The map is never resized or scrolled by the interface. No zone is wider than 880 px; the middle 1100x700 px is always clear except for on-demand screens. Camera input always works over empty map. Opening a screen dims the map by 25% rather than hiding it, and pointer-over-map keeps hover feedback alive.

## 3. Building first
The colony is run through its **buildings** and work places, not beaver by beaver. Selecting a building opens the Inspector with, in order: name and one-line state, what it needs and makes, worker slots (a beaver per slot, click to reassign), work area, then actions. A beaver is reached from its slot; a beaver's own panel leads with the building it works at. The build tool is one click in Z6, and every placed thing starts as a site with the same inspector.

## 4. Layered detail
Each layer adds only what the player asked for.

| Layer | Trigger | Shows |
|---|---|---|
| L0 glance | always | icons and numbers in Z1, Z2, Z3; markers on the map |
| L1 hover | pointer rests 0.3 s | tooltip: name, one state line, key numbers |
| L2 inspect | click | Z5 inspector |
| L3 compare | Screens button or key | dense, sortable tables for many things at once (stores, jobs, families) |
| L4 why | hover a number in L2/L3 | the parts that add up to it, with sources (`ui/explain.rs`) |

Every figure the sim knows that bears on a decision is reachable by L4.

## 5. One widget kit
Identity: a **lodge built of cut timber**. Dark bark planks with gnawed corners, a pond-coloured waterline marking what is active, a world of soft blobs beneath them. None of it is the cream panel (`#FCF2DE`) or a pill button.

| Part | Spec |
|---|---|
| Colours | Bark `#3F2E24` panel; Raised `#4D3A2D` rows and buttons; Rail `#2A1F19` outlines and shadow; Line `#5B4636` dividers; Pith `#F1E3C6` text; Dim `#BFA88A` secondary text; **Pond `#5FC7D6` active and selected**; Moss `#8DBF5A` good; Amber `#F0A93B` warning; Berry `#E0556B` danger. Pith on Bark is 11:1; Dim on Bark above 6:1. |
| Shapes | Rectangles. No circles or capsules in controls. Meters are rows of square segments (4 px gaps). |
| Corners | **Gnawed**: 8 px diagonal chamfer on top-left and bottom-right of panels, 6 px on buttons; the other two corners stay square. Icon tiles keep the existing soft 28 px (96 px source) rounding as content, never as a control shape. |
| Elevation | Hard 3 px offset shadow in Rail, no blur. |
| Buttons | 44 px tall, Raised fill, icon left then label; active = 4 px Pond bar along the bottom; primary = Moss fill with Rail text. |
| Type | 14 px body, 16 px titles, 13 px minimum; Regular and Bold only; sentence case. |
| Spacing | 8 px grid; panel padding 16 px, row height 40 px. |

**Font:** Atkinson Hyperlegible, `assets/fonts/AtkinsonHyperlegible-Regular.ttf` and `assets/fonts/AtkinsonHyperlegible-Bold.ttf`, by the Braille Institute of America, licence **SIL Open Font Licence 1.1** (`assets/fonts/OFL.txt`), source `https://github.com/google/fonts/tree/main/ofl/atkinsonhyperlegible`. Chosen because letterforms are built to stay distinct at small size on busy backgrounds. Static files, so Bevy needs no variable-font support.

## 6. Icons before words
Every row, button and marker carries an icon; the word is a caption and may be dropped when narrow. Files are `assets/icons/<name>.png`, 96 px, tint per group as in `docs/STYLE.md` §7. **E** = exists, **N** = new (to be drawn).

| Group | Icons |
|---|---|
| Goods (9) | E logs, branches, bark, plants, mud, stone, baskets, rope, tools. Food uses the needs icon `food`. |
| Needs (5) | E food, shelter, rest, mood, social |
| Jobs (4) | E collector, builder, hauler, specialist |
| Buildings (19, one per `BuildingKind`) | N dam, lodge, burrow, foodcache, scentmound, guardpost, canal, lock, waterwheel, workshop, warehouse, dock, barge, loggingcamp, foragerhut, mudpit, quarrycamp, mill, storehouse |
| Actions (20) | E assign, fillslot, movearea, accept, turnaway, cancel, demolish, collect, carry, build, haul, craft, navcolony, navbeavers, navstores, navbuild, navtech, navevents; N close, locate (show on map) |
| Map modes (4) | E modewater, modeforest, modefood, modework |
| Alerts (8) | N alertfood, alertshelter, alertidle, alertstalled, alertfull, alertjoin, alertwinter, alertwater |
| Seasons (4) | N spring (sprout), summer (sun), autumn (leaf), winter (snowflake) |
| Speed (4) | N pause, play1, play2, play3 |

Totals: 40 exist, 37 new. The `Icon` enum in `ui/icons.rs` gains one variant per new file; the loader test already checks every variant has a PNG.

## 7. Readability over terrain
The map behind is bright green and cyan, so UI is dark and opaque (95%+), never translucent. On-map text (labels, "+3 logs") sits on a Rail stroke 3 px wide. Map markers are drawn at least 24 px with a Rail outline and have a **shape per meaning** so colour is never alone: square = info, triangle = warning, octagon = urgent. Overlays of map modes use Pond, Moss, Amber tints at 40% and keep tree and beaver silhouettes visible. Text contrast at least 4.5:1, markers 3:1.

## 8. Map feedback
Everything the player does or the colony does shows on the map first.
- **Hover:** cell outline plus L1 tooltip. **Selection:** Pond ring under the thing; its work area drawn as a dashed rectangle with drag handles.
- **Placement ghost:** white outline when valid; Berry hatch with a one-line reason when not; a dam also draws its waterline rise and flooded cells.
- **Orders:** a flag at the target and a line from the beaver for 1 s. **Progress:** sites rise, a segment meter floats above.
- **Gains:** floating "+3 logs" at the store. **Alerts:** a marker at the place and an arrow on the screen edge when it is off screen; clicking an alert in Z4 flies the camera there.

## 9. Input conventions
| Input | Does |
|---|---|
| Left click | select; confirm placement; click through to the map when not over UI |
| Right click | order for selected beavers; cancels a tool, placement or drag |
| Left drag on empty map | orbit camera; drag with Assign or Move area tool draws a work area |
| Wheel / Q E | zoom / rotate; middle drag pans |
| Space; 1 2 3 | pause; speed |
| B, A, M | Build, Assign, Move area; keys shown on hover only |
| Esc | one step back: tool, then screen, then selection |
| Enter | accept the focused decision (join request) |

No input needs two hands; every action has a click path.

## 10. Ownership of the UI tasks
Each later UI task edits only its own files; shared state is read through the public view-model APIs. Files marked *new* are created by that task.

| Task | Files |
|---|---|
| `ui_kit` (widget kit, font loading) | `crates/game/src/ui/kit.rs`, `crates/game/src/ui/kit_tests.rs` *new*, `assets/fonts/*` |
| `ui_zones` (Z1 to Z3, Z7, layout) | `crates/game/src/ui/shell.rs`, `crates/game/src/ui/zones.rs` *new*, `crates/game/src/flow_view.rs` |
| `ui_toolbar` (Z6) | `crates/game/src/ui/modebar.rs`, `crates/game/src/ui/orders.rs` |
| `ui_icons` (art and loader) | `assets/icons/*`, `crates/game/src/ui/icons.rs`, `tools/make_icons.py`, `docs/icons_sheet.png` |
| `ui_inspector` (Z5, building first) | `crates/game/src/ui/selection.rs`, `crates/game/src/ui/panels/entity.rs` |
| `ui_screens` (L3 tables, L4 why) | `crates/game/src/ui/panels.rs`, `crates/game/src/ui/screens.rs`, `crates/game/src/ui/explain.rs` |
| `ui_alerts` (Z4, events) | `crates/game/src/ui/events.rs`, `crates/game/src/ui/alerts.rs` *new* |
| `ui_mapmodes` (overlays) | `crates/game/src/ui/mapmodes.rs`, `crates/game/src/map_overlay.rs` |
| `ui_feedback` (ghost, flags, floaters) | `crates/game/src/ui/feedback.rs` *new*, `crates/game/src/map_orders.rs` |
| `ui_input` (gestures, keys) | `crates/game/src/ui/gestures.rs`, `crates/game/src/ui/gestures_tests.rs`, `crates/game/src/input.rs` |
| `ui_menu` (start menu) | `crates/game/src/ui/menu.rs` |
| `ui_spec` (this) | `docs/UI_SPEC.md`, `docs/ui_wireframe.png`, `tools/make_ui_wireframe.py` |

`ui/mod.rs` is touched only to add a `mod` line for a *new* file.
