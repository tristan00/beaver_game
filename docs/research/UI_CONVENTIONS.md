# UI conventions in city builders and strategy games

Research for the Beaver Colony HUD (BEA-38). Compiled 2026-11-17. Companion to `docs/UI_SPEC.md`, which already fixes our zones Z1-Z7; this report checks those choices against other games and lists what to adopt.

## How to read the evidence

Public wikis were the best available sources, but many blocked automated reading (Fandom: HTTP 402; Dwarf Fortress wiki: 429; wiki.gg Anno/Frostpunk: 401; Cities: Skylines 2 wiki: DNS failure). Most readable pages are stubs or key lists, not layout descriptions. So support is graded:

- **Strong**: the page states it in words (quoted or closely paraphrased).
- **Partial**: the page states part of it (a key, an option, a name) but not position.
- **Not found**: no readable source in this session. Listed as a gap, not a claim.

No game art or assets were copied; nothing here is an asset.

Where sources disagree: none contradict each other; they cover different things. The one tension is that RimWorld puts build and inspect bottom-left (source 1) while our spec puts build bottom-centre (UI_SPEC Z6). That is a design choice, not an error; see conventions.

## Findings by game

### 1. RimWorld (strong for HUD corners)
Source: https://rimworldwiki.com/wiki/User_interface (stub page, edited 2026-03-16).
- Resource list: top-left; a "transparent list" of each resource and its quantity in stockpiles.
- Architect (build) menu: bottom-left. The Inspect pane is also bottom-left and "appears whenever the player selects a pawn, item, or structure".
- Time controls: bottom-right; pause, normal, 3x (two triangles), 5x (three triangles); current time also bottom-right.
- Toggles (e.g. zone visibility): bottom-right. Alerts: top-right. Colonist bar: top.
- A zone "can still be selected to display it and configure it" even when hidden.
- Not on page: minimap, work-priority panel, placement preview, drag tools.
Secondary (search-result summary of the same wiki, https://mail.rimworldwiki.com/wiki/User_interface): right-clicking a colonist on the bar jumps to them. Partial support, single source.

### 2. Timberborn (partial; no positions)
Sources: https://timberborn.wiki.gg/wiki/User_Interface/Main_Screen and https://timberborn.wiki.gg/wiki/User_Interface/Actions.
- Named parts: Inventory (Science, Materials, Ingredients, Food, Water, Badwater), expandable via "Inventory Detail"; Toolbar shows tool groups, with submenus "centered above the toolbar"; Object Detail "shows detail on the selected object"; Population Management is "an overview of the entire population"; Time shows time of day and working hours; also Well-being, Calendar and Forecast, Journal, Alerts, Automation Pins, view layers.
- Object actions: Copy Settings, Duplicate, Customize, Focus.
- Not stated: screen corners, speed controls, minimap, placement preview, area-drag tools. Closest genre match to us; the toolbar with centred submenu supports Z6.

### 3. Cities: Skylines (partial; keys and behaviour only)
Sources: https://skylines.paradoxwikis.com/Controls (v1.16); https://skylines.paradoxwikis.com/Citizens; https://en.wikipedia.org/wiki/Cities:_Skylines.
- Speed: "SPACE, 1, 2, 3 = Timeflow speed: paused, normal, faster, fastest".
- Zoning keys 4-9 select zone types, 0 de-zones; Page Up/Down change build elevation.
- Clicking a citizen "will bring up the citizen information window" with residence, workplace, "Current activity"; unemployed citizens are reported as such.
- Roads can be built "straight or free-form"; the zoning grid "adapts to the shape of the adjacent roads".
- Not found: panel positions, ghost colours, minimap.

### 4. Oxygen Not Included (partial; keys and priority model)
Sources: https://oxygennotincluded.wiki.gg/wiki/Controls and https://oxygennotincluded.wiki.gg/wiki/Priority.
- Area tools on single keys: G dig, M mop, Y harvest, P prioritise; Shift = "Drag Straight".
- L opens "Manage Priorities". Levels "very high" to "very low" plus "disabled"; sub-priority 1-9; default 5; a gear icon holds an "Enable Proximity" option.
- Speed: Space pause, Tab cycle speed, Num +/- change speed.
- Not stated: positions, ghosts.

### 5. Age of Empires II: Definitive Edition (partial; options only)
Source: https://www.ageofempires.com/age-ii-de-accessibility/.
- "In Game HUD Scale" slider up to 125%; tooltip position fixed or "floating next to the mouse cursor"; a terrain grid "for building placement"; object highlights Off / Only Selections / Only Hovers / All; a "command panel" exists for the focused unit or building.
- Not found: HUD positions, minimap, drag-select. Layout from memory is deliberately not claimed.

### 6. Anno 1800 (partial)
Source: https://en.wikipedia.org/wiki/Anno_1800.
- Blueprint mode places "silhouetted blueprint buildings" without spending resources; "each blueprint building can be built with a single click".
- Not found: HUD positions, build menu, drag tools (https://anno1800.wiki.gg returned 401).

### 7. Frostpunk (partial)
Source: https://en.wikipedia.org/wiki/Frostpunk.
- "Players construct buildings along a circular grid surrounding the generator." Hope and discontent are meters in the UI (position not stated). "Workers and engineers can be assigned to work at various facilities."

### 8. Banished and Dwarf Fortress (not found)
- Banished: https://www.shiningrocksoftware.com/game/ gives only marketing lines ("Keep an eye on workers and inventory"); Fandom pages blocked (402).
- Dwarf Fortress: https://dwarffortresswiki.org/index.php/Designation was rate-limited (429).
- Nothing is claimed for either. Worth a later pass with a readable source.

## Pattern summary

| Element | Evidence | Support |
|---|---|---|
| Resource bar | Top-left in RimWorld; expandable "Inventory" in Timberborn | Partial (2 games) |
| Build menu | Bottom-left (RimWorld); toolbar with centred submenus (Timberborn) | Partial |
| Selection panel | Pane appears on selection (RimWorld, Timberborn, Cities: Skylines) | Partial; 3 games agree on behaviour |
| Minimap | No readable source | Not found |
| Time controls | Pause + speeds, bottom-right in RimWorld; keys in Cities: Skylines and ONI | Strong for existence, partial for position |
| Job/worker panels | Population overview (Timberborn); numeric priorities (ONI); workplace in citizen window (Cities: Skylines) | Partial |
| Placement ghosts | Blueprint silhouette, one click (Anno 1800); terrain grid (AoE2) | Partial |
| Area-drag orders | One key per tool, Shift for straight drags (ONI); zone keys and de-zone (Cities: Skylines) | Partial |

## Conventions to adopt

Zone names refer to `docs/UI_SPEC.md`. Source in brackets.

1. **Stores as a persistent icon-and-count list along the top edge, hover for the rest.** Top-centre (Z2). [RimWorld; Timberborn Inventory]
2. **Toolbar with a submenu opening above it.** Bottom-centre (Z6): category row, then a centred row of buildings above. [Timberborn; RimWorld Architect]
3. **Selecting opens a panel; nothing selected means no panel.** Right side under the speed controls (Z5), not bottom-left, so it never collides with build. [RimWorld; Cities: Skylines]
4. **Pause and three speeds as buttons, with Space, 1, 2, 3 keys.** Top-right (Z3); keys work anywhere. [RimWorld; Cities: Skylines]
5. **Alerts as a short list; clicking one jumps the camera.** Left, under the clock (Z4). [RimWorld; Timberborn Alerts]
6. **Population overview plus per-building worker slots.** Overview is a centred screen opened from Screens in Z6; slots are in the inspector (Z5). [Timberborn; Cities: Skylines]
7. **Numeric job priority with a hotkey-opened panel, with a "disabled" state.** Centred screen opened by key (L as in ONI). [ONI Priority]
8. **One key per area tool; Shift constrains a drag to a straight line.** Key shown on the Z6 button; the drag happens on the map with the dashed rectangle from UI_SPEC section 8. [ONI Controls]
9. **Zones stay selectable and can be hidden.** Toggle bottom-left with map modes (Z7). [RimWorld]
10. **Placement ghost with a grid hint only while placing; one click places.** On the map under the cursor. [AoE2; Anno 1800]
11. **HUD scale (up to at least 125%) and tooltip position options.** In the settings screen. [AoE2 accessibility]
12. **Expandable stores detail.** Click on the Z2 ribbon opens the full list. [Timberborn Inventory Detail]

## Gaps
- Minimap: no source; our spec has none. Confirm that omission for the map size.
- Banished, Dwarf Fortress, and HUD positions for AoE2, Anno and Cities: Skylines need readable sources (official manuals or reviewed screenshots).
