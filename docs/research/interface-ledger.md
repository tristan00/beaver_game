# Interface ledger: what each convention states

Source: `docs/research/city-builder-interfaces.md`, section "Conventions to follow", items 1 to 12 (C1 to C12). Each is quoted verbatim, as a block quote, including its confidence level. Finding IDs such as [1-TB] refer to that file's findings; see `city-builder-interfaces-trace.md`.

## Conventions (verbatim)

### C1

> 1. **Keep stockpile or finance figures on the main screen.** Timberborn (Inventory) [1-TB], RimWorld (resource list top-left) [1-RW], Anno 1800 (balance and income in the top bar) [1-AN]. Their position differs between games, and only Anno 1800 (Ludo) shows them in the same bar as the time [1-AN, 1-RW, 1-TB]. Colour has been used to mark need state (Timberborn: satisfied needs shown in green) [1-TB] and to tell demand types apart (Cities: Skylines demand bars) [1-CS]. Confidence: medium for figures on the main screen (three games); low for the use of colour (two games, each using it differently).

### C2

> 2. **Offer a categorised build menu.** Timberborn (tool groups) [2-TB], RimWorld (Architect tabs) [2-RW], Anno 1800 (hammer, category tabs) [2-AN], Banished (category icons) [2-BA]. Anno opens it with B [2-AN] and Dwarf Fortress menus open by letter key [2-DF]; Cities: Skylines hotkeys are not sourced [2-CS]. Cycling groups and tools with Ctrl/Shift+scroll is documented only for Timberborn [2-TB]. Confidence: medium for categories (four games, RimWorld Strong); low for hotkeys (two games) and for scroll cycling (one game).

### C3

> 3. **Show a placement preview that is green when valid and red when blocked.** Timberborn [3-TB]. Confidence: low (one game, one source).

### C4

> 4. **Let the player place plans separate from real orders.** Dwarf Fortress blueprints are ignored by dwarves until converted with k [3-DF]; RimWorld plans are not acted on by colonists [3-RW]. Factorio is not counted: its finding says ghosts are placed with Shift+LMB and have no range limit, not that they are kept apart from orders or left unbuilt [3-FA]. Confidence: low (two games).

### C5

> 5. **Right-click cancels an in-progress road placement.** Cities: Skylines [3-CS]. **A Duplicate action places multiple copies.** Timberborn (B) [3-TB]. Confidence: low (one game each).

### C6

> 6. **When drawing roads, snap to existing roads, angles, the zoning grid and guidelines.** Cities: Skylines [3-CS]. Confidence: low (one game).

### C7

> 7. **Selecting a building opens a panel with its settings, including its worker priority.** Timberborn (Object Detail, workplace priority in 5 levels) [4-TB]. **Selecting several objects of one type shows their count.** RimWorld (inspect pane) [4-RW]. Confidence: low (one game each). Panel position is not a convention: RimWorld uses bottom-left [4-RW] and the Anno 1800 sources disagree [4-AN].

### C8

> 8. **Area orders are drag-selected.** Dwarf Fortress (click, drag, click to make a rectangle) [5-DF], Cities: Skylines (marquee zoning tool) [5-CS]. **Each area order has a matching removal tool.** Dwarf Fortress (x removes designations) [5-DF], RimWorld (a clear tool for each area) [5-RW], Timberborn (mark/unmark areas) [5-TB]. Confidence: low for drag-selection (two games); medium for removal tools (three games).

### C9

> 9. **Distinguish exclusive zones from overlapping area layers.** RimWorld: zones are contiguous and cannot overlap; areas are overlapping layers [5-RW]. Confidence: low (one game, though its finding is Strong on taxonomy).

### C10

> 10. **Priority is a small set of levels.** RimWorld (1 to 4) [6-RW], Dwarf Fortress (1 to 7) [6-DF], ONI ("very high" to "very low", sub-priority 1 to 9) [6-ONI], Timberborn (5 levels) [6-TB]. **An explicit "never" level and a stated tie-break exist in two games.** "Never": RimWorld (blank) [6-RW], ONI ("disabled") [6-ONI]. Tie-break: RimWorld (left column) [6-RW], ONI (hidden fixed priority, or closest task with "Enable Proximity") [6-ONI]. Confidence: medium for small level sets (four games); low for "never" and tie-break (two games each).

### C11

> 11. **Show alerts on the main screen.** Timberborn (Alerts) [8-TB], Factorio (beside the quickbar) [8-FA], RimWorld (top-right) [8-RW], Anno 1800 (left side, recent notifications) [8-AN]. Only Timberborn's finding says the alerts form a list [8-TB]; the Factorio, RimWorld and Anno 1800 findings give a position but not a layout. Counts per alert, cycling through targets and a Journal log are documented only for Timberborn [8-TB]; timed alerts and per-type toggles only for Factorio [8-FA]. Confidence: medium for alerts on the main screen (four games); low for a list layout (one game) and for counts, cycling, log, timing and toggles (one game each). Whether selecting an alert centres the camera is not established.

### C12

> 12. **Provide a camera action that moves to a selected object.** Timberborn (Focus) [7-TB], Anno 1800 (padlock locks the camera to a selection) [7-AN], RimWorld (click a colonist in the top bar to jump to them) [7-RW]. **Provide an overview map.** Anno 1800 (minimap) [7-AN], Factorio (Tab/M map toggle) [7-FA]. Confidence: medium for moving to a selection (three games); low for the overview map (two games). Pan and rotate defaults are not sourced.

## Element map

Rule: an element is *supported* only where the quoted text states that behaviour itself. Otherwise it is a *gap*; the nearest convention is named only as context.

| Element | Status | Convention / reason |
|---|---|---|
| Resource bar | supported | C1: figures "on the main screen" (stockpile or finance). |
| Build menu | supported | C2: "Offer a categorised build menu." |
| Selection panel | supported | C7: "Selecting a building opens a panel with its settings". |
| Notifications | supported | C11: "Show alerts on the main screen." Whether selecting an alert centres the camera is not established. |
| Camera: pan | gap | C12 says "Pan and rotate defaults are not sourced." |
| Camera: zoom | gap | No convention mentions zoom. |
| Camera: move to selection | supported | C12: "Provide a camera action that moves to a selected object." |
| Camera: overview | supported | C12: "Provide an overview map." (low confidence) |
| Settling-spot choice | gap | No convention mentions choosing a start or settling location. |
| Building placement | supported | C3: placement preview green when valid, red when blocked (low confidence, one game). |
| Dam flood preview | gap | C3 states a valid/blocked placement preview only; nothing about water, floods or dams. |
| Drag gather | gap | C8 states area orders are drag-selected, but never names gathering; the quote does not state this behaviour itself. |
| Job assignment | gap | C7 states a building's worker priority and C10 a small set of priority levels, but neither states assigning jobs or workers to tasks. |

## Gaps

Pan, zoom, settling-spot choice, dam flood preview, drag gather, job assignment. The research file also lists unobtained sources (camera for Timberborn, RimWorld, Cities: Skylines, ONI; drag mechanics for Timberborn, RimWorld) under "Limits of this study".
