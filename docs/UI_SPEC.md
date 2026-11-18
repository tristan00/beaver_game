# Beaver Colony: UI spec

A conventional city-builder screen, dressed in this game's own look. Layout follows `docs/research/UI_CONVENTIONS.md`; every choice below names the convention it follows (**C1-C12**, numbered as in that report's "Conventions to adopt"). Where the research has no source, the row says **none** and why. Image generator: `python tools/make_ui_wireframe.py` (needs Pillow; run from the repo root).

| Wireframe (1920x1080) | Shows |
|---|---|
| `docs/ui_wireframe.png` | Full HUD, a burrow selected |
| `docs/ui_wireframe_dam.png` | Dam preview: build menu open, water rise, confirm / cancel |
| `docs/ui_wireframe_settle.png` | Settle choice |
| `docs/ui_wireframe_jobs.png` | Drag to gather, and the job panel |

Research evidence is mostly graded partial; C-numbers are the report's adopted conventions, not claims about any one game's exact layout.

## 1. HUD layout (`ui_wireframe.png`)
| Zone | Where | Holds | Follows |
|---|---|---|---|
| Z1 Clock and needs | top-left | season icon, day, five need meters | **C1** (resource info top-left, RimWorld); clock grouped with it: **none** |
| Z2 Stores ribbon | top-centre | 8 main goods, icon and count; hover for the rest; click opens the full list | **C1**, **C12** |
| Z3 Speed | top-right | pause, 1x, 2x, 3x | **C4** (buttons plus Space, 1, 2, 3) |
| Z4 Alerts | left, under Z1 | up to 4, newest first, "+n" for more; click flies the camera | **C5** |
| Z5 Inspector / tool panel | right, under Z3, 368 px wide | the selected thing; during a tool, that tool's panel; empty when neither | **C3** (panel only on selection; right side so it never meets build) |
| Z6 Toolbar | bottom-centre | Build, Gather, Haul, Jobs, Demolish, Screens | **C2** (toolbar, submenu opens above it), **C8** (one key per tool) |
| Z7 Map modes | bottom-left | Water, Forest, Food, Work; toggles overlays and zones | **C9** |
| Minimap | none | omitted | **none**: research found no source (its gap list); revisit if the map outgrows one screen |

Always-on zones cover about 20% of the frame with Z5 open; the middle 1100x700 stays clear. Camera input works over any empty map. Screens (Colony, Stores, Buildings, Tech, Events) open centred, one at a time, from **Screens** (**C6**: population overview as a centred screen); Esc closes.

HUD scale 100-125% and tooltip position (fixed or by cursor) live in settings: **C11**.

## 2. Build menu (`ui_wireframe_dam.png`, lower centre)
Build opens a panel above Z6 (**C2**): a row of category tabs, then one tile per building under the open tab (icon, name below). Tabs are the tech branches already in the game (Basics, Food, Wood, Water, Craft); locked buildings are dimmed with the reason on hover. Choosing a tile arms the placement ghost (section 5). B opens it, Esc closes it (**C8**).

## 3. Selection panel (Z5, `ui_wireframe.png`)
Building first: name and one-line state; Needs, Makes, Work area rows; worker slots (a tile per beaver, open slots amber); actions Assign, Move area, Demolish, Locate. Click a slot beaver to select that beaver; a beaver's panel leads with its workplace. **Follows C3** (panel appears on selection) and **C6** (per-building worker slots, as in Timberborn's detail panel and Cities: Skylines' workplace line). Hover a number for its parts (own rule, **none**).

## 4. Job panel (`ui_wireframe_jobs.png`)
Opens centred with J or from the toolbar (**C6**, **C7**). One row per job (Logger, Forager, Builder, Digger, Hauler, Crafter): worker count with - and +, and a priority of **off**, 1, 2, 3, 4 (**C7**: numeric priority with a disabled state, from ONI). Below, a strip of beavers with their jobs; idle ones in amber. Idle adults take the highest-priority open job. The map stays visible behind it.

## 5. Opening interactions
Every step uses the Pond mark for the active thing and Rail-outlined map text.

