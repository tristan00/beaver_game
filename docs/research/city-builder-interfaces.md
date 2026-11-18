# How established city builders and strategy games lay out and operate their interface

Researched 2026-11-17 from web sources fetched during this task (no prior research or spec was consulted).
All sources are community wikis or guides unless marked; none is an in-game check.

**Support levels:** *Strong* = stated on pages I read, with two sources agreeing. *Single* = one page only. *Weak* = secondary guide, old, or stub page. *Gap* = not found.

Games covered (8): Timberborn, RimWorld, Dwarf Fortress, Banished, Anno 1800, Cities: Skylines, Oxygen Not Included, Factorio. Frostpunk was attempted, but both wiki pages refused access (HTTP 401/402), so it is **not** covered.

## 1. Resource bar / status display

| Game | Finding | Support | Source |
|---|---|---|---|
| Timberborn | Main screen has Inventory (science, materials, food, water, badwater; per-good detail with satisfied needs in green), Well-Being average, district focus, calendar/forecast (Temperate blue, Drought yellow, Badtide red), time, population. Screen positions not stated. | Single | [Main Screen](https://timberborn.wiki.gg/wiki/User_Interface/Main_Screen) |
| RimWorld | Resource list top-left, stockpiled items only, scrollable, with a categorized mode. Colonist bar at top. Time controls bottom-right. | Single, stub page | [User interface](https://rimworldwiki.com/wiki/User_interface) |
| Anno 1800 | Top bar: balance and income far left, time and speed centre, map icons far right (Ludo, May 2026). Gamepressure (2025) instead lists empire level, finances, dwellers and stored supplies at upper left. | Weak; **sources differ** (probably version differences) | [Ludo](https://www.ludo.guide/guide/anno-1800/getting-started/game-interface-hud), [Gamepressure](https://www.gamepressure.com/anno-1800/interface/z4b4f8) |
| Cities: Skylines | Demand bars (green residential, blue commercial, orange industrial) at the bottom, slightly left of centre. | Single (wiki "verified for 1.16") | [Zoning](https://skylines.paradoxwikis.com/Zoning) |
| Banished | Not documented in the guide read. | Gap | [Gamepressure](https://www.gamepressure.com/banished/interface/z06082) |

Pattern: Timberborn and Anno 1800 (Ludo) show resource or finance figures together with time; RimWorld has a resource list top-left but its time controls are bottom-right. Timberborn (green for satisfied needs) and Cities: Skylines (coloured demand bars) use colour to show need or demand state.

## 2. Build menu

- **Timberborn**: toolbar of tool groups (Select, Cut trees, Plant crops, Plant trees, Demolish, Assign priorities, Paths and Structures, Landscaping, Storage, Housing, Water, Food, Wood, Metal, Power, Science and others). Submenus "display centered above the Toolbar". Ctrl+scroll cycles groups, Shift+scroll cycles tools in a submenu. Single source, page marked a stub. [Category:Toolbar](https://timberborn.wiki.gg/wiki/Category:Toolbar)
- **RimWorld**: Architect menu, bottom-left, with tabs Orders, Zone/Area, Structure, Production, Furniture, Power, Security, Misc, Floors and more. Strong (two wiki pages agree). [Architect](https://rimworldwiki.com/wiki/Architect), [User interface](https://rimworldwiki.com/wiki/User_interface)
- **Banished**: category icons lower right (Tools & Reports, Housing, Roads & Bridges, Storage/Market/Trade, Town Services, Food Production, Resource Production, Removal, Options); windows can be dragged. Weak (single guide, 2016). [Gamepressure](https://www.gamepressure.com/banished/interface/z06082)
- **Anno 1800**: hammer icon at the far left of the bottom bar opens the build menu, with category tabs; hotkey B. Weak/Single. [Ludo](https://www.ludo.guide/guide/anno-1800/getting-started/game-interface-hud)
- **Cities: Skylines**: the Zoning page lists zoning tools in the toolbar ([Zoning](https://skylines.paradoxwikis.com/Zoning)); hotkeys are **not covered** (the wiki's shortcut pages returned no usable content).
- **Dwarf Fortress**: modal menus by letter key (d designations, p stockpiles, i zones). Single. [Designations menu](https://dwarffortresswiki.org/Designations_menu)
- **Oxygen Not Included**: build menu page returned 404. **Gap**.

Pattern: Timberborn, RimWorld, Banished and Anno 1800 have categorised build menus (RimWorld bottom-left, Banished lower right, Anno at the left of the bottom bar). Anno opens it with B and Dwarf Fortress menus open by letter key; only Timberborn is documented with keyboard cycling of groups and tools.

## 3. Placement ghosts and previews

- **Timberborn**: blueprint is translucent; preview green when valid, red when blocked. Needs flat dry ground and path access. Single source (timberborn.org guide, updated 04/08/2026). The wiki says "Duplicate" (B) places multiple copies. Whether Shift keeps the tool active is **not established**. [Placement guide](https://timberborn.org/articles/construction-building-placement-guide), [Actions](https://timberborn.wiki.gg/wiki/User_Interface/Actions)
- **Factorio**: Shift+LMB builds ghosts; ghosts have no range limit; player-placed ghosts last forever; R rotates, H/V flip blueprints; blueprints can be placed from the map. Strong (several wiki pages). [Controls](https://wiki.factorio.com/Controls), [Ghost](https://wiki.factorio.com/Ghost), [Blueprint](https://wiki.factorio.com/Blueprint)
- **Cities: Skylines**: roads snap to roads, angles, the zoning grid and guidelines; click start, click end; right-click cancels; PageUp/PageDown change height. The zoning grid appears beside a road once built. Single wiki; part of the fetch was model-summarised. [Roads](https://skylines.paradoxwikis.com/Roads)
- **Dwarf Fortress**: blueprint mode (M) shows designations cyan instead of beige; dwarves ignore them until converted (k). Single. [Designations menu](https://dwarffortresswiki.org/Designations_menu)
- **RimWorld**: the Plan tool draws plans colonists do not act on. Ghost and rotation behaviour **not documented** on pages read. [Architect](https://rimworldwiki.com/wiki/Architect)

Pattern: Timberborn's preview is green when valid and red when blocked (single source). Factorio ghosts, the DF blueprint mode and the RimWorld Plan tool each let the player mark work without it being acted on at once.

## 4. Selection panels

- **Timberborn**: object panel with header actions (demolish, copy settings, duplicate, focus, close), pause, name, district, description, and for workplaces worker count/type/list, recipe, productivity, workplace priority (5 levels), hauler priority. Single, detailed. [Object Detail](https://timberborn.wiki.gg/wiki/User_Interface/Object_Detail)
- **RimWorld**: inspect pane bottom-left (shares the corner with Architect and cell info); shows a count when several same-type objects are selected; stockpile settings via a button above the Architect menu. Single, stub page. [User interface](https://rimworldwiki.com/wiki/User_interface), [Zone/Area](https://rimworldwiki.com/wiki/Zone/Area)
- **Anno 1800**: info panel location **disagrees**: bottom-left (Ludo) vs right side (Gamepressure). Weak.

## 5. Area drag orders

- **Dwarf Fortress**: click first tile, drag, click second to make a rectangle; paint mode (B) marks tiles singly; designations span z-levels as a cuboid; x removes designations; priority 1 (highest) to 7, default 4. Single page; its version banner conflicts (v53.17 vs "v50 information"), so details may vary by version. [Designations menu](https://dwarffortresswiki.org/Designations_menu)
- **Timberborn**: tools mark/unmark areas for lumberjacks, farmers and foresters; Demolish clears rubble, crops, trees. Further drag mechanics **not documented** on pages read. Path drawing by drag is **not established**. [Category:Toolbar](https://timberborn.wiki.gg/wiki/Category:Toolbar)
- **RimWorld**: Orders (Mine, Chop Wood, Harvest, Haul, Hunt and others) and zone tools (stockpile, growing, allowed area, home, roof). Zones are contiguous and cannot overlap; areas are overlapping layers and each has a clear tool; shrink zone works square by square. Strong on taxonomy; drag/Shift/Ctrl behaviour is a **Gap**. [Zone/Area](https://rimworldwiki.com/wiki/Zone/Area), [Architect](https://rimworldwiki.com/wiki/Architect)
- **Cities: Skylines**: zoning offers fill, marquee and brush; 8×8 m cells, zonable up to four cells from a road. The page says "four basic tools" but lists three; unresolved. [Zoning](https://skylines.paradoxwikis.com/Zoning)

## 6. Job assignment and priorities

| Game | Model | Support | Source |
|---|---|---|---|
| RimWorld | Work tab (F1) grid of colonist by work type. Simple mode: assigned or not. Manual mode: priority 1 to 4, blank = never; ties go to the left column. Colonists work only within their allowed area. Shift chains directed tasks. Page is a stub; its mode-icon description is ambiguous. | Single | [Work](https://rimworldwiki.com/wiki/Work) |
| Dwarf Fortress | Labors enabled per dwarf; designations carry a required labor; per-designation priority 1 to 7. | Single | [Designations menu](https://dwarffortresswiki.org/Designations_menu) |
| Oxygen Not Included | Priority panel (L), priority overlay (P); default priority 5 cannot be changed; levels very high to disabled; sub-priority 1 to 9; "Enable Proximity" option; Red/Yellow alert. | Single | [Priority](https://oxygennotincluded.wiki.gg/wiki/Priority) |
| Timberborn | Per-workplace priority (5 levels) when workers are scarce; construction-site priority (5 levels). Tie-break order between competing sites is **not established** (the Building page returned HTTP 403). | Single | [Object Detail](https://timberborn.wiki.gg/wiki/User_Interface/Object_Detail) |
| Banished | Under Tools & Reports, assign villagers to professions and set priorities; detail page not read. | Weak | [Gamepressure](https://www.gamepressure.com/banished/interface/z06082) |

Pattern: three models. (a) per-worker settings (RimWorld work grid, DF labors), (b) priorities on jobs or buildings (Timberborn workplaces and construction sites, DF designations, ONI priority levels), (c) profession assignment (Banished, weak). Documented tie-breaks: RimWorld (left column) and ONI ("Enable Proximity" option); Timberborn's is not established and DF's is not stated.

## 7. Camera

Weakly covered. Anno 1800: minimap bottom-right, zoom with +/- or wheel, padlock locks the camera to a selection (Ludo, single). Factorio: wheel zooms, Tab/M toggles the map ([Controls](https://wiki.factorio.com/Controls)). Timberborn: "Focus" centres a selected object ([Object Detail](https://timberborn.wiki.gg/wiki/User_Interface/Object_Detail)). RimWorld: clicking a colonist in the top bar jumps to them ([User interface](https://rimworldwiki.com/wiki/User_interface)). Pan and rotate defaults, and DF z-levels, were not sourced. **Gap** for most games.

## 8. Notifications

- **Timberborn**: Alerts list issues with counts in brackets; selecting one cycles through targets; a Journal logs recent changes. Single. [Main Screen](https://timberborn.wiki.gg/wiki/User_Interface/Main_Screen)
- **Factorio**: alerts beside the quickbar, lasting 5 to 30 s by type; per-type toggles via a button above the minimap (added 2.1.18). Single, current. [Alerts](https://wiki.factorio.com/Alerts)
- **RimWorld**: alerts and suggestions top-right; letters not covered. Single, stub. [User interface](https://rimworldwiki.com/wiki/User_interface)
- **Anno 1800**: left side shows quests and recent notifications; Ludo adds a bell icon on the right of the top bar (only in one source). [Ludo](https://www.ludo.guide/guide/anno-1800/getting-started/game-interface-hud), [Gamepressure](https://www.gamepressure.com/anno-1800/interface/z4b4f8)
- **Cities: Skylines (Chirper)**, **ONI diagnostics**, **DF announcements**: not obtained.

## Conventions to follow

Each is a recommendation derived from the findings above and traced to the games whose findings state it. See `city-builder-interfaces-trace.md` for the sentence-by-finding table.

1. **Show stockpile or finance figures together with time, and use colour for need or demand state.** Anno 1800 (balance and time in the top bar), Timberborn (inventory, time, green satisfied needs), RimWorld (resource list top-left, time controls bottom-right), Cities: Skylines (coloured demand bars). Confidence: medium.
2. **Offer a categorised build menu.** Timberborn, RimWorld (Architect), Anno 1800 (hammer, B), Banished (category icons), Cities: Skylines (zoning tools in toolbar). Keyboard tool cycling is documented only for Timberborn; hotkeys for Cities: Skylines are not sourced. Confidence: medium for categories, low for hotkeys.
3. **Show a placement preview that is green when valid and red when blocked; let the player mark work without committing it.** Timberborn (green/red, single source), Factorio (ghosts), Dwarf Fortress (blueprint mode), RimWorld (Plan). Confidence: medium for plan-vs-commit (three games), low for the colours (one source).
4. **Right-click cancels an in-progress placement; a duplicate action places multiple copies.** Cities: Skylines (right-click cancel), Timberborn (Duplicate). Confidence: low.
5. **Snap to the grid and to existing structures while placing.** Cities: Skylines. Confidence: low (one game).
6. **Selecting an object opens a panel with its settings, including per-building worker priority; multi-select shows counts.** Timberborn (Object Detail), RimWorld (inspect pane counts). Confidence: medium. Panel position is not a convention: Anno 1800 sources disagree.
7. **Area orders are drag-selected, with a matching clear tool.** Dwarf Fortress (rectangle drag, x removes), Cities: Skylines (marquee, fill, brush for zoning), RimWorld (a clear tool for each area, shrink zone), Timberborn (mark/unmark areas; drag mechanics not documented). Confidence: low-medium; the drag itself is documented only for DF and Cities: Skylines.
8. **Distinguish exclusive zones from overlapping area layers, and label which a tool makes.** RimWorld. Confidence: low (one game, but clearly specified).
9. **Priority is a small numeric range; an explicit "never" and a documented tie-break are worth providing.** Ranges: RimWorld (1 to 4), Dwarf Fortress (1 to 7), ONI (very high to disabled, sub-priority 1 to 9), Timberborn (5 levels). "Never": RimWorld (blank), ONI (disabled). Tie-break: RimWorld (left column), ONI (proximity option). Confidence: medium.
10. **Alerts are a counted list with a log.** Timberborn (counts, cycling through targets, Journal), Factorio (timed alerts, per-type toggles), RimWorld (alerts top-right), Anno 1800 (notification list). Confidence: medium; camera centring on an alert target is not established.
11. **Provide a "focus selected" camera action and an overview map.** Timberborn (Focus), Anno 1800 (minimap, lock), Factorio (map toggle). Confidence: low-medium; pan/zoom defaults unsourced.

## Limits of this study

- All evidence is from wikis and guides; I did not play the games. Several pages are stubs or dated (Banished 2016, ONI overlay hotkeys 2024).
- Not obtained: Frostpunk entirely; Banished resource bar, notifications and camera; RimWorld drag, ghost, rotation, letters and camera; DF stockpile, zone and announcement pages (wiki returned 429); Cities: Skylines notifications and camera; ONI build menu, camera and diagnostics; Timberborn camera and drag mechanics; PC Gamer's Anno 1800 article (truncated).
- Every claim above links to a page that was opened. Cities: Skylines hotkeys and the Timberborn construction tie-break order are not established.
