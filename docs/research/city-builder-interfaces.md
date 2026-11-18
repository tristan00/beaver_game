# How established city builders and strategy games lay out and operate their interface

Researched 2026-11-17 from web sources fetched during this task (no prior research or spec was consulted).
All sources are community wikis or guides unless marked; none is an in-game check.

**Support levels (findings):** *Strong* = stated on pages I read, with two sources agreeing. *Single* = one page only. *Weak* = secondary guide, old, or stub page. *Gap* = not found.

**Confidence (conventions):** *medium* = findings for three or more games state it, at least one of them Single or Strong. *low* = findings for only one or two games state it, or all of them are Weak. No convention reaches *high*.

Each finding has an ID in brackets (for example `[1-TB]` = section 1, Timberborn). The pattern summaries and conventions cite these IDs, and `city-builder-interfaces-trace.md` maps every sentence to them.

Games covered (8): Timberborn (TB), RimWorld (RW), Dwarf Fortress (DF), Banished (BA), Anno 1800 (AN), Cities: Skylines (CS), Oxygen Not Included (ONI), Factorio (FA). Frostpunk was attempted, but both wiki pages refused access (HTTP 401/402), so it is **not** covered.

## 1. Resource bar / status display

| ID | Game | Finding | Support | Source |
|---|---|---|---|---|
| 1-TB | Timberborn | Main screen has Inventory (science, materials, food, water, badwater; per-good detail with satisfied needs in green), Well-Being average, district focus, calendar/forecast (Temperate blue, Drought yellow, Badtide red), time, population. Screen positions not stated. | Single | [Main Screen](https://timberborn.wiki.gg/wiki/User_Interface/Main_Screen) |
| 1-RW | RimWorld | Resource list top-left, stockpiled items only, scrollable, with a categorized mode. Colonist bar at top. Time controls bottom-right. | Single, stub page | [User interface](https://rimworldwiki.com/wiki/User_interface) |
| 1-AN | Anno 1800 | Top bar: coin balance and per-minute income far left, game time and speed controls centre (P pauses, 1/2/3 set speed), map icons far right (Ludo, May 2026). Gamepressure (2025) instead lists empire level, finances, dwellers and stored supplies at upper left. | Weak; **sources differ** (probably version differences) | [Ludo](https://www.ludo.guide/guide/anno-1800/getting-started/game-interface-hud), [Gamepressure](https://www.gamepressure.com/anno-1800/interface/z4b4f8) |
| 1-CS | Cities: Skylines | Demand bars (green residential, blue commercial, orange industrial) at the bottom, slightly left of centre. | Single (wiki "verified for 1.16") | [Zoning](https://skylines.paradoxwikis.com/Zoning) |
| 1-BA | Banished | Not documented in the guide read. | Gap | [Gamepressure](https://www.gamepressure.com/banished/interface/z06082) |

Pattern: Timberborn [1-TB], RimWorld [1-RW] and Anno 1800 [1-AN] keep stockpile or finance figures on the main screen. Only Anno 1800 (Ludo) puts them in the same bar as the time [1-AN]; RimWorld puts the resource list top-left and the time controls bottom-right [1-RW], and Timberborn's positions are not stated [1-TB]. Timberborn colours satisfied needs green and colours forecast seasons [1-TB]; Cities: Skylines colours its demand bars by zone type [1-CS].

## 2. Build menu

- **[2-TB] Timberborn**: toolbar of tool groups (Select, Cut trees, Plant crops, Plant trees, Demolish, Assign priorities, Paths and Structures, Landscaping, Storage, Housing, Water, Food, Wood, Metal, Power, Science and others). Submenus "display centered above the Toolbar". Ctrl+scroll cycles groups, Shift+scroll cycles tools in a submenu. Single source, page marked a stub. [Category:Toolbar](https://timberborn.wiki.gg/wiki/Category:Toolbar)
- **[2-RW] RimWorld**: Architect menu, bottom-left, with tabs Orders, Zone/Area, Structure, Production, Furniture, Power, Security, Misc, Floors and more. Strong (two wiki pages agree). [Architect](https://rimworldwiki.com/wiki/Architect), [User interface](https://rimworldwiki.com/wiki/User_interface)
- **[2-BA] Banished**: category icons lower right (Tools & Reports, Housing, Roads & Bridges, Storage/Market/Trade, Town Services, Food Production, Resource Production, Removal, Options); windows can be dragged. Weak (single guide, 2016). [Gamepressure](https://www.gamepressure.com/banished/interface/z06082)
- **[2-AN] Anno 1800**: hammer icon at the far left of the bottom bar opens the build menu, with category tabs; hotkey B. Weak/Single. [Ludo](https://www.ludo.guide/guide/anno-1800/getting-started/game-interface-hud)
- **[2-CS] Cities: Skylines**: the Zoning page lists zoning tools in the toolbar ([Zoning](https://skylines.paradoxwikis.com/Zoning)); hotkeys are **not covered** (the wiki's shortcut pages returned no usable content).
- **[2-DF] Dwarf Fortress**: modal menus by letter key (d designations, p stockpiles, i zones). Single. [Designations menu](https://dwarffortresswiki.org/Designations_menu)
- **ONI**: build menu page returned 404. **Gap**.

Pattern: Timberborn [2-TB], RimWorld [2-RW], Banished [2-BA] and Anno 1800 [2-AN] have categorised build menus, in different places (RimWorld bottom-left, Banished lower right, Anno at the left of the bottom bar). Anno opens its menu with B [2-AN] and Dwarf Fortress menus open by letter key [2-DF]. Only Timberborn is documented with cycling of groups and tools, using Ctrl+scroll and Shift+scroll [2-TB].

## 3. Placement ghosts and previews

- **[3-TB] Timberborn**: blueprint is translucent; preview green when valid, red when blocked. Needs flat dry ground and path access. Single source (timberborn.org guide, updated 04/08/2026). The wiki says "Duplicate" (B) places multiple copies. Whether Shift keeps the tool active is **not established**. [Placement guide](https://timberborn.org/articles/construction-building-placement-guide), [Actions](https://timberborn.wiki.gg/wiki/User_Interface/Actions)
- **[3-FA] Factorio**: Shift+LMB builds ghosts; ghosts have no range limit; player-placed ghosts last forever; R rotates, H/V flip blueprints; blueprints can be placed from the map. Strong (several wiki pages). [Controls](https://wiki.factorio.com/Controls), [Ghost](https://wiki.factorio.com/Ghost), [Blueprint](https://wiki.factorio.com/Blueprint)
- **[3-CS] Cities: Skylines**: roads snap to roads, angles, the zoning grid and guidelines; click start, click end; right-click cancels; PageUp/PageDown change height. The zoning grid appears beside a road once built. Single wiki; part of the fetch was model-summarised. [Roads](https://skylines.paradoxwikis.com/Roads)
- **[3-DF] Dwarf Fortress**: blueprint mode (M) shows designations cyan instead of beige; dwarves ignore them until converted (k). Single. [Designations menu](https://dwarffortresswiki.org/Designations_menu)
- **[3-RW] RimWorld**: the Plan tool draws plans colonists do not act on. Ghost and rotation behaviour **not documented** on pages read. [Architect](https://rimworldwiki.com/wiki/Architect)

Pattern: Timberborn's preview is green when valid and red when blocked (single source) [3-TB]. Dwarf Fortress blueprints [3-DF] and RimWorld plans [3-RW] mark work that workers ignore; Factorio offers a separate ghost placement (Shift+LMB) that has no range limit [3-FA].

## 4. Selection panels

- **[4-TB] Timberborn**: object panel with header actions (demolish, copy settings, duplicate, focus, close), pause, name, district, description, and for workplaces worker count/type/list, recipe, productivity, workplace priority (5 levels), hauler priority. Single, detailed. [Object Detail](https://timberborn.wiki.gg/wiki/User_Interface/Object_Detail)
- **[4-RW] RimWorld**: inspect pane bottom-left (shares the corner with Architect and cell info); shows a count when several same-type objects are selected; stockpile settings via a button above the Architect menu. Single, stub page. [User interface](https://rimworldwiki.com/wiki/User_interface), [Zone/Area](https://rimworldwiki.com/wiki/Zone/Area)
- **[4-AN] Anno 1800**: the panel for a selected building appears bottom-left according to Ludo, but on the right according to Gamepressure. **Sources disagree**. Weak. [Ludo](https://www.ludo.guide/guide/anno-1800/getting-started/game-interface-hud), [Gamepressure](https://www.gamepressure.com/anno-1800/interface/z4b4f8)

## 5. Area drag orders

- **[5-DF] Dwarf Fortress**: click first tile, drag, click second to make a rectangle; paint mode (B) marks tiles singly; designations span z-levels as a cuboid; x removes designations; priority 1 (highest) to 7, default 4. Single page; its version banner conflicts (v53.17 vs "v50 information"), so details may vary by version. [Designations menu](https://dwarffortresswiki.org/Designations_menu)
- **[5-TB] Timberborn**: tools mark/unmark areas for lumberjacks, farmers and foresters; Demolish clears rubble, crops, trees. Further drag mechanics **not documented** on pages read. Path drawing by drag is **not established**. [Category:Toolbar](https://timberborn.wiki.gg/wiki/Category:Toolbar)
- **[5-RW] RimWorld**: Orders (Mine, Chop Wood, Harvest, Haul, Hunt and others) and zone tools (stockpile, growing, allowed area, home, roof). Zones are contiguous and cannot overlap; areas are overlapping layers and each has a clear tool; shrink zone works square by square. Strong on taxonomy; drag/Shift/Ctrl behaviour is a **Gap**. [Zone/Area](https://rimworldwiki.com/wiki/Zone/Area), [Architect](https://rimworldwiki.com/wiki/Architect)
- **[5-CS] Cities: Skylines**: zoning offers fill, marquee and brush; 8×8 m cells, zonable up to four cells from a road. The page says "four basic tools" but lists three; unresolved. [Zoning](https://skylines.paradoxwikis.com/Zoning)

## 6. Job assignment and priorities

| ID | Game | Model | Support | Source |
|---|---|---|---|---|
| 6-RW | RimWorld | Work tab (F1) grid of colonist by work type. Simple mode: assigned or not. Manual mode: priority 1 to 4, blank = never; ties go to the left column. Colonists work only within their allowed area. Shift chains directed tasks. Page is a stub; its mode-icon description is ambiguous. | Single | [Work](https://rimworldwiki.com/wiki/Work) |
| 6-DF | Dwarf Fortress | Labors enabled per dwarf; designations carry a required labor; per-designation priority 1 to 7. | Single | [Designations menu](https://dwarffortresswiki.org/Designations_menu) |
| 6-ONI | Oxygen Not Included | Priority panel (L), priority overlay (P); default priority is always 5 and cannot be changed; levels run from "very high" to "very low", and tasks set to "disabled" are never done; sub-priority 9 is taken first, 1 last. Between tasks with the same priority and sub-priority, a hidden fixed priority decides, unless "Enable Proximity" (priority panel options) is on, in which case the duplicant takes the closest. Red/Yellow alert. | Single (page edited 7 May 2026) | [Priority](https://oxygennotincluded.wiki.gg/wiki/Priority) |
| 6-TB | Timberborn | Per-workplace priority (5 levels) when workers are scarce; construction-site priority (5 levels). Tie-break order between competing sites is **not established**. | Single | [Object Detail](https://timberborn.wiki.gg/wiki/User_Interface/Object_Detail) |
| 6-BA | Banished | Under Tools & Reports, assign villagers to professions and set priorities. | Weak | [Gamepressure](https://www.gamepressure.com/banished/interface/z06082) |

Pattern: three models. (a) per-worker settings: RimWorld work grid [6-RW], DF labors [6-DF]. (b) priorities on jobs or buildings: Timberborn workplaces and construction sites [6-TB], DF designations [6-DF], ONI priority levels [6-ONI]. (c) profession assignment: Banished, weak [6-BA]. Documented tie-breaks: RimWorld takes the left column [6-RW]; ONI uses a hidden fixed priority, or the closest task when "Enable Proximity" is on [6-ONI]. Timberborn's is not established [6-TB] and DF's is not stated [6-DF].

## 7. Camera

Weakly covered.

- **[7-AN] Anno 1800**: minimap bottom-right; zoom with the + and - buttons or the mouse wheel; a padlock icon locks the camera to a selected ship or building. Weak (single guide). [Ludo](https://www.ludo.guide/guide/anno-1800/getting-started/game-interface-hud)
- **[7-FA] Factorio**: wheel zooms; Tab/M toggles the map. Single. [Controls](https://wiki.factorio.com/Controls)
- **[7-TB] Timberborn**: "Focus" in the object panel centres the camera on the selected object. Single. [Object Detail](https://timberborn.wiki.gg/wiki/User_Interface/Object_Detail)
- **[7-RW] RimWorld**: clicking a colonist in the top bar jumps to them. Single, stub. [User interface](https://rimworldwiki.com/wiki/User_interface)
- Pan and rotate defaults, and DF z-levels, were not sourced. **Gap** for most games.

## 8. Notifications

- **[8-TB] Timberborn**: Alerts list issues with counts in brackets; selecting one cycles through targets; a Journal logs recent changes. Single. [Main Screen](https://timberborn.wiki.gg/wiki/User_Interface/Main_Screen)
- **[8-FA] Factorio**: alerts beside the quickbar, lasting 5 to 30 s by type; per-type toggles via a button above the minimap (added 2.1.18). Single, current. [Alerts](https://wiki.factorio.com/Alerts)
- **[8-RW] RimWorld**: alerts and suggestions top-right; letters not covered. Single, stub. [User interface](https://rimworldwiki.com/wiki/User_interface)
- **[8-AN] Anno 1800**: left side shows quests and recent notifications; Ludo adds a bell icon on the right of the top bar (only in one source). Weak. [Ludo](https://www.ludo.guide/guide/anno-1800/getting-started/game-interface-hud), [Gamepressure](https://www.gamepressure.com/anno-1800/interface/z4b4f8)
- **Cities: Skylines (Chirper)**, **ONI diagnostics**, **DF announcements**: not obtained.

## Conventions to follow

Each convention is drawn from the findings above. Every sentence cites the finding IDs that state it; confidence follows the rule at the top. See `city-builder-interfaces-trace.md` for the sentence-by-finding table.

1. **Keep stockpile or finance figures on the main screen.** Timberborn (Inventory) [1-TB], RimWorld (resource list top-left) [1-RW], Anno 1800 (balance and income in the top bar) [1-AN]. Their position differs between games, and only Anno 1800 (Ludo) shows them in the same bar as the time [1-AN, 1-RW, 1-TB]. Colour has been used to mark need state (Timberborn: satisfied needs shown in green) [1-TB] and to tell demand types apart (Cities: Skylines demand bars) [1-CS]. Confidence: medium for figures on the main screen (three games); low for the use of colour (two games, each using it differently).
2. **Offer a categorised build menu.** Timberborn (tool groups) [2-TB], RimWorld (Architect tabs) [2-RW], Anno 1800 (hammer, category tabs) [2-AN], Banished (category icons) [2-BA]. Anno opens it with B [2-AN] and Dwarf Fortress menus open by letter key [2-DF]; Cities: Skylines hotkeys are not sourced [2-CS]. Cycling groups and tools with Ctrl/Shift+scroll is documented only for Timberborn [2-TB]. Confidence: medium for categories (four games, RimWorld Strong); low for hotkeys (two games) and for scroll cycling (one game).
3. **Show a placement preview that is green when valid and red when blocked.** Timberborn [3-TB]. Confidence: low (one game, one source).
4. **Let the player place plans separate from real orders.** Dwarf Fortress blueprints are ignored by dwarves until converted with k [3-DF]; RimWorld plans are not acted on by colonists [3-RW]. Factorio is not counted: its finding says ghosts are placed with Shift+LMB and have no range limit, not that they are kept apart from orders or left unbuilt [3-FA]. Confidence: low (two games).
5. **Right-click cancels an in-progress road placement.** Cities: Skylines [3-CS]. **A Duplicate action places multiple copies.** Timberborn (B) [3-TB]. Confidence: low (one game each).
6. **When drawing roads, snap to existing roads, angles, the zoning grid and guidelines.** Cities: Skylines [3-CS]. Confidence: low (one game).
7. **Selecting a building opens a panel with its settings, including its worker priority.** Timberborn (Object Detail, workplace priority in 5 levels) [4-TB]. **Selecting several objects of one type shows their count.** RimWorld (inspect pane) [4-RW]. Confidence: low (one game each). Panel position is not a convention: RimWorld uses bottom-left [4-RW] and the Anno 1800 sources disagree [4-AN].
8. **Area orders are drag-selected.** Dwarf Fortress (click, drag, click to make a rectangle) [5-DF], Cities: Skylines (marquee zoning tool) [5-CS]. **Each area order has a matching removal tool.** Dwarf Fortress (x removes designations) [5-DF], RimWorld (a clear tool for each area) [5-RW], Timberborn (mark/unmark areas) [5-TB]. Confidence: low for drag-selection (two games); medium for removal tools (three games).
9. **Distinguish exclusive zones from overlapping area layers.** RimWorld: zones are contiguous and cannot overlap; areas are overlapping layers [5-RW]. Confidence: low (one game, though its finding is Strong on taxonomy).
10. **Priority is a small set of levels.** RimWorld (1 to 4) [6-RW], Dwarf Fortress (1 to 7) [6-DF], ONI ("very high" to "very low", sub-priority 1 to 9) [6-ONI], Timberborn (5 levels) [6-TB]. **An explicit "never" level and a stated tie-break exist in two games.** "Never": RimWorld (blank) [6-RW], ONI ("disabled") [6-ONI]. Tie-break: RimWorld (left column) [6-RW], ONI (hidden fixed priority, or closest task with "Enable Proximity") [6-ONI]. Confidence: medium for small level sets (four games); low for "never" and tie-break (two games each).
11. **Show alerts on the main screen.** Timberborn (Alerts) [8-TB], Factorio (beside the quickbar) [8-FA], RimWorld (top-right) [8-RW], Anno 1800 (left side, recent notifications) [8-AN]. Only Timberborn's finding says the alerts form a list [8-TB]; the Factorio, RimWorld and Anno 1800 findings give a position but not a layout. Counts per alert, cycling through targets and a Journal log are documented only for Timberborn [8-TB]; timed alerts and per-type toggles only for Factorio [8-FA]. Confidence: medium for alerts on the main screen (four games); low for a list layout (one game) and for counts, cycling, log, timing and toggles (one game each). Whether selecting an alert centres the camera is not established.
12. **Provide a camera action that moves to a selected object.** Timberborn (Focus) [7-TB], Anno 1800 (padlock locks the camera to a selection) [7-AN], RimWorld (click a colonist in the top bar to jump to them) [7-RW]. **Provide an overview map.** Anno 1800 (minimap) [7-AN], Factorio (Tab/M map toggle) [7-FA]. Confidence: medium for moving to a selection (three games); low for the overview map (two games). Pan and rotate defaults are not sourced.

## Limits of this study

- All evidence is from wikis and guides; I did not play the games. Several pages are stubs or dated (Banished 2016, ONI overlay hotkeys 2024).
- Not obtained: Frostpunk entirely; Banished resource bar, notifications and camera; RimWorld drag, ghost, rotation, letters and camera; DF stockpile, zone and announcement pages (wiki returned 429); Cities: Skylines notifications and camera; ONI build menu, camera and diagnostics; Timberborn camera and drag mechanics.
- Every finding above links to a page that was opened. Cities: Skylines hotkeys and the Timberborn construction tie-break order are not established.