| # | Interaction | Behaviour | Follows |
|---|---|---|---|
| 1 | **Settle choice** (`ui_wireframe_settle.png`) | Time starts paused. Three candidate banks are outlined on the map with a square marker and a name. Clicking one turns its outline Pond and fills the settle card (Z6 slot): one line of facts, three rows (wood, forage, flood risk), **Settle here** (primary) and **Cancel** (pick another). Settling unpauses and shows the Build menu with Burrow first. | **C10** (one-click placement), **C3** (panel on selection); paused start and card: **none** |
| 2 | **Placement ghost** | A chosen building follows the cursor as a white outline; invalid shows a Berry hatch and a one-line reason. Left click places it as a site; Shift keeps the tool; right click or Esc cancels. The grid shows only while placing. Nothing is ever placed automatically. | **C10** (AoE2 grid, Anno 1800 single-click blueprint) |
| 3 | **Dam water preview** (`ui_wireframe_dam.png`) | Dam: drag a line on the map, ends have square handles. While dragging and after, the map shows the new shoreline as a dotted Pond line, flooded land as a hatched Pond tint at 30% (hatch, not a flat fill, so it reads against the river), and an Amber triangle on every tree and building that would flood. The Z5 tool panel lists Water rises, Floods, Trees flooded (Amber when above 0), Buildings flooded, Cost, Pond fills in, and a Height - / + stepper. **Confirm** (Enter, primary) lays the site; **Cancel** (Esc, right click) discards. Nothing is built until Confirm. | **C10** (ghost before commit); confirm / cancel pair: **none**, own rule: no sim change without a visible preview |
| 4 | **Drag to gather** (`ui_wireframe_jobs.png`) | G arms Gather (or the toolbar button). Drag on the map draws a dashed Pond rectangle with square handles and a count tag ("12 trees, 2 beavers will gather"). Releasing assigns the selected beavers, or idle adults if none are selected, and drops a flag; Esc or right click before release cancels. The area stays selectable and can be moved or resized by its handles. | **C8** (one key per area tool); **C9** (areas stay selectable) |
| 5 | **Job assignment** | Three ways, all live: the job panel (section 4); the slots in a building's inspector (click an open slot, then a beaver); and a beaver's own job buttons. A change shows at once in the job panel counts. | **C6**, **C7** |

## 6. Map feedback
- Hover: cell outline and tooltip after 0.3 s. Selection: Pond ring; work area as a dashed rectangle with handles.
- Orders: a flag at the target and a line from the beaver for 1 s. Progress: a segment meter over a site. Gains: floating "+3 logs".
- Alerts: a marker at the place; an edge arrow when it is off screen (shown in `ui_wireframe.png`); clicking in Z4 flies there (**C5**).
- Markers are 24 px or larger, Rail-outlined, and differ by shape as well as colour: square info, triangle warning, octagon urgent.

## 7. Input
| Input | Does |
|---|---|
| Left click | select; place; confirm |
| Right click | order selected beavers; cancel tool, ghost or drag |
| Left drag, empty map | orbit camera; with Gather or Move area, draw an area |
| Wheel / Q E / middle drag | zoom / rotate / pan |
| Space, 1 2 3 | pause, speed (**C4**) |
| B, G, H, J, X | Build, Gather, Haul, Jobs, Demolish (**C8**); shown on the buttons |
| Shift while dragging | constrain to a straight line (**C8**) |
| Esc | one step back: ghost, tool, screen, selection |
| Enter | confirm the focused decision (dam, settle, join request) |

## 8. The game's own look
Identity: a lodge built of cut timber. This section is unchanged from the previous spec and is the only owner of interface styling (`docs/STYLE.md` covers the world only). **No other game's art, icons, fonts or assets are used**: every icon is a file in `assets/icons/` drawn for this game, the font is Atkinson Hyperlegible (SIL OFL 1.1, `assets/fonts/OFL.txt`), and the wireframes are generated from them by `tools/make_ui_wireframe.py`. The research report copied nothing from other games and this spec adds nothing.

| Part | Spec |
|---|---|
| Colours | Bark `#3F2E24` panel; Raised `#4D3A2D` rows and buttons; Rail `#2A1F19` outlines and shadow; Line `#5B4636` dividers; Pith `#F1E3C6` text; Dim `#BFA88A` secondary; **Pond `#5FC7D6` active**; Moss `#8DBF5A` good; Amber `#F0A93B` warning; Berry `#E0556B` danger |
| Shapes | Rectangles; meters are rows of square segments with 4 px gaps; no circles or pills |
| Corners | gnawed: 8 px chamfer on top-left and bottom-right of panels, 6 px on buttons |
| Elevation | hard 3 px offset shadow in Rail |
| Buttons | 44 px tall; icon then label; active has a 4 px Pond bar along the bottom; primary is Moss with Rail text |
| Type | 14 px body, 16 px titles, 13 px minimum; Regular and Bold; sentence case |
| Spacing | 8 px grid; panel padding 16; row 40 |
| Contrast | text 4.5:1 or better, markers 3:1; panels at least 95% opaque |

## 9. Current UI files: replaced and removed
Paths under `crates/game/src/`. **Keep** = stays as is. **Replace** = the file is rewritten for this spec (same name unless a new name is given). **Remove** = deleted, its useful parts moved as stated.

| File | Fate | Why |
|---|---|---|
| `ui/kit.rs`, `ui/kit_tests.rs`, `ui/icons.rs`, `ui/explain.rs` | Keep | widget kit, icon loader, number provenance already match section 8 |
| `ui/screens.rs`, `ui/gestures.rs`, `ui/gestures_tests.rs`, `ui/mapmodes.rs`, `ui/mapmodes_tests.rs`, `ui/events.rs` | Keep | pure view-models and gesture logic; only screen list and key names change |
| `ui/status.rs` | **Replace** by `ui/topbar.rs` | Z1, Z2, Z3 in one top strip (speed is 1x, 2x, 3x, not 1x, 3x, 10x) |
| `ui/buildbar.rs` | **Replace** by `ui/toolbar.rs` | today it is the six-screen navigation bar; Z6 becomes tools, and screens move under **Screens** |
| `ui/inspector.rs` | **Replace** | Z5 layout in section 3 and the tool-panel mode |
| `ui/selection.rs` | **Replace** | view-model gains worker slots and workplace-first beaver panel (currently a stub) |
| `ui/modebar.rs` | **Replace** | keeps the four mode buttons only; legends and action buttons move to Build and Gather |
| `ui/alerts.rs` | **Replace** | Z4 list with the shapes of section 6 |
| `ui/panels.rs` | **Replace** | draws only the centred screens and the job panel; per-zone click handling moves to each zone |
| `ui/shell.rs` | **Replace** | navigation state loses the bottom bar; gains tool state (Build, Gather, Haul, Jobs, Demolish) |
| `ui/mod.rs` | Replace | plugin list updated |
| `build.rs` (1264 lines) | **Replace** by `ui/buildmenu.rs` (menu), `ui/placement.rs` (ghost, dam preview, confirm / cancel), `ui/jobs.rs` (job panel and per-beaver buttons); `build_logic.rs` stays | one monolith held menu, ghost, dam readout and job buttons |
| `flow.rs` opening parts (Arrival, Scouting, `settle`), `startflow.rs` | **Replace** by `ui/settle.rs` | settle choice of section 5 row 1; it also fixes the restart path, which today falls back to the old scouting flow |
| `map_orders.rs`, `map_overlay.rs` | Replace (edit) | Gather area and flooded-tile hatch drawing |
| `ui/orders.rs` | **Remove** | empty stub; nothing calls it |
| `opening.rs` | **Remove** | auto-settle helper for old scenes; no longer a way to start |
| Bottom navigation bar (Colony / Work / Stores / Buildings / Tech / Events buttons) | **Remove** | replaced by **Screens**; Work becomes the Jobs panel |
| Per-beaver job button row inside the build panel (`build.rs` `BAction::SetJob`) | **Remove** | moved into the job panel and the beaver card |
| Speed button 10x | **Remove** | three speeds only |

Sim change needed, not UI: candidate landing banks for the settle choice (today the camp is fixed in `crates/sim/src/starts.rs`).
